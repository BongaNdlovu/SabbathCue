//! Shared line-delimited JSON worker process support for local STT providers.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::mpsc;

use crate::error::SttError;
use crate::types::{TranscriptEvent, Word};

const CHECK_READY_TIMEOUT: Duration = Duration::from_secs(20);
const WORKER_MONITOR_INTERVAL: Duration = Duration::from_millis(50);
const PREFLIGHT_CACHE_TTL: Duration = Duration::from_secs(600);

type PreflightCacheKey = (String, PathBuf, PathBuf);
type PreflightCache = Mutex<HashMap<PreflightCacheKey, Instant>>;

static PREFLIGHT_CACHE: OnceLock<PreflightCache> = OnceLock::new();

fn preflight_cache() -> &'static PreflightCache {
    PREFLIGHT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn preflight_cache_hit(label: &str, model_path: &Path, worker_path: &Path) -> bool {
    let Ok(cache) = preflight_cache().lock() else {
        return false;
    };
    let Some(validated_at) = cache.get(&(
        label.to_string(),
        model_path.to_path_buf(),
        worker_path.to_path_buf(),
    )) else {
        return false;
    };
    validated_at.elapsed() < PREFLIGHT_CACHE_TTL
}

fn record_preflight_success(label: &str, model_path: &Path, worker_path: &Path) {
    let Ok(mut cache) = preflight_cache().lock() else {
        return;
    };
    cache.insert(
        (
            label.to_string(),
            model_path.to_path_buf(),
            worker_path.to_path_buf(),
        ),
        Instant::now(),
    );
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum WorkerEvent {
    Ready,
    Partial {
        text: String,
        #[serde(default)]
        words: Vec<WorkerWord>,
    },
    Final {
        text: String,
        #[serde(default)]
        words: Vec<WorkerWord>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkerWord {
    #[serde(alias = "word")]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) start: f64,
    #[serde(default)]
    pub(crate) end: f64,
    #[serde(default)]
    pub(crate) conf: f64,
}

/// Keeps a temporary worker input file on disk until dropped.
pub(crate) struct WorkerTempFile {
    path: PathBuf,
}

impl WorkerTempFile {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkerTempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) struct CancellationGuard {
    cancelled: Arc<AtomicBool>,
}

impl CancellationGuard {
    pub(crate) fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self { cancelled }
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

impl Drop for CancellationGuard {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(crate) struct WorkerProcess {
    label: &'static str,
    child: Option<Child>,
    reader: Option<std::thread::JoinHandle<()>>,
    stderr_reader: Option<std::thread::JoinHandle<()>>,
}

impl WorkerProcess {
    pub(crate) fn new(
        label: &'static str,
        child: Child,
        reader: std::thread::JoinHandle<()>,
        stderr_reader: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            label,
            child: Some(child),
            reader: Some(reader),
            stderr_reader: Some(stderr_reader),
        }
    }

    fn child_mut(&mut self) -> Option<&mut Child> {
        self.child.as_mut()
    }

    /// Terminate the worker process without joining the reader threads.
    ///
    /// Joining can deadlock when a reader is blocked in `blocking_send` on a
    /// full event channel (consumer gone): `start()` would never return. The
    /// reader threads exit on their own once the channel's receivers drop and
    /// their sends begin to fail, so the child is killed here and the threads
    /// are left to wind down instead of being joined.
    pub(crate) fn stop_detached(mut self) {
        self.stop_inner(false);
    }

    fn stop_inner(&mut self, join_threads: bool) {
        if let Some(mut child) = self.child.take() {
            terminate_child(self.label, &mut child);
        }

        if join_threads {
            if let Some(reader) = self.reader.take() {
                if reader.join().is_err() {
                    log::warn!("{} reader thread panicked", self.label);
                }
            }
            if let Some(stderr_reader) = self.stderr_reader.take() {
                if stderr_reader.join().is_err() {
                    log::warn!("{} stderr reader thread panicked", self.label);
                }
            }
        }
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        self.stop_inner(false);
    }
}

pub(crate) fn write_worker_temp_file(
    prefix: &str,
    suffix: &str,
    contents: &str,
) -> Result<WorkerTempFile, SttError> {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile()
        .map_err(|e| {
            SttError::ConnectionFailed(format!("failed to create worker temp file: {e}"))
        })?;
    file.write_all(contents.as_bytes()).map_err(|e| {
        SttError::ConnectionFailed(format!("failed to write worker temp file: {e}"))
    })?;
    file.flush().map_err(|e| {
        SttError::ConnectionFailed(format!("failed to flush worker temp file: {e}"))
    })?;

    let (_file, path) = file.keep().map_err(|e| {
        SttError::ConnectionFailed(format!("failed to persist worker temp file: {e}"))
    })?;
    Ok(WorkerTempFile { path })
}

fn resolve_worker_path(worker_path: &Path) -> PathBuf {
    std::fs::canonicalize(worker_path).map_or_else(
        |_| worker_path.to_path_buf(),
        |path| simplify_model_path(&path),
    )
}

pub(crate) fn worker_command(worker_path: &Path) -> Command {
    // PyInstaller 6.22.1+ onefile bootloaders compare the parent and child
    // executable paths. A path that still contains `..` (as built from
    // CARGO_MANIFEST_DIR / dev_root) fails that check and the worker exits
    // before emitting ready.
    let worker_path = resolve_worker_path(worker_path);
    let mut command = if worker_is_executable(&worker_path) {
        Command::new(&worker_path)
    } else {
        let mut command = Command::new(python_executable());
        command.arg(&worker_path);
        command
    };
    suppress_console_window(&mut command);
    command
}

pub(crate) fn worker_is_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

#[cfg(windows)]
pub(crate) fn suppress_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn suppress_console_window(_command: &mut Command) {}

pub(crate) fn simplify_model_path(model_path: &Path) -> PathBuf {
    let text = model_path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest.to_string());
    }
    model_path.to_path_buf()
}

pub(crate) fn python_executable() -> String {
    std::env::var("SABBATHCUE_PYTHON")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "python".to_string())
}

pub(crate) fn first_nonempty_lines(bytes: &[u8], limit: usize) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(limit)
        .collect::<Vec<_>>()
        .join("\n")
}

