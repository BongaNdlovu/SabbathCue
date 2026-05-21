//! Local faster-whisper STT provider using a Python/CTranslate2 worker.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use crossbeam_channel::Receiver;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::SttError;
use crate::provider::SttProvider;
use crate::types::{TranscriptEvent, Word};

const MAX_BUFFER_SAMPLES: usize = 16_000 * 10;
const MIN_BUFFER_SAMPLES: usize = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FasterWhisperProfile {
    Fast,
    Balanced,
}

impl FasterWhisperProfile {
    pub(crate) fn from_name(name: Option<&str>) -> Self {
        match name {
            Some("fast") => Self::Fast,
            _ => Self::Balanced,
        }
    }

    pub(crate) const fn live_chunk_samples(self) -> usize {
        match self {
            Self::Fast => 16_000 + 4_000,
            Self::Balanced => (16_000 * 2) + 4_000,
        }
    }

    pub(crate) const fn beam_size(self) -> u8 {
        match self {
            Self::Fast => 1,
            Self::Balanced => 3,
        }
    }
}

#[derive(Debug, Serialize)]
struct WorkerRequest {
    id: u64,
    samples_b64: String,
}

#[derive(Debug, Deserialize)]
struct WorkerResponse {
    id: u64,
    text: Option<String>,
    segments: Option<Vec<WorkerSegment>>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkerSegment {
    text: String,
    start: f64,
    end: f64,
}

struct FasterWhisperWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl FasterWhisperWorker {
    fn spawn(
        python_path: &str,
        script_path: &PathBuf,
        model: &str,
        language: Option<&str>,
        profile: FasterWhisperProfile,
    ) -> Result<Self, SttError> {
        if !script_path.exists() {
            return Err(SttError::ModelNotFound(format!(
                "faster-whisper worker not found: {}",
                script_path.display()
            )));
        }

        let compute_type = std::env::var("SABBATHCUE_FASTER_WHISPER_COMPUTE_TYPE")
            .unwrap_or_else(|_| "int8".to_string());
        let device = std::env::var("SABBATHCUE_FASTER_WHISPER_DEVICE")
            .unwrap_or_else(|_| "auto".to_string());

        let mut command = Command::new(python_path);
        command
            .arg(script_path)
            .arg("--model")
            .arg(model)
            .arg("--device")
            .arg(device)
            .arg("--compute-type")
            .arg(compute_type)
            .arg("--beam-size")
            .arg(profile.beam_size().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        if let Some(language) = language {
            command.arg("--language").arg(language);
        }

        let mut child = command.spawn().map_err(|e| {
            SttError::ConnectionFailed(format!("failed to start faster-whisper worker: {e}"))
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            SttError::ConnectionFailed("failed to open faster-whisper stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            SttError::ConnectionFailed("failed to open faster-whisper stdout".to_string())
        })?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        })
    }

    fn transcribe(&mut self, samples: &[i16]) -> Result<(String, Vec<Word>, f64), SttError> {
        if let Some(status) = self.child.try_wait().map_err(|e| {
            SttError::ConnectionFailed(format!("failed to check faster-whisper worker: {e}"))
        })? {
            return Err(self.worker_exited_error(status));
        }

        self.next_id += 1;
        let id = self.next_id;
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let request = WorkerRequest {
            id,
            samples_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
        };
        let line = serde_json::to_string(&request)
            .map_err(|e| SttError::ParseError(format!("worker request encode failed: {e}")))?;

        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| {
                if let Ok(Some(status)) = self.child.try_wait() {
                    self.worker_exited_error(status)
                } else {
                    SttError::SendError(format!("faster-whisper worker write failed: {e}"))
                }
            })?;

        let mut response_line = String::new();
        let bytes_read = self.stdout.read_line(&mut response_line).map_err(|e| {
            SttError::ConnectionFailed(format!("faster-whisper worker read failed: {e}"))
        })?;
        if bytes_read == 0 {
            return Err(SttError::ConnectionFailed(
                "faster-whisper worker exited".to_string(),
            ));
        }

