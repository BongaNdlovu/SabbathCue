//! Local Whisper STT provider using whisper.cpp via whisper-rs.
//!
//! Processes audio through the app's VAD to detect speech boundaries,
//! then runs Whisper inference on each speech segment. Emits the same
//! [`TranscriptEvent`] types as the Deepgram provider.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use tokio::sync::mpsc;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::error::SttError;
use crate::provider::SttProvider;
use crate::types::{TranscriptEvent, Word};

/// Maximum audio buffer before force-flushing to inference (10 seconds at 16 kHz).
const MAX_BUFFER_SAMPLES: usize = 16_000 * 10;

/// Minimum audio buffer for inference (1.0 seconds).
/// Whisper warns "input is too short" below 1s.
const MIN_BUFFER_SAMPLES: usize = 16_000;
const WHISPER_PROMPT_PREAMBLE: &str = "Sermon and Bible reading transcription. Spell Bible references and book names exactly. Common Bible words: Jesus, Christ, God, Lord, Holy Spirit, Genesis, Exodus, Leviticus, Numbers, Deuteronomy, Joshua, Judges, Ruth, Samuel, Kings, Chronicles, Ezra, Nehemiah, Esther, Job, Psalms, Proverbs, Ecclesiastes, Song of Solomon, Isaiah, Jeremiah, Lamentations, Ezekiel, Daniel, Hosea, Joel, Amos, Obadiah, Jonah, Micah, Nahum, Habakkuk, Zephaniah, Haggai, Zechariah, Malachi, Matthew, Mark, Luke, John, Acts, Romans, Corinthians, Galatians, Ephesians, Philippians, Colossians, Thessalonians, Timothy, Titus, Philemon, Hebrews, James, Peter, Jude, Revelation.";
const MAX_ROLLING_PROMPT_CHARS: usize = 1_200;
const MAX_ROLLING_TRANSCRIPTS: usize = 6;
const ROLLING_PROMPT_MAX_AGE: Duration = Duration::from_secs(45);

#[derive(Debug, Default)]
struct RollingTranscriptPrompt {
    recent: std::collections::VecDeque<String>,
    last_updated: Option<Instant>,
}

impl RollingTranscriptPrompt {
    fn push(&mut self, text: &str) {
        self.push_at(text, Instant::now());
    }

    fn push_at(&mut self, text: &str, now: Instant) {
        let cleaned = text.trim();
        if cleaned.is_empty() {
            return;
        }

        self.recent.push_back(cleaned.to_string());
        while self.recent.len() > MAX_ROLLING_TRANSCRIPTS {
            self.recent.pop_front();
        }
        self.last_updated = Some(now);
    }

    fn prompt(&mut self) -> String {
        self.clear_if_stale(Instant::now());
        let mut prompt = WHISPER_PROMPT_PREAMBLE.to_string();
        if self.recent.is_empty() {
            return prompt;
        }

        prompt.push_str(" Recent transcript: ");
        for text in self.recent.iter().rev() {
            if prompt.len() + text.len() + 1 > MAX_ROLLING_PROMPT_CHARS {
                break;
            }
            if !prompt.ends_with(' ') {
                prompt.push(' ');
            }
            prompt.push_str(text);
        }
        prompt
    }