fn stderr_suffix(stderr: &str) -> String {
    if stderr.is_empty() {
        String::new()
    } else {
        format!(" Stderr:\n{stderr}")
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Worker preflight has a linear process lifecycle with tightly coupled cleanup"
)]
pub(crate) fn check_ready(
    label: &'static str,
    model_path: &Path,
    worker_path: &Path,
    configure_command: impl FnOnce(&mut Command),
) -> Result<(), SttError> {
    if !model_path.exists() {
        return Err(SttError::ConnectionFailed(format!(
            "{label} model not found: {}",
            model_path.display()
        )));
    }
    if !worker_path.exists() {
        return Err(SttError::ConnectionFailed(format!(
            "{label} worker not found: {}",
            worker_path.display()
        )));
    }

    if preflight_cache_hit(label, model_path, worker_path) {
        log::info!(
            "{label} preflight skipped; worker and model validated recently (model={}, worker={})",
            model_path.display(),
            worker_path.display()
        );
        return Ok(());
    }

    let mut command = worker_command(worker_path);
    configure_command(&mut command);
    command.stdin(Stdio::null());
    let started_at = Instant::now();
    log::info!(
        "Checking {label} worker readiness: model={}, worker={}",
        model_path.display(),
        worker_path.display()
    );
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| SttError::ConnectionFailed(format!("failed to start {label} worker: {e}")))?;

    let stdout = child.stdout.take().ok_or_else(|| {
        terminate_child(label, &mut child);
        SttError::ConnectionFailed(format!("failed to open {label} preflight stdout"))
    })?;
    let (event_tx, event_rx) = std::sync::mpsc::channel::<WorkerEvent>();
    let reader = std::thread::Builder::new()
        .name(format!("{}-preflight-reader", label.to_ascii_lowercase()))
        .spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(event) = serde_json::from_str::<WorkerEvent>(&line) {
                    let _ = event_tx.send(event);
                }
            }
        })
        .map_err(|e| {
            terminate_child(label, &mut child);
            SttError::ConnectionFailed(format!("failed to spawn {label} preflight reader: {e}"))
        })?;

    let mut ready = false;
    let deadline = started_at + CHECK_READY_TIMEOUT;
    loop {
        match event_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(WorkerEvent::Ready) => {
                ready = true;
                break;
            }
            Ok(WorkerEvent::Error { message }) => {
                terminate_child(label, &mut child);
                let _ = reader.join();
                return Err(SttError::ConnectionFailed(message));
            }
            Ok(WorkerEvent::Partial { .. } | WorkerEvent::Final { .. })
            | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if let Ok(Some(status)) = child.try_wait() {
            let stderr = child.stderr.take().map_or_else(String::new, |stderr| {
                let mut bytes = Vec::new();
                let _ = BufReader::new(stderr).read_to_end(&mut bytes);
                first_nonempty_lines(&bytes, 6)
            });
            let _ = reader.join();
            return Err(SttError::ConnectionFailed(format!(
                "{label} worker preflight exited before ready with status {status}.{}",
                stderr_suffix(&stderr)
            )));
        }

        if Instant::now() >= deadline {
            terminate_child(label, &mut child);
            let _ = reader.join();
            return Err(SttError::ConnectionFailed(format!(
                "{label} worker did not report ready within {} seconds",
                CHECK_READY_TIMEOUT.as_secs()
            )));
        }
    }

    let stderr = if ready {
        String::new()
    } else {
        child.stderr.take().map_or_else(String::new, |stderr| {
            let mut bytes = Vec::new();
            let _ = BufReader::new(stderr).read_to_end(&mut bytes);
            first_nonempty_lines(&bytes, 6)
        })
    };
    terminate_child(label, &mut child);
    let _ = reader.join();
    if ready {
        log::info!(
            "{label} worker readiness check passed in {}ms",
            started_at.elapsed().as_millis()
        );
        record_preflight_success(label, model_path, worker_path);
        Ok(())
    } else {
        Err(SttError::ConnectionFailed(format!(
            "{label} worker exited without reporting ready.{}",
            stderr_suffix(&stderr)
        )))
    }
}

