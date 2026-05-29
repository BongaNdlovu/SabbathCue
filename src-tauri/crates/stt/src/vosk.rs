//! Offline Vosk STT provider.
//!
//! The provider streams 16 kHz mono PCM to a small Python worker. Keeping the
//! binding out-of-process avoids native linker friction in the desktop build
//! while still giving the app a real streaming, offline STT path when the
//! `vosk` Python package and model are installed.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::error::SttError;
use crate::keyterms::verse_only_keyterms;
use crate::provider::SttProvider;
use crate::types::{TranscriptEvent, Word};

/// 50ms at 16 kHz. A small latency tradeoff gives Vosk more acoustic context
/// per pass, which helps short Bible references land more consistently.
const DEFAULT_CHUNK_SAMPLES: usize = 800;

#[derive(Debug)]
pub struct VoskProvider {
    model_path: PathBuf,
    worker_path: PathBuf,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorkerEvent {
    Ready,
    Partial {
        text: String,
        #[serde(default)]
        words: Vec<VoskWord>,
    },
    Final {
        text: String,
        #[serde(default)]
        words: Vec<VoskWord>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Deserialize)]
struct VoskWord {
    word: String,
    start: f64,
    end: f64,
    #[serde(default)]
    conf: f64,
}

impl VoskProvider {
    pub fn new(model_path: PathBuf, worker_path: PathBuf) -> Self {
        Self {
            model_path,
            worker_path,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_worker(&self, grammar_file: &GrammarTempFile) -> Result<Child, SttError> {
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

        let mut command = Command::new(python_executable());
        command
            .arg(&self.worker_path)
            .arg("--model")
            .arg(&self.model_path)
            .arg("--sample-rate")
            .arg("16000");
        push_grammar_file_args(&mut command, grammar_file.path());
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SttError::ConnectionFailed(format!("failed to start Vosk worker: {e}")))
    }
}

/// Keeps a grammar JSON temp file on disk until dropped.
struct GrammarTempFile {
    path: PathBuf,
}

impl GrammarTempFile {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for GrammarTempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn push_grammar_file_args(command: &mut Command, grammar_file: &Path) {
    command.arg("--grammar-json-file").arg(grammar_file);
}

fn write_grammar_temp_file(json: &str) -> Result<GrammarTempFile, SttError> {
    let mut file = tempfile::Builder::new()
        .prefix("sabbathcue-vosk-grammar-")
        .suffix(".json")
        .tempfile()
        .map_err(|e| {
            SttError::ConnectionFailed(format!("failed to create Vosk grammar temp file: {e}"))
        })?;
    file.write_all(json.as_bytes()).map_err(|e| {
        SttError::ConnectionFailed(format!("failed to write Vosk grammar temp file: {e}"))
    })?;
    file.flush().map_err(|e| {
        SttError::ConnectionFailed(format!("failed to flush Vosk grammar temp file: {e}"))
    })?;

    let (_file, path) = file.keep().map_err(|e| {
        SttError::ConnectionFailed(format!("failed to persist Vosk grammar temp file: {e}"))
    })?;
    Ok(GrammarTempFile { path })
}

fn python_executable() -> String {
    std::env::var("SABBATHCUE_PYTHON")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "python".to_string())
}

fn write_samples(stdin: &mut ChildStdin, samples: &[i16]) -> Result<(), SttError> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    stdin
        .write_all(&bytes)
        .map_err(|e| SttError::SendError(format!("Vosk worker write failed: {e}")))
}

fn to_words(words: Vec<VoskWord>) -> Vec<Word> {
    words
        .into_iter()
        .map(|word| Word {
            text: word.word.clone(),
            start: word.start,
            end: word.end,
            confidence: word.conf,
            punctuated_word: Some(word.word),
        })
        .collect()
}

fn average_confidence(words: &[Word]) -> f64 {
    let scored = words
        .iter()
        .filter(|word| word.confidence > 0.0)
        .collect::<Vec<_>>();
    if scored.is_empty() {
        return 0.75;
    }
    let scored_len = u32::try_from(scored.len()).expect("word count fits in u32");
    scored.iter().map(|word| word.confidence).sum::<f64>() / f64::from(scored_len)
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
        let grammar_json = vosk_grammar_json()?;
        let grammar_file = write_grammar_temp_file(&grammar_json)?;
        let mut child = self.spawn_worker(&grammar_file)?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| SttError::ConnectionFailed("failed to open Vosk stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SttError::ConnectionFailed("failed to open Vosk stdout".to_string()))?;

        let cancelled = self.cancelled.clone();
        let reader_tx = event_tx.clone();
        let reader = std::thread::Builder::new()
            .name("vosk-worker-reader".into())
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
            .map_err(|e| SttError::ConnectionFailed(format!("failed to spawn Vosk reader: {e}")))?;

        let writer_cancelled = cancelled.clone();
        let writer = tokio::task::spawn_blocking(move || {
            let mut pending: Vec<i16> = Vec::with_capacity(DEFAULT_CHUNK_SAMPLES);
            loop {
                if writer_cancelled.load(Ordering::SeqCst) {
                    break;
                }
                match audio_rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(samples) => {
                        pending.extend(samples);
                        while pending.len() >= DEFAULT_CHUNK_SAMPLES {
                            let chunk: Vec<i16> = pending.drain(..DEFAULT_CHUNK_SAMPLES).collect();
                            write_samples(&mut stdin, &chunk)?;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        if !pending.is_empty() {
                            write_samples(&mut stdin, &pending)?;
                            pending.clear();
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
            if !pending.is_empty() {
                write_samples(&mut stdin, &pending)?;
            }
            Ok::<(), SttError>(())
        });

        let _ = writer.await;
        let _ = child.kill();
        let _ = reader.join();
        let _ = event_tx.send(TranscriptEvent::Disconnected).await;
        Ok(())
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
        assert!(
            parsed.iter().any(|p| p == "chapter"),
            "grammar must include 'chapter'"
        );
        assert!(
            parsed.iter().any(|p| p == "verse"),
            "grammar must include 'verse'"
        );
        assert!(
            parsed.iter().any(|p| p == "verses"),
            "grammar must include 'verses'"
        );
        assert!(
            parsed.iter().any(|p| p == "to"),
            "grammar must include 'to'"
        );
        assert!(
            parsed.iter().any(|p| p == "through"),
            "grammar must include 'through'"
        );
        assert!(
            parsed.iter().any(|p| p == "next"),
            "grammar must include 'next'"
        );
        assert!(
            parsed.iter().any(|p| p == "previous"),
            "grammar must include 'previous'"
        );
    }

    #[test]
    fn grammar_json_excludes_worship_and_unk_terms() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        assert!(
            !parsed.iter().any(|p| p == "[unk]"),
            "grammar must NOT include '[unk]'"
        );
        assert!(
            !parsed.iter().any(|p| p == "sabbath"),
            "grammar must NOT include 'sabbath'"
        );
        assert!(
            !parsed.iter().any(|p| p == "holy spirit"),
            "grammar must NOT include 'holy spirit'"
        );
        assert!(
            !parsed.iter().any(|p| p == "scripture reading"),
            "grammar must NOT include 'scripture reading'"
        );
        assert!(
            !parsed.iter().any(|p| p == "hymn number"),
            "grammar must NOT include 'hymn number'"
        );
    }

    #[test]
    fn grammar_json_includes_high_number_support() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        assert!(
            parsed.iter().any(|p| p == "hundred"),
            "grammar must include 'hundred' for high chapter/verse numbers"
        );
        assert!(
            parsed.iter().any(|p| p == "and"),
            "grammar must include 'and' for number-word grouping"
        );
    }

    #[test]
    fn grammar_json_preserves_voice_control_and_translation_terms() {
        let json = vosk_grammar_json().expect("grammar JSON should be valid");
        let parsed: Vec<String> =
            serde_json::from_str(&json).expect("grammar JSON must parse as string array");
        assert!(
            parsed.iter().any(|p| p == "stop transcribing"),
            "grammar must keep the supported stop command"
        );
        assert!(
            parsed.iter().any(|p| p == "niv"),
            "grammar must keep supported translation abbreviations"
        );
        assert!(
            parsed.iter().any(|p| p == "king james version"),
            "grammar must keep supported translation names"
        );
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

        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

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
        assert!(
            !args.iter().any(|arg| arg == "--grammar-json"),
            "worker args must not use --grammar-json"
        );
    }
}