        let response: WorkerResponse = serde_json::from_str(&response_line)
            .map_err(|e| SttError::ParseError(format!("worker response decode failed: {e}")))?;
        if response.id == 0 {
            if let Some(error) = response.error {
                return Err(SttError::ConnectionFailed(format!(
                    "faster-whisper worker error: {error}"
                )));
            }
        }
        if response.id != id {
            return Err(SttError::ParseError(format!(
                "worker response id mismatch: expected {id}, got {}",
                response.id
            )));
        }
        if let Some(error) = response.error {
            return Err(SttError::ConnectionFailed(format!(
                "faster-whisper worker error: {error}"
            )));
        }

        let text = response.text.unwrap_or_default().trim().to_string();
        let words = response
            .segments
            .unwrap_or_default()
            .into_iter()
            .flat_map(segment_to_words)
            .collect();
        Ok((text, words, 0.9))
    }

    fn worker_exited_error(&mut self, status: std::process::ExitStatus) -> SttError {
        let mut response_line = String::new();
        let response_error = match self.stdout.read_line(&mut response_line) {
            Ok(0) => None,
            Ok(_) => serde_json::from_str::<WorkerResponse>(&response_line)
                .ok()
                .and_then(|response| response.error),
            Err(_) => None,
        };

        let message = response_error.map_or_else(
            || format!("faster-whisper worker exited with status {status}"),
            |error| format!("faster-whisper worker error: {error}"),
        );
        SttError::ConnectionFailed(message)
    }
}

impl Drop for FasterWhisperWorker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn segment_to_words(segment: WorkerSegment) -> Vec<Word> {
    let tokens: Vec<&str> = segment.text.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }

    #[expect(clippy::cast_precision_loss, reason = "word counts are small")]
    let duration_per_word = (segment.end - segment.start).max(0.0) / tokens.len() as f64;
    tokens
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            #[expect(clippy::cast_precision_loss, reason = "word counts are small")]
            let start = segment.start + (index as f64 * duration_per_word);
            Word {
                text: text.to_lowercase(),
                start,
                end: start + duration_per_word,
                confidence: 0.9,
                punctuated_word: Some(text.to_string()),
            }
        })
        .collect()
}

fn queue_inference(
    inference_tx: &mpsc::Sender<Vec<i16>>,
    audio_buffer: &mut Vec<i16>,
    reason: &str,
) {
    if audio_buffer.len() < MIN_BUFFER_SAMPLES {
        audio_buffer.clear();
        return;
    }

    #[expect(clippy::cast_precision_loss, reason = "audio sample count fits in f64")]
    let buffer_duration_s = audio_buffer.len() as f64 / 16_000.0;
    log::info!(
        "[FASTER-WHISPER] flush on {reason}: audio_buffer={} samples ({:.1}s)",
        audio_buffer.len(),
        buffer_duration_s,
    );

    if inference_tx.try_send(std::mem::take(audio_buffer)).is_err() {
        log::warn!("[FASTER-WHISPER] inference queue is full; dropping {reason} chunk");
    }
}

/// Local faster-whisper provider.
pub struct FasterWhisperProvider {
    python_path: String,
    script_path: PathBuf,
    model: String,
    language: Option<String>,
    profile: FasterWhisperProfile,
    cancelled: Arc<AtomicBool>,
}