pub(crate) fn spawn_reader(
    label: &'static str,
    stdout: impl std::io::Read + Send + 'static,
    reader_tx: mpsc::Sender<TranscriptEvent>,
) -> Result<std::thread::JoinHandle<()>, SttError> {
    std::thread::Builder::new()
        .name(format!("{}-worker-reader", label.to_ascii_lowercase()))
        .spawn(move || {
            let lines = BufReader::new(stdout).lines();
            for line in lines {
                let Ok(line) = line else { break };
                let Ok(event) = serde_json::from_str::<WorkerEvent>(&line) else {
                    continue;
                };
                match event {
                    WorkerEvent::Ready => {
                        let _ = reader_tx.blocking_send(TranscriptEvent::Connected);
                    }
                    WorkerEvent::Partial { text, words } if !text.trim().is_empty() => {
                        let _ = reader_tx.blocking_send(TranscriptEvent::Partial {
                            transcript: text,
                            words: to_words(words),
                        });
                    }
                    WorkerEvent::Final { text, words } if !text.trim().is_empty() => {
                        let words = to_words(words);
                        let confidence = average_confidence(&words);
                        let _ = reader_tx.blocking_send(TranscriptEvent::Final {
                            transcript: text,
                            words,
                            confidence,
                            speech_final: true,
                        });
                        let _ = reader_tx.blocking_send(TranscriptEvent::UtteranceEnd);
                    }
                    WorkerEvent::Error { message } => {
                        let _ = reader_tx.blocking_send(TranscriptEvent::Error(message));
                    }
                    WorkerEvent::Partial { .. } | WorkerEvent::Final { .. } => {}
                }
            }
        })
        .map_err(|e| SttError::ConnectionFailed(format!("failed to spawn {label} reader: {e}")))
}