    fn clear_if_stale(&mut self, now: Instant) {
        let Some(last_updated) = self.last_updated else {
            return;
        };
        if now.duration_since(last_updated) >= ROLLING_PROMPT_MAX_AGE {
            self.recent.clear();
            self.last_updated = None;
            log::info!("[Whisper] Cleared stale rolling prompt context");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhisperProfile {
    Fast,
    Balanced,
}

impl WhisperProfile {
    pub(crate) fn from_name(name: Option<&str>) -> Self {
        match name {
            Some("fast") => Self::Fast,
            _ => Self::Balanced,
        }
    }

    pub(crate) const fn live_chunk_samples(self) -> usize {
        match self {
            Self::Fast => 16_000,
            Self::Balanced => 16_000 + 8_000,
        }
    }

    pub(crate) const fn sampling_strategy(self) -> SamplingStrategy {
        SamplingStrategy::Greedy { best_of: 1 }
    }

    pub(crate) const fn use_context(self) -> bool {
        true
    }

    pub(crate) const fn no_speech_threshold(self) -> f32 {
        match self {
            Self::Fast => 0.65,
            Self::Balanced => 0.7,
        }
    }
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
        "[WHISPER] flush on {reason}: audio_buffer={} samples ({:.1}s)",
        audio_buffer.len(),
        buffer_duration_s,
    );

    if inference_tx.try_send(std::mem::take(audio_buffer)).is_err() {
        log::warn!("[WHISPER] inference queue is full; dropping {reason} chunk");
    }
}

/// Convert i16 PCM samples to f32 in [-1.0, 1.0] range.
fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| f32::from(s) / 32768.0).collect()
}

/// Extract transcript text, words, and average confidence from Whisper state.
#[expect(
    clippy::cast_precision_loss,
    reason = "timestamps and word counts are small enough"
)]
fn extract_segments(state: &WhisperState) -> (String, Vec<Word>, f64) {
    let n_segments = state.full_n_segments();
    let mut full_text = String::new();
    let mut words = Vec::new();
    let mut total_confidence: f64 = 0.0;

    for i in 0..n_segments {
        let Some(segment) = state.get_segment(i) else {
            continue;
        };
        let text = segment.to_str_lossy().unwrap_or_default().to_string();
        let start_ts = segment.start_timestamp();
        let end_ts = segment.end_timestamp();

        let start_sec = start_ts as f64 / 100.0;
        let end_sec = end_ts as f64 / 100.0;
        let confidence = 0.9;

        let n_words = text.split_whitespace().count();
        if n_words > 0 {
            let duration_per_word = (end_sec - start_sec) / n_words as f64;
            for (j, word_text) in text.split_whitespace().enumerate() {
                let w_start = start_sec + (j as f64 * duration_per_word);
                let w_end = w_start + duration_per_word;
                words.push(Word {
                    text: word_text.to_lowercase(),
                    start: w_start,
                    end: w_end,
                    confidence,
                    punctuated_word: Some(word_text.to_string()),
                });
            }
        }

        full_text.push_str(&text);
        total_confidence += confidence;
    }

    let avg_confidence = if n_segments > 0 {
        total_confidence / f64::from(n_segments)
    } else {
        0.0
    };

    (full_text.trim().to_string(), words, avg_confidence)
}

/// Local Whisper STT provider.
pub struct WhisperProvider {
    model_path: PathBuf,
    language: Option<String>,
    n_threads: i32,
    profile: WhisperProfile,
    cancelled: Arc<AtomicBool>,
}

