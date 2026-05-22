//! Offline Vosk STT provider.
//!
//! The provider streams 16 kHz mono PCM to a small Python worker. Keeping the
//! binding out-of-process avoids native linker friction in the desktop build
//! while still giving the app a real streaming, offline STT path when the
//! `vosk` Python package and model are installed.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::error::SttError;
use crate::provider::SttProvider;
use crate::types::{TranscriptEvent, Word};

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
    Partial { text: String },
    Final { text: String },
    Error { message: String },
}

impl VoskProvider {
    pub fn new(model_path: PathBuf, worker_path: PathBuf) -> Self {
        Self {
            model_path,
            worker_path,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_worker(&self) -> Result<Child, SttError> {
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

        Command::new(python_executable())
            .arg(&self.worker_path)
            .arg("--model")
            .arg(&self.model_path)
            .arg("--sample-rate")
            .arg("16000")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SttError::ConnectionFailed(format!("failed to start Vosk worker: {e}")))
    }
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

fn empty_words() -> Vec<Word> {
    Vec::new()
}

#[async_trait::async_trait]
impl SttProvider for VoskProvider {
    async fn start(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        let mut child = self.spawn_worker()?;
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
                        WorkerEvent::Partial { text } if !text.trim().is_empty() => {
                            let _ = reader_tx.blocking_send(TranscriptEvent::Partial {
                                transcript: text,
                                words: empty_words(),
                            });
                        }
                        WorkerEvent::Final { text } if !text.trim().is_empty() => {
                            let _ = reader_tx.blocking_send(TranscriptEvent::Final {
                                transcript: text,
                                words: empty_words(),
                                confidence: 0.75,
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
                match audio_rx.recv_timeout(Duration::from_millis(20)) {
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
