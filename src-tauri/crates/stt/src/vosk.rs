//! Offline Vosk STT provider.
//!
//! The provider streams 16 kHz mono PCM to a small worker process. Production
//! builds can bundle the worker as a self-contained executable; development
//! builds can still run the Python script directly.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use tokio::sync::mpsc;

use crate::error::SttError;
use crate::keyterms::verse_only_keyterms;
use crate::provider::SttProvider;
use crate::types::TranscriptEvent;
use crate::worker::{self, WorkerTempFile};

const VOSK_LABEL: &str = "Vosk";
const VOSK_CHUNK_SAMPLES: usize = 1_280;

#[derive(Debug)]
pub struct VoskProvider {
    model_path: PathBuf,
    worker_path: PathBuf,
    cancelled: Arc<AtomicBool>,
}

impl VoskProvider {
    pub fn new(model_path: PathBuf, worker_path: PathBuf) -> Self {
        Self {
            model_path,
            worker_path,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn check_ready(&self) -> Result<(), SttError> {
        let grammar_json = vosk_grammar_json()?;
        let grammar_file = write_grammar_temp_file(&grammar_json)?;
        worker::check_ready(VOSK_LABEL, &self.model_path, &self.worker_path, |command| {
            push_worker_args(command, &self.model_path);
            push_grammar_file_args(command, grammar_file.path());
        })
    }

    fn spawn_worker(&self, grammar_file: &WorkerTempFile) -> Result<Child, SttError> {
        if !self.model_path.exists() {
            return Err(SttError::ConnectionFailed(format!(
                "Vosk model not found: {}",
                self.model_path.display()
            )));
        }
        if !self.worker_path.exists() {
            return Err(SttError::ConnectionFailed(format!(
                "Vosk worker not found: {}",
                self.worker_path.display()
            )));
        }

        let mut command = worker::worker_command(&self.worker_path);
        push_worker_args(&mut command, &self.model_path);
        push_grammar_file_args(&mut command, grammar_file.path());
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SttError::ConnectionFailed(format!("failed to start Vosk worker: {e}")))
    }
}

fn push_grammar_file_args(command: &mut Command, grammar_file: &Path) {
    command.arg("--grammar-json-file").arg(grammar_file);
}

fn push_worker_args(command: &mut Command, model_path: &Path) {
    command
        .arg("--model")
        .arg(worker::simplify_model_path(model_path))
        .arg("--sample-rate")
        .arg("16000");
}

fn write_grammar_temp_file(json: &str) -> Result<WorkerTempFile, SttError> {
    worker::write_worker_temp_file("sabbathcue-vosk-grammar-", ".json", json)
}

fn vosk_grammar_json() -> Result<String, SttError> {
    let phrases = verse_only_keyterms();
    serde_json::to_string(&phrases)
        .map_err(|e| SttError::ConnectionFailed(format!("failed to build Vosk grammar: {e}")))
}

#[async_trait::async_trait]
impl SttProvider for VoskProvider {
    async fn start(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        self.cancelled.store(false, Ordering::SeqCst);
        let cancel_on_drop = worker::CancellationGuard::new(self.cancelled.clone());
        let grammar_json = vosk_grammar_json()?;
        let grammar_file = write_grammar_temp_file(&grammar_json)?;
        let mut child = self.spawn_worker(&grammar_file)?;
        let Some(mut stdin) = child.stdin.take() else {
            worker::terminate_child(VOSK_LABEL, &mut child);
            return Err(SttError::ConnectionFailed(
                "failed to open Vosk stdin".to_string(),
            ));
        };
        let Some(stdout) = child.stdout.take() else {
            worker::terminate_child(VOSK_LABEL, &mut child);
            return Err(SttError::ConnectionFailed(
                "failed to open Vosk stdout".to_string(),
            ));
        };
        let Some(stderr) = child.stderr.take() else {
            worker::terminate_child(VOSK_LABEL, &mut child);
            return Err(SttError::ConnectionFailed(
                "failed to open Vosk stderr".to_string(),
            ));
        };

        let reader = match worker::spawn_reader(VOSK_LABEL, stdout, event_tx.clone()) {
            Ok(reader) => reader,
            Err(error) => {
                worker::terminate_child(VOSK_LABEL, &mut child);
                return Err(error);
            }
        };
        let stderr_reader = match worker::spawn_stderr_logger(VOSK_LABEL, stderr) {
            Ok(reader) => reader,
            Err(error) => {
                worker::terminate_child(VOSK_LABEL, &mut child);
                return Err(error);
            }
        };

        let writer_cancelled = self.cancelled.clone();
        let writer = tokio::task::spawn_blocking(move || {
            let mut pending: Vec<i16> = Vec::with_capacity(VOSK_CHUNK_SAMPLES);
            loop {
                if writer_cancelled.load(Ordering::SeqCst) {
                    break;
                }
                match audio_rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(samples) => {
                        pending.extend(samples);
                        while pending.len() >= VOSK_CHUNK_SAMPLES {
                            let chunk: Vec<i16> = pending.drain(..VOSK_CHUNK_SAMPLES).collect();
                            worker::write_samples(VOSK_LABEL, &mut stdin, &chunk)?;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        if !pending.is_empty() {
                            worker::write_samples(VOSK_LABEL, &mut stdin, &pending)?;
                            pending.clear();
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
            if !pending.is_empty() {
                worker::write_samples(VOSK_LABEL, &mut stdin, &pending)?;
            }
            Ok::<(), SttError>(())
        });

        let mut process = worker::WorkerProcess::new(VOSK_LABEL, child, reader, stderr_reader);
        let run_result = worker::wait_for_worker_shutdown(
            VOSK_LABEL,
            self.cancelled.as_ref(),
            &mut process,
            &writer,
        )
        .await;
        cancel_on_drop.cancel();
        worker::stop_writer(VOSK_LABEL, writer).await;
        // Detached stop: joining the reader threads here deadlocked when a
        // reader was blocked sending into a full (or consumer-less) event
        // channel, so `start_transcription` never returned.
        process.stop_detached();
        let _ = event_tx.send(TranscriptEvent::Disconnected).await;
        run_result
    }

    fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn name(&self) -> &'static str {
        "vosk"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_json_is_valid_json_array() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        assert!(!parsed.is_empty(), "grammar phrases must not be empty");
    }

    #[test]
    fn grammar_json_includes_bible_reference_terms() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        for cue in [
            "chapter", "verse", "verses", "to", "through", "next", "previous",
        ] {
            assert!(
                parsed.iter().any(|p| p == cue),
                "grammar must include '{cue}'"
            );
        }
    }

    #[test]
    fn grammar_json_includes_open_dictation_fallback() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        assert!(
            parsed.iter().any(|p| p == "[unk]"),
            "grammar must include [unk] so Vosk can recognize speech outside the phrase list"
        );
    }

    #[test]
    fn grammar_json_excludes_worship_terms() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        for excluded in ["sabbath", "holy spirit", "scripture reading"] {
            assert!(
                !parsed.iter().any(|p| p == excluded),
                "grammar must not include '{excluded}'"
            );
        }
    }