impl WhisperProvider {
    /// Create a new Whisper provider.
    ///
    /// - `model_path`: path to a GGML model file
    /// - `language`: ISO language code (e.g. "en") or `None` for auto-detect
    /// - `n_threads`: number of CPU threads for inference
    pub fn new(
        model_path: PathBuf,
        language: Option<String>,
        n_threads: i32,
        profile: Option<&str>,
    ) -> Self {
        Self {
            model_path,
            language,
            n_threads: n_threads.max(1),
            profile: WhisperProfile::from_name(profile),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl std::fmt::Debug for WhisperProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhisperProvider")
            .field("model_path", &self.model_path)
            .field("language", &self.language)
            .field("n_threads", &self.n_threads)
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl SttProvider for WhisperProvider {
    #[expect(
        clippy::too_many_lines,
        reason = "spawns two blocking tasks with setup; splitting would obscure the pipeline flow"
    )]
    #[expect(clippy::cast_precision_loss, reason = "audio sample count fits in f64")]
    async fn start(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        if !self.model_path.exists() {
            return Err(SttError::ModelNotFound(format!(
                "Whisper model not found: {}",
                self.model_path.display()
            )));
        }

        let _ = event_tx.send(TranscriptEvent::Connected).await;

        let model_path = self.model_path.clone();
        let language = self.language.clone();
        let n_threads = self.n_threads;
        let profile = self.profile;
        let cancelled = self.cancelled.clone();

        let (inference_tx, mut inference_rx) = mpsc::channel::<Vec<i16>>(4);

        // ── Task 1: VAD + audio accumulation ─────────────────────────────
        let vad_cancelled = cancelled.clone();
        let vad_event_tx = event_tx.clone();
        let vad_handle = tokio::task::spawn_blocking(move || {
            use rhema_audio::{AudioFrame, Vad, VadConfig, VadTransition};

            // Keep thresholds modest so quieter microphones still open the gate.
            // Whisper's no_speech threshold still filters near-silence at inference time.
            let vad_config = VadConfig {
                silence_threshold: 0.002,
                frame_threshold: 0.001,
                min_voice_frames: 2,
                silence_frame_count: 6,
                ..VadConfig::default()
            };
            let mut vad = Vad::new(vad_config);
            let mut audio_buffer: Vec<i16> = Vec::new();
            let mut frames_seen: u64 = 0;

            loop {
                if vad_cancelled.load(Ordering::SeqCst) {
                    queue_inference(&inference_tx, &mut audio_buffer, "cancel");
                    break;
                }

                match audio_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(samples) => {
                        let frame = AudioFrame {
                            samples,
                            timestamp_ms: 0,
                        };
                        let result = vad.process(&frame);

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
                            frames_seen += 1;
                        }

                        if frames_seen.is_multiple_of(50) && !audio_buffer.is_empty() {
                            log::debug!(
                                "[WHISPER] audio_buffer={} samples ({:.1}s) frames_seen={frames_seen}",
                                audio_buffer.len(),
                                audio_buffer.len() as f64 / 16_000.0,
                            );
                        }

                        if audio_buffer.len() >= MAX_BUFFER_SAMPLES {
                            log::warn!(
                                "[WHISPER] flush on MAX_BUFFER ({} samples / {:.1}s) — VAD never closed",
                                audio_buffer.len(),
                                audio_buffer.len() as f64 / 16_000.0,
                            );
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

        // ── Task 2: Whisper inference ────────────────────────────────────
        let inf_cancelled = cancelled.clone();
        let inf_event_tx = event_tx.clone();
        let inf_handle = tokio::task::spawn_blocking(move || {
            let ctx = match WhisperContext::new_with_params(
                &model_path,
                WhisperContextParameters::default(),
            ) {
                Ok(ctx) => ctx,
                Err(e) => {
                    log::error!("[Whisper] Failed to load model: {e}");
                    let _ = inf_event_tx.blocking_send(TranscriptEvent::Error(format!(
                        "Failed to load Whisper model: {e}"
                    )));
                    return;
                }
            };

            let mut state = match ctx.create_state() {
                Ok(s) => s,
                Err(e) => {
                    log::error!("[Whisper] Failed to create state: {e}");
                    let _ = inf_event_tx.blocking_send(TranscriptEvent::Error(format!(
                        "Failed to create Whisper state: {e}"
                    )));
                    return;
                }
            };

            log::info!(
                "[Whisper] Model loaded, ready for inference: profile={profile:?}, threads={n_threads}"
            );

            let mut rolling_prompt = RollingTranscriptPrompt::default();

            while let Some(audio_i16) = inference_rx.blocking_recv() {
                if inf_cancelled.load(Ordering::SeqCst) {
                    break;
                }

                let audio_f32 = i16_to_f32(&audio_i16);

                let mut params = FullParams::new(profile.sampling_strategy());
                params.set_language(Some(language.as_deref().unwrap_or("en")));
                params.set_n_threads(n_threads);
                params.set_print_progress(false);
                params.set_print_special(false);
                params.set_print_realtime(false);
                let initial_prompt = rolling_prompt.prompt();
                params.set_initial_prompt(&initial_prompt);
                params.set_no_context(!profile.use_context());
                params.set_no_timestamps(false);
                params.set_single_segment(true);
                params.set_token_timestamps(false);
                params.set_temperature(0.0);
                params.set_temperature_inc(0.0);
                params.set_no_speech_thold(profile.no_speech_threshold());
                params.set_suppress_blank(true);
                params.set_suppress_nst(true);

                let start = std::time::Instant::now();
                if let Err(e) = state.full(params, &audio_f32) {
                    log::error!("[Whisper] Inference error: {e}");
                    let _ = inf_event_tx.blocking_send(TranscriptEvent::Error(format!(
                        "Whisper inference error: {e}"
                    )));
                    continue;
                }
                let elapsed = start.elapsed();

                let (text, words, confidence) = extract_segments(&state);

                #[expect(clippy::cast_precision_loss, reason = "audio sample count fits in f64")]
                let audio_duration_s = audio_i16.len() as f64 / 16_000.0;
                log::info!(
                    "[Whisper] Transcribed {audio_duration_s:.1}s audio in {elapsed:.1?}: \"{text}\""
                );

                if !text.is_empty() {
                    rolling_prompt.push(&text);
                    let _ = inf_event_tx.blocking_send(TranscriptEvent::Final {
                        transcript: text,
                        words,
                        confidence,
                        speech_final: true,
                    });
                }

                let _ = inf_event_tx.blocking_send(TranscriptEvent::UtteranceEnd);
            }

            log::info!("[Whisper] Inference task exiting");
        });

        let _ = tokio::join!(vad_handle, inf_handle);
        let _ = event_tx.send(TranscriptEvent::Disconnected).await;

        Ok(())
    }

    fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn name(&self) -> &'static str {
        "whisper"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_profile_defaults_to_balanced() {
        assert_eq!(WhisperProfile::from_name(None), WhisperProfile::Balanced);
        assert_eq!(
            WhisperProfile::from_name(Some("not-a-profile")),
            WhisperProfile::Balanced
        );
        assert_eq!(
            WhisperProfile::from_name(Some("accurate")),
            WhisperProfile::Balanced
        );
    }

    #[test]
    fn profiles_prioritize_accuracy_with_bounded_latency() {
        let fast = WhisperProfile::Fast;
        let balanced = WhisperProfile::Balanced;

        assert!(fast.live_chunk_samples() < balanced.live_chunk_samples());
        assert!(matches!(
            fast.sampling_strategy(),
            SamplingStrategy::Greedy { best_of: 1 }
        ));
        assert!(matches!(
            balanced.sampling_strategy(),
            SamplingStrategy::Greedy { best_of: 1 }
        ));
        assert!(fast.use_context());
        assert!(balanced.use_context());
    }

    #[test]
    fn rolling_prompt_keeps_bounded_recent_transcript() {
        let mut prompt = RollingTranscriptPrompt::default();
        for i in 0..10 {
            prompt.push(&format!("segment {i}"));
        }

        let text = prompt.prompt();
        assert!(text.contains("Jesus"));
        assert!(text.contains("Zechariah"));
        assert!(text.contains("Malachi"));
        assert!(text.contains("segment 9"));
        assert!(!text.contains("segment 0"));
        assert!(text.len() <= MAX_ROLLING_PROMPT_CHARS);
    }

    #[test]
    fn rolling_prompt_dumps_stale_transcript() {
        let mut prompt = RollingTranscriptPrompt::default();
        let now = Instant::now();

        prompt.push_at("old transcript", now);
        prompt.clear_if_stale(now + ROLLING_PROMPT_MAX_AGE);

        let text = prompt.prompt();
        assert!(!text.contains("old transcript"));
        assert!(text.contains("Jesus"));
    }
}