pub(crate) fn spawn_stderr_logger(
    label: &'static str,
    stderr: ChildStderr,
) -> Result<std::thread::JoinHandle<()>, SttError> {
    std::thread::Builder::new()
        .name(format!("{}-worker-stderr", label.to_ascii_lowercase()))
        .spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        log::info!("[{}] {line}", label.to_ascii_uppercase());
                    }
                    Ok(_) => {}
                    Err(error) => {
                        log::warn!("{label} stderr reader failed: {error}");
                        break;
                    }
                }
            }
        })
        .map_err(|e| {
            SttError::ConnectionFailed(format!("failed to spawn {label} stderr reader: {e}"))
        })
}

pub(crate) fn write_samples(
    label: &'static str,
    stdin: &mut ChildStdin,
    samples: &[i16],
) -> Result<(), SttError> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    stdin
        .write_all(&bytes)
        .map_err(|e| SttError::SendError(format!("{label} worker write failed: {e}")))
}

pub(crate) async fn stop_writer(
    label: &'static str,
    writer: tokio::task::JoinHandle<Result<(), SttError>>,
) {
    match tokio::time::timeout(Duration::from_secs(2), writer).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(e))) => log::warn!("{label} writer exited with error: {e}"),
        Ok(Err(e)) => log::warn!("{label} writer task failed: {e}"),
        Err(_) => {
            log::warn!("{label} writer did not stop promptly; terminating worker process");
        }
    }
}

pub(crate) async fn wait_for_worker_shutdown(
    label: &'static str,
    cancelled: &AtomicBool,
    process: &mut WorkerProcess,
    writer: &tokio::task::JoinHandle<Result<(), SttError>>,
) -> Result<(), SttError> {
    loop {
        if cancelled.load(Ordering::SeqCst) || writer.is_finished() {
            return Ok(());
        }

        let Some(child) = process.child_mut() else {
            return Ok(());
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(SttError::ConnectionFailed(format!(
                    "{label} worker exited unexpectedly with status {status}"
                )));
            }
            Ok(None) => {}
            Err(e) => {
                return Err(SttError::ConnectionFailed(format!(
                    "failed to inspect {label} worker process: {e}"
                )));
            }
        }

        tokio::time::sleep(WORKER_MONITOR_INTERVAL).await;
    }
}

pub(crate) fn terminate_child(label: &'static str, child: &mut Child) {
    match child.try_wait() {
        Ok(Some(_status)) => {}
        Ok(None) => {
            if let Err(e) = child.kill() {
                log::warn!("Failed to terminate {label} worker process: {e}");
            }
        }
        Err(e) => log::warn!("Failed to inspect {label} worker process: {e}"),
    }
    if let Err(e) = child.wait() {
        log::warn!("Failed to reap {label} worker process: {e}");
    }
}

pub(crate) fn to_words(words: Vec<WorkerWord>) -> Vec<Word> {
    words
        .into_iter()
        .map(|word| Word {
            text: word.text.clone(),
            start: word.start,
            end: word.end,
            confidence: word.conf,
            punctuated_word: Some(word.text),
        })
        .collect()
}

pub(crate) fn average_confidence(words: &[Word]) -> f64 {
    let mut sum = 0.0;
    let mut count = 0_u32;
    for word in words.iter().filter(|word| word.confidence > 0.0) {
        sum += word.confidence;
        count += 1;
    }
    if count == 0 {
        0.75
    } else {
        sum / f64::from(count)
    }
}