    #[test]
    fn grammar_json_includes_hymn_cue_words() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        for cue in ["hymn", "hymnal", "song", "number", "sda", "adventist"] {
            assert!(
                parsed.iter().any(|p| p == cue),
                "grammar must include hymn cue word '{cue}'"
            );
        }
    }

    #[test]
    fn grammar_json_preserves_voice_control_and_translation_terms() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        for cue in ["stop transcribing", "niv", "king james version"] {
            assert!(
                parsed.iter().any(|p| p == cue),
                "grammar must include '{cue}'"
            );
        }
    }

    #[test]
    fn grammar_json_has_no_duplicates() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let mut parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        let before = parsed.len();
        parsed.sort();
        parsed.dedup();
        assert_eq!(
            before,
            parsed.len(),
            "grammar must not contain duplicate phrases"
        );
    }

    #[test]
    fn write_grammar_temp_file_writes_valid_json_array() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let temp = write_grammar_temp_file(&json).expect("temp grammar file should be created");
        let contents =
            std::fs::read_to_string(temp.path()).expect("grammar temp file should exist");
        let parsed: Vec<String> =
            serde_json::from_str(&contents).expect("grammar temp file must contain JSON array");
        let expected: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        assert_eq!(parsed, expected);
    }

    #[test]
    fn spawn_uses_grammar_file_not_inline_json() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let temp = write_grammar_temp_file(&json).expect("temp grammar file should be created");

        let mut command = Command::new("python");
        push_grammar_file_args(&mut command, temp.path());

        let args = worker::collect_command_args(&command);

        assert!(
            args.windows(2).any(|window| {
                window[0] == "--grammar-json-file" && window[1] == temp.path().to_string_lossy()
            }),
            "worker args must include --grammar-json-file with the temp path"
        );
        assert!(
            !args.iter().any(|arg| arg == &json),
            "worker args must not include the full inline grammar JSON payload"
        );
    }

    #[test]
    #[ignore = "requires Python, the vosk package, and a local model download"]
    fn local_worker_preflight_succeeds_when_model_is_installed() {
        let model_path = installed_vosk_model_path();
        let worker_path = project_root().join("scripts").join("vosk_worker.py");

        if !model_has_required_files(&model_path) || !worker_path.exists() {
            eprintln!(
                "Skipping local Vosk preflight: model={} worker={}",
                model_path.display(),
                worker_path.display()
            );
            return;
        }

        VoskProvider::new(model_path, worker_path)
            .check_ready()
            .expect("local Vosk worker/model preflight should report ready");
    }

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
    }

    fn model_has_required_files(model_path: &Path) -> bool {
        model_path.join("am").join("final.mdl").exists()
            && model_path.join("conf").join("model.conf").exists()
            && model_path.join("graph").join("HCLr.fst").exists()
            && model_path.join("graph").join("Gr.fst").exists()
    }

    fn installed_vosk_model_path() -> PathBuf {
        let model_root = project_root()
            .join("models")
            .join("vosk")
            .join("vosk-model-en-us-0.22-lgraph");
        let nested_model_root = model_root.join("vosk-model-en-us-0.22-lgraph");

        if model_has_required_files(&model_root) {
            model_root
        } else if model_has_required_files(&nested_model_root) {
            nested_model_root
        } else {
            model_root
        }
    }

    #[test]
    fn check_ready_reports_missing_model() {
        let temp = tempfile::tempdir().expect("temp dir");
        let worker_path = temp.path().join("vosk_worker.exe");
        std::fs::write(&worker_path, b"stub").expect("stub worker");

        let provider = VoskProvider::new(temp.path().join("missing-model"), worker_path);
        let error = provider.check_ready().expect_err("missing model must fail");
        assert!(
            error.to_string().contains("Vosk model not found"),
            "error should mention the missing model, got: {error}"
        );
    }

    #[test]
    fn check_ready_reports_missing_worker() {
        let temp = tempfile::tempdir().expect("temp dir");
        let model_path = temp.path().join("model");
        std::fs::create_dir_all(&model_path).expect("model dir");

        let provider = VoskProvider::new(model_path, temp.path().join("missing-worker.exe"));
        let error = provider
            .check_ready()
            .expect_err("missing worker must fail");
        assert!(
            error.to_string().contains("Vosk worker not found"),
            "error should mention the missing worker, got: {error}"
        );
    }

    #[test]
    fn worker_args_strip_windows_extended_length_prefix() {
        let mut command = Command::new("worker");
        push_worker_args(&mut command, Path::new(r"\\?\C:\models\vosk\model"));
        let args = worker::collect_command_args(&command);

        assert!(
            args.windows(2)
                .any(|w| w[0] == "--model" && w[1] == r"C:\models\vosk\model"),
            "model arg must not carry the \\\\?\\ prefix, got: {args:?}"
        );
    }

    #[test]
    fn worker_args_include_model_and_sample_rate() {
        let mut command = Command::new("worker");
        push_worker_args(&mut command, Path::new("C:/models/vosk"));
        let args = worker::collect_command_args(&command);
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--model" && w[1].ends_with("vosk")));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--sample-rate" && w[1] == "16000"));
    }

    /// End-to-end preflight against the real bundled worker executable and
    /// model. This is exactly what the installed app runs before starting
    /// transcription, so it catches broken worker/model/grammar combinations.
    /// Skips (with a note) when local assets are missing, e.g. on CI.
    #[test]
    fn bundled_worker_preflight_reports_ready() {
        let model_path = installed_vosk_model_path();
        let worker_path = project_root().join("sidecars").join("vosk_worker.exe");

        if !model_has_required_files(&model_path) || !worker_path.exists() {
            eprintln!(
                "Skipping bundled Vosk preflight: model={} worker={}",
                model_path.display(),
                worker_path.display()
            );
            return;
        }

        VoskProvider::new(model_path, worker_path)
            .check_ready()
            .expect("bundled Vosk worker/model preflight should report ready");
    }
}