impl FasterWhisperProvider {
    pub fn new(
        python_path: String,
        script_path: PathBuf,
        model: String,
        language: Option<String>,
        profile: Option<&str>,
    ) -> Self {
        Self {
            python_path,
            script_path,
            model,
            language,
            profile: FasterWhisperProfile::from_name(profile),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl std::fmt::Debug for FasterWhisperProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FasterWhisperProvider")
            .field("python_path", &self.python_path)
            .field("script_path", &self.script_path)
            .field("model", &self.model)
            .field("language", &self.language)
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl SttProvider for FasterWhisperProvider {
    #[expect(clippy::too_many_lines, reason = "mirrors the local Whisper pipeline")]
    async fn start(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        let _ = event_tx.send(TranscriptEvent::Connected).await;

        let python_path = self.python_path.clone();
        let script_path = self.script_path.clone();
        let model = self.model.clone();
        let language = self.language.clone();
        let profile = self.profile;
        let cancelled = self.cancelled.clone();

        let (inference_tx, mut inference_rx) = mpsc::channel::<Vec<i16>>(4);

        let vad_cancelled = cancelled.clone();
        let vad_event_tx = event_tx.clone();
        let vad_handle = tokio::task::spawn_blocking(move || {
            use rhema_audio::{AudioFrame, Vad, VadConfig, VadTransition};

            let vad_config = VadConfig {
                silence_threshold: 0.002,
                frame_threshold: 0.001,
                min_voice_frames: 2,
                silence_frame_count: 8,
                ..VadConfig::default()
            };
            let mut vad = Vad::new(vad_config);
            let mut audio_buffer: Vec<i16> = Vec::new();

            loop {
                if vad_cancelled.load(Ordering::SeqCst) {
                    queue_inference(&inference_tx, &mut audio_buffer, "cancel");
                    break;
                }

                match audio_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(samples) => {
                        let result = vad.process(&AudioFrame {
                            samples,
                            timestamp_ms: 0,
                        });

                        if let Some(transition) = result.transition {
                            match transition {
                                VadTransition::SpeechStarted => {
                                    let _ =
                                        vad_event_tx.blocking_send(TranscriptEvent::SpeechStarted);
                                }
                                VadTransition::SpeechEnded => {
                                    queue_inference(
                                        &inference_tx,
                                        &mut audio_buffer,
                                        "SpeechEnded",
                                    );
                                }
                            }
                        }

                        for frame in result.frames {
                            audio_buffer.extend_from_slice(&frame.samples);
                        }

                        if audio_buffer.len() >= MAX_BUFFER_SAMPLES {
                            queue_inference(&inference_tx, &mut audio_buffer, "MAX_BUFFER");
                        } else if audio_buffer.len() >= profile.live_chunk_samples() {
                            queue_inference(&inference_tx, &mut audio_buffer, "live chunk");
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        queue_inference(&inference_tx, &mut audio_buffer, "disconnect");
                        break;
                    }
                }
            }
        });

        let inf_cancelled = cancelled.clone();
        let inf_event_tx = event_tx.clone();
        let inf_handle = tokio::task::spawn_blocking(move || {
            let mut worker = match FasterWhisperWorker::spawn(
                &python_path,
                &script_path,
                &model,
                language.as_deref(),
                profile,
            ) {
                Ok(worker) => worker,
                Err(e) => {
                    let _ = inf_event_tx.blocking_send(TranscriptEvent::Error(e.to_string()));
                    return;
                }
            };

            log::info!("[FASTER-WHISPER] Worker ready: model={model}, profile={profile:?}");

            while let Some(audio_i16) = inference_rx.blocking_recv() {
                if inf_cancelled.load(Ordering::SeqCst) {
                    break;
                }

                let start = std::time::Instant::now();
                match worker.transcribe(&audio_i16) {
                    Ok((text, words, confidence)) => {
                        #[expect(clippy::cast_precision_loss, reason = "audio sample count fits")]
                        let audio_duration_s = audio_i16.len() as f64 / 16_000.0;
                        log::info!(
                            "[FASTER-WHISPER] Transcribed {:.1}s audio in {:.1?}: \"{text}\"",
                            audio_duration_s,
                            start.elapsed()
                        );
                        if !text.is_empty() {
                            let _ = inf_event_tx.blocking_send(TranscriptEvent::Final {
                                transcript: text,
                                words,
                                confidence,
                                speech_final: true,
                            });
                        }
                        let _ = inf_event_tx.blocking_send(TranscriptEvent::UtteranceEnd);
                    }
                    Err(e) => {
                        log::error!("[FASTER-WHISPER] Inference error: {e}");
                        let _ = inf_event_tx.blocking_send(TranscriptEvent::Error(e.to_string()));
                    }
                }
            }

            log::info!("[FASTER-WHISPER] Inference task exiting");
        });

        let _ = tokio::join!(vad_handle, inf_handle);
        let _ = event_tx.send(TranscriptEvent::Disconnected).await;

        Ok(())
    }

    fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn name(&self) -> &'static str {
        "faster-whisper"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_profile_defaults_to_balanced() {
        assert_eq!(
            FasterWhisperProfile::from_name(None),
            FasterWhisperProfile::Balanced
        );
        assert_eq!(
            FasterWhisperProfile::from_name(Some("accurate")),
            FasterWhisperProfile::Balanced
        );
    }

    #[test]
    fn profiles_change_latency_and_beam_size() {
        assert!(
            FasterWhisperProfile::Fast.live_chunk_samples()
                < FasterWhisperProfile::Balanced.live_chunk_samples()
        );
        assert!(
            FasterWhisperProfile::Fast.beam_size() < FasterWhisperProfile::Balanced.beam_size()
        );
    }
}