#[cfg(test)]
pub(crate) fn collect_command_args(command: &Command) -> Vec<String> {
    command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_command_resolves_parent_dir_segments_in_exe_path() {
        let temp = tempfile::tempdir().expect("temp dir");
        let nested = temp.path().join("nested");
        std::fs::create_dir_all(&nested).expect("nested dir");
        let exe = nested.join("worker.exe");
        std::fs::write(&exe, b"stub").expect("stub exe");

        let dotted = temp
            .path()
            .join("nested")
            .join("..")
            .join("nested")
            .join("worker.exe");
        let command = worker_command(&dotted);
        let program = command.get_program().to_string_lossy();

        assert!(
            !program.as_ref().contains(".."),
            "worker exe path must not contain '..' (PyInstaller 6.22.1+ parent check), got {program}"
        );
        assert!(
            program.ends_with("worker.exe"),
            "resolved program must still be the worker exe, got {program}"
        );
        assert!(
            !program.contains(r"\\?\"),
            "resolved program must not keep the Windows extended-length prefix, got {program}"
        );
    }

    #[test]
    fn worker_command_runs_exe_directly_and_py_through_python() {
        let exe_command = worker_command(Path::new("C:/app/scripts/worker.exe"));
        assert!(
            exe_command
                .get_program()
                .to_string_lossy()
                .ends_with("worker.exe"),
            "bundled .exe worker must run directly"
        );
        assert_eq!(exe_command.get_args().count(), 0);

        let py_command = worker_command(Path::new("C:/app/scripts/worker.py"));
        let program = py_command.get_program().to_string_lossy().into_owned();
        assert!(
            !program.to_ascii_lowercase().ends_with(".py"),
            ".py worker must run through a Python interpreter, got program: {program}"
        );
        let args = collect_command_args(&py_command);
        assert!(
            args.iter().any(|arg| arg.ends_with("worker.py")),
            "python invocation must pass the script path"
        );
    }

    #[test]
    fn simplify_model_path_handles_unc_and_plain_paths() {
        assert_eq!(
            simplify_model_path(Path::new(r"\\?\UNC\server\share\model")),
            PathBuf::from(r"\\server\share\model")
        );
        assert_eq!(
            simplify_model_path(Path::new(r"C:\models\local")),
            PathBuf::from(r"C:\models\local")
        );
        assert_eq!(
            simplify_model_path(Path::new("/opt/models/local")),
            PathBuf::from("/opt/models/local")
        );
    }

    #[test]
    fn to_words_maps_worker_fields() {
        let words = to_words(vec![WorkerWord {
            text: "genesis".to_string(),
            start: 1.5,
            end: 2.0,
            conf: 0.9,
        }]);

        assert_eq!(words[0].punctuated_word.as_deref(), Some("genesis"));
    }

    #[test]
    fn average_confidence_defaults_when_unscored() {
        assert!((average_confidence(&[]) - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn average_confidence_uses_scored_words() {
        let scored = to_words(vec![
            WorkerWord {
                text: "john".to_string(),
                start: 0.0,
                end: 0.5,
                conf: 0.8,
            },
            WorkerWord {
                text: "three".to_string(),
                start: 0.5,
                end: 1.0,
                conf: 0.6,
            },
        ]);

        assert!((average_confidence(&scored) - 0.7).abs() < 1e-9);
    }

    #[test]
    fn reader_maps_worker_events_to_transcript_events() {
        let stdout: &[u8] = concat!(
            "{\"type\": \"ready\"}\n",
            "not json at all\n",
            "{\"type\": \"partial\", \"text\": \"  \"}\n",
            "{\"type\": \"partial\", \"text\": \"john three\", \"words\": []}\n",
            "{\"type\": \"final\", \"text\": \"john three sixteen\", \"words\": [{\"word\": \"john\", \"start\": 0.0, \"end\": 0.4, \"conf\": 0.9}]}\n",
            "{\"type\": \"error\", \"message\": \"model exploded\"}\n",
        )
        .as_bytes();

        let (tx, mut rx) = mpsc::channel(16);
        let reader = spawn_reader("Test", stdout, tx).expect("reader thread");
        reader.join().expect("reader thread must not panic");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        assert!(matches!(events[0], TranscriptEvent::Connected));
        assert!(matches!(events[1], TranscriptEvent::Partial { .. }));
        assert!(matches!(events[2], TranscriptEvent::Final { .. }));
        assert!(matches!(events[3], TranscriptEvent::UtteranceEnd));
        assert!(matches!(events[4], TranscriptEvent::Error(_)));
        assert_eq!(
            events.len(),
            5,
            "blank partials and bad JSON must be skipped"
        );
    }
}
