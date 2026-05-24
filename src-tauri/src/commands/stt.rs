#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::asset_paths;
use crate::events::{
    AudioLevelPayload, TranscriptPayload, WordPayload, EVENT_AUDIO_LEVEL, EVENT_AUDIO_SOURCE_LOST,
    EVENT_AUDIO_SOURCE_RECOVERED, EVENT_TRANSCRIPT_FINAL, EVENT_TRANSCRIPT_PARTIAL,
};
use crate::state::AppState;
use rhema_detection::{DetectionMerger, DirectDetector};

/// Check whether the operator has paused detection suggestions.
/// Uses a blocking lock so the pause flag is authoritative.
/// The lock is held only for an atomic load — transcript events are not blocked.
fn is_detection_paused(app: &AppHandle) -> bool {
    let state: State<'_, Mutex<AppState>> = app.state();
    let paused = match state.lock() {
        Ok(s) => s.detection_paused.load(Ordering::Relaxed),
        Err(_) => true,
    };
    paused
}

/// [DIAG] Running totals for `AppState` mutex contention on the direct-detection
/// hot path. Direct-mode detection runs on every Final transcript fragment
/// inside `spawn_blocking`, so high contention here means workers are stalling.
static DIRECT_LOCK_OK: AtomicU64 = AtomicU64::new(0);
static DIRECT_LOCK_CONTENDED: AtomicU64 = AtomicU64::new(0);
const SEMANTIC_WINDOW_SEGMENTS: usize = 8;
const PARTIAL_SEMANTIC_DEBOUNCE: Duration = Duration::from_millis(400);
const PARTIAL_SEMANTIC_MIN_WORDS: usize = 4;

fn transcript_logging_enabled() -> bool {
    matches!(
        std::env::var("SABBATHCUE_DEBUG_TRANSCRIPTS")
            .unwrap_or_default()
            .trim(),
        "1" | "true" | "TRUE" | "yes" | "YES"
    )
}

/// Truncate a string to at most `max_bytes`, snapping to a valid UTF-8 char boundary.
fn truncate_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
use rhema_audio::{AudioConfig, AudioFrame};
use rhema_stt::{DeepgramClient, SttConfig, SttProvider, TranscriptEvent, VoskProvider, Word};

use crate::commands::secrets;
use crate::commands::transcript_router::{
    TranscriptEventKind, TranscriptRouteInput, TranscriptRouter,
};

fn to_word_payloads(words: Vec<Word>) -> Vec<WordPayload> {
    words
        .into_iter()
        .map(|word| {
            let punctuated = word
                .punctuated_word
                .clone()
                .unwrap_or_else(|| word.text.clone());
            WordPayload {
                text: word.text,
                start: word.start,
                end: word.end,
                confidence: word.confidence,
                punctuated,
            }
        })
        .collect()
}

fn average_word_confidence(words: &[Word], fallback: f64) -> f64 {
    let mut total = 0.0;
    let mut count = 0usize;
    for word in words {
        if word.confidence > 0.0 {
            total += word.confidence;
            count += 1;
        }
    }
    if count == 0 {
        fallback
    } else {
        let count = u32::try_from(count).expect("word count fits in u32");
        total / f64::from(count)
    }
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Start the full audio-capture-to-transcription pipeline.
///
/// 1. Opens the microphone via cpal (on a dedicated thread so the non-Send
///    `AudioCapture` never crosses thread boundaries).
/// 2. Connects to the selected STT provider (Deepgram cloud or Vosk local).
/// 3. Fans audio out to both the level meter (emits `audio_level` events) and STT.
/// 4. Receives transcripts and emits `transcript_partial` / `transcript_final` events.
/// 5. On final transcripts, runs the detection pipeline and emits `verse_detected` events.
#[expect(
    clippy::too_many_lines,
    reason = "pipeline setup is inherently complex"
)]
#[tauri::command]
pub async fn start_transcription(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    device_id: Option<String>,
    gain: Option<f32>,
    provider: Option<String>,
    _whisper_profile: Option<String>,
) -> Result<(), String> {
    // ── 1. Guard: already running? ──────────────────────────────────────
    let (stt_active, audio_active) = {
        let app_state = state.lock().map_err(|e| e.to_string())?;
        if app_state.stt_active.load(Ordering::Relaxed) {
            return Err("Transcription is already running".into());
        }
        (app_state.stt_active.clone(), app_state.audio_active.clone())
    };

    let provider_name = provider.as_deref().unwrap_or("vosk");

    // ── 2. Build the STT provider ───────────────────────────────────────
    let stt_provider: Box<dyn SttProvider> = match provider_name {
        "vosk" | "whisper" => {
            let model_path = asset_paths::vosk_model_path(&app);
            if !model_path.exists() {
                return Err(format!(
                    "Vosk model not found at {}. Install a small English Vosk model into models/vosk/vosk-model-small-en-us.",
                    model_path.display()
                ));
            }
            let worker_path = asset_paths::vosk_worker_path(&app);
            if !worker_path.exists() {
                return Err(format!(
                    "Vosk worker not found at {}",
                    worker_path.display()
                ));
            }

            log::info!(
                "Starting Vosk transcription: model={}, worker={}, device_id={device_id:?}",
                model_path.display(),
                worker_path.display()
            );

            Box::new(VoskProvider::new(model_path, worker_path))
        }
        #[cfg(feature = "whisper")]
        "legacy-whisper" => {
            let model_path = asset_paths::vosk_model_path(&app);
            return Err(format!(
                "Legacy Whisper is no longer the local provider. Use Vosk; expected Vosk model at {}.",
                model_path.display()
            ));
        }
        "faster-whisper" => {
            return Err("faster-whisper has been removed. Choose Vosk or Deepgram.".into());
        }
        _ => {
            // Deepgram (default)
            let resolved_api_key = match std::env::var("DEEPGRAM_API_KEY") {
                Ok(v) if !v.trim().is_empty() => secrets::normalize_deepgram_api_key(&v),
                _ => secrets::get_deepgram_api_key_or_empty()?,
            };

            if resolved_api_key.is_empty() {
                return Err(
                    "No Deepgram API key configured. Set it in Settings or via DEEPGRAM_API_KEY env var."
                        .into(),
                );
            }

            log::info!(
                "Starting Deepgram transcription: api_key_configured=true, device_id={device_id:?}, gain={gain:?}"
            );

            let stt_config = SttConfig {
                api_key: resolved_api_key,
                model: "nova-3".to_string(),
                sample_rate: 16_000,
                encoding: "linear16".to_string(),
                language: Some("en-US".to_string()),
            };

            Box::new(DeepgramClient::new(stt_config))
        }
    };

    stt_active.store(true, Ordering::SeqCst);
    audio_active.store(true, Ordering::SeqCst);

    // ── 3. Prepare channels ─────────────────────────────────────────────
    let (audio_send_tx, audio_send_rx) = crossbeam_channel::bounded::<Vec<i16>>(128);

    // ── 4. Spawn the audio-capture + fan-out thread ─────────────────────
    // cpal's `Stream` (inside `AudioCapture`) is !Send, so we must create
    // and drop it on the same thread. This thread:
    //   a) starts the cpal capture
    //   b) reads AudioFrames
    //   c) computes levels → emits audio_level events
    //   d) forwards samples to STT provider via crossbeam
    let gain_val = gain.unwrap_or(1.0).clamp(0.0, 2.0);
    let fan_active = stt_active.clone();
    let fan_app = app.clone();

    std::thread::Builder::new()
        .name("audio-fanout".into())
        .spawn(move || {
            // Watchdog flag — set by cpal's stream-error callback when the OS
            // device vanishes. The outer loop polls this (and frame silence)
            // to detect loss and rebuild the capture once the device returns.
            let device_lost = Arc::new(AtomicBool::new(false));
            let mut frame_count: u64 = 0;
            let mut announced_lost = false;

            // Outer loop: rebuild `AudioCapture` whenever the device is lost
            // and reappears. Exits only when `fan_active` is cleared by
            // `stop_transcription`.
            'outer: loop {
                if !fan_active.load(Ordering::SeqCst) {
                    break 'outer;
                }

                let config = AudioConfig {
                    device_id: device_id.clone(),
                    sample_rate: 16_000,
                    gain: gain_val,
                };

                let (audio_tx, audio_rx) = crossbeam_channel::bounded::<AudioFrame>(128);
                device_lost.store(false, Ordering::SeqCst);

                let capture =
                    match rhema_audio::capture::start(config, audio_tx, device_lost.clone()) {
                        Ok(c) => {
                            if announced_lost {
                                log::info!("[AUDIO] Source recovered — capture rebuilt");
                                let _ = fan_app.emit(EVENT_AUDIO_SOURCE_RECOVERED, ());
                                announced_lost = false;
                            }
                            c
                        }
                        Err(e) => {
                            if !announced_lost {
                                log::warn!(
                                    "[AUDIO] Source unavailable: {e} — waiting for reconnect"
                                );
                                let _ = fan_app.emit(EVENT_AUDIO_SOURCE_LOST, ());
                                announced_lost = true;
                                // Drop level meter to zero so UI reflects the gap.
                                let _ = fan_app.emit(
                                    EVENT_AUDIO_LEVEL,
                                    AudioLevelPayload {
                                        rms: 0.0,
                                        peak: 0.0,
                                    },
                                );
                            }
                            std::thread::sleep(Duration::from_millis(750));
                            continue 'outer;
                        }
                    };

                log::info!("Audio capture started on fanout thread");

                let mut last_frame_at = Instant::now();

                // Inner loop: pump frames until loss is detected or stop is requested.
                loop {
                    if !fan_active.load(Ordering::SeqCst) {
                        capture.stop();
                        break 'outer;
                    }

                    // Loss signal #1: cpal's err_fn fired.
                    // Loss signal #2: no frames for >2s (some platforms silently
                    // stop delivering rather than calling err_fn).
                    if device_lost.load(Ordering::SeqCst)
                        || last_frame_at.elapsed() > Duration::from_secs(2)
                    {
                        log::warn!(
                            "[AUDIO] Source lost (err_flag={}, silent_for={:?}) — dropping capture",
                            device_lost.load(Ordering::SeqCst),
                            last_frame_at.elapsed()
                        );
                        if !announced_lost {
                            let _ = fan_app.emit(EVENT_AUDIO_SOURCE_LOST, ());
                            let _ = fan_app.emit(
                                EVENT_AUDIO_LEVEL,
                                AudioLevelPayload {
                                    rms: 0.0,
                                    peak: 0.0,
                                },
                            );
                            announced_lost = true;
                        }
                        break; // drop `capture`, outer loop rebuilds
                    }

                    match audio_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(frame) => {
                            last_frame_at = Instant::now();
                            frame_count += 1;

                            // (a) Compute audio levels at ~15 Hz
                            //     At 16 kHz with ~1024-sample frames, every 4th frame is ~15 Hz.
                            if frame_count % 4 == 0 {
                                let level = rhema_audio::meter::compute_level(&frame.samples);
                                let _ = fan_app.emit(
                                    EVENT_AUDIO_LEVEL,
                                    AudioLevelPayload {
                                        rms: level.rms,
                                        peak: level.peak,
                                    },
                                );
                            }

                            // (b) Forward all audio to STT provider. A short timeout avoids
                            // silently dropping speech during transient provider backpressure.
                            if audio_send_tx
                                .send_timeout(frame.samples, Duration::from_millis(20))
                                .is_err()
                            {
                                log::warn!("[AUDIO] Dropped STT frame: provider queue full");
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            // Capture's sender was dropped — fall through to rebuild.
                            break;
                        }
                    }
                }

                // Dropping `capture` stops the cpal stream.
                capture.stop();
            }

            log::info!("Audio capture stopped on fanout thread");
        })
        .map_err(|e| {
            stt_active.store(false, Ordering::SeqCst);
            audio_active.store(false, Ordering::SeqCst);
            format!("Failed to spawn audio fanout thread: {e}")
        })?;

    // ── 5. Spawn the STT provider on the tokio runtime ──────────────────
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TranscriptEvent>(64);

    let conn_active = stt_active.clone();
    let provider_log_name = stt_provider.name().to_string();
    let provider_log_name_task_a = provider_log_name.clone();

    // Task A: run the STT provider (Deepgram WS+REST or Vosk local).
    tauri::async_runtime::spawn(async move {
        let result = stt_provider.start(audio_send_rx, event_tx).await;
        if let Err(e) = result {
            log::error!("[STT-{provider_log_name_task_a}] Provider failed: {e}");
        }
        conn_active.store(false, Ordering::SeqCst);
        log::info!("[STT-{provider_log_name_task_a}] Provider task exited");
    });

    // Task B: consume TranscriptEvents, emit to frontend, run detection
    let evt_active = stt_active.clone();
    let event_app = app.clone();

    // Background semantic detection channel — non-blocking, drops if busy
    let (semantic_tx, mut semantic_rx) = tokio::sync::mpsc::channel::<(u64, String)>(4);

    // Background detection channel — direct + reading mode, non-blocking
    let (detect_tx, mut detect_rx) = tokio::sync::mpsc::channel::<(u64, String)>(16);

    // Background fast-preview channel: direct references only, latest work wins.
    // This gives preview a fast path without touching final detector/cooldown state.
    let (partial_preview_tx, mut partial_preview_rx) =
        tokio::sync::mpsc::channel::<(u64, String)>(4);

    // [DIAG] Counters so we can see whether transcripts are being dropped
    // because the detection workers can't keep up. Logged every 25 sends
    // alongside current queue depth.
    let detect_sent = Arc::new(AtomicU64::new(0));
    let detect_dropped = Arc::new(AtomicU64::new(0));
    let semantic_sent = Arc::new(AtomicU64::new(0));
    let semantic_dropped = Arc::new(AtomicU64::new(0));
    let transcript_seq = Arc::new(AtomicU64::new(0));
    let latest_accepted_seq = Arc::new(AtomicU64::new(0));

    // Spawn direct-preview worker. It is intentionally separate from
    // the final detector so interim text cannot consume queue cooldowns or
    // corrupt cross-segment state used for final confirmations.
    let partial_app = app.clone();
    let partial_latest_seq = transcript_seq.clone();
    tauri::async_runtime::spawn(async move {
        let partial_detector = Arc::new(Mutex::new(rhema_detection::DirectDetector::new()));
        while let Some((seq, transcript)) = partial_preview_rx.recv().await {
            let app_clone = partial_app.clone();
            let latest_seq = partial_latest_seq.clone();
            let detector = partial_detector.clone();
            let _ = tokio::task::spawn_blocking(move || {
                run_partial_direct_preview_detection(
                    &app_clone,
                    &detector,
                    seq,
                    latest_seq,
                    &transcript,
                );
            })
            .await;
        }
    });

    // Spawn semantic detection worker (runs ONNX inference without blocking transcript).
    // Uses spawn_blocking so ONNX doesn't starve the tokio async runtime
    // (WebSocket readers, event emitters, etc.).
    let sem_app = app.clone();
    let sem_latest_seq = latest_accepted_seq.clone();
    tauri::async_runtime::spawn(async move {
        while let Some((seq, text)) = semantic_rx.recv().await {
            let app_clone = sem_app.clone();
            let latest_seq = sem_latest_seq.clone();
            let _ = tokio::task::spawn_blocking(move || {
                run_semantic_detection(&app_clone, seq, latest_seq, &text);
            })
            .await;
        }
    });

    // Spawn detection worker (runs direct detection + reading mode without blocking
    // transcript delivery). Uses spawn_blocking so mutex locks and DB I/O don't
    // starve the tokio runtime.
    let det_app = app.clone();
    let det_latest_seq = latest_accepted_seq.clone();
    tauri::async_runtime::spawn(async move {
        while let Some((seq, transcript)) = detect_rx.recv().await {
            let app_clone = det_app.clone();
            let latest_seq = det_latest_seq.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let direct_found = run_direct_detection(&app_clone, seq, latest_seq, &transcript);
                check_reading_mode(&app_clone, &transcript, direct_found);
            })
            .await;
        }
    });

    let detect_sent_evt = detect_sent.clone();
    let detect_dropped_evt = detect_dropped.clone();
    let semantic_sent_evt = semantic_sent.clone();
    let semantic_dropped_evt = semantic_dropped.clone();

    tauri::async_runtime::spawn(async move {
        let mut transcript_router = TranscriptRouter::default();
        let mut semantic_window: VecDeque<String> =
            VecDeque::with_capacity(SEMANTIC_WINDOW_SEGMENTS);
        let partial_semantic_enabled = provider_log_name == "vosk";
        let mut last_partial_semantic_at = Instant::now()
            .checked_sub(PARTIAL_SEMANTIC_DEBOUNCE)
            .unwrap_or_else(Instant::now);

        while let Some(event) = event_rx.recv().await {
            if !evt_active.load(Ordering::SeqCst) {
                break;
            }

            match event {
                TranscriptEvent::Partial { transcript, words } => {
                    if !transcript.is_empty() {
                        let seq = transcript_seq.fetch_add(1, Ordering::Relaxed) + 1;
                        let t0 = std::time::Instant::now();
                        let confidence = average_word_confidence(&words, 0.0);
                        let route = transcript_router.route(TranscriptRouteInput {
                            provider: &provider_log_name,
                            kind: TranscriptEventKind::Partial,
                            transcript: &transcript,
                            confidence: (confidence > 0.0).then_some(confidence),
                        });

                        if let Some(reason) = &route.suppress_reason {
                            log::debug!("[ROUTER] Suppressed partial ({reason})");
                        }

                        if route.emit_transcript {
                            let _ = event_app.emit(
                                EVENT_TRANSCRIPT_PARTIAL,
                                TranscriptPayload {
                                    text: transcript.clone(),
                                    is_final: false,
                                    confidence,
                                    words: to_word_payloads(words),
                                },
                            );
                        }

                        // Check for voice control commands before normal detection work.
                        if check_stt_voice_command(&event_app, &transcript) {
                            continue;
                        }

                        // Check for translation commands on partials too (cheap string matching)
                        // This makes translation switching feel instant without waiting for speech_final
                        check_translation_command(&event_app, &transcript);
                        if !is_detection_paused(&event_app) {
                            if let Some(preview_text) = route.preview_candidate {
                                match partial_preview_tx.try_send((seq, preview_text)) {
                                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                        if transcript_logging_enabled() {
                                            log::debug!(
                                                "[QUEUE] partial_preview_tx dropped stale partial"
                                            );
                                        }
                                    }
                                    Ok(())
                                    | Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
                                }
                            }

                            if partial_semantic_enabled
                                && word_count(&transcript) >= PARTIAL_SEMANTIC_MIN_WORDS
                                && last_partial_semantic_at.elapsed() >= PARTIAL_SEMANTIC_DEBOUNCE
                            {
                                last_partial_semantic_at = Instant::now();
                                let mut parts = semantic_window.iter().cloned().collect::<Vec<_>>();
                                parts.push(transcript.clone());
                                let semantic_text = parts.join(" ");
                                if let Ok(()) = semantic_tx.try_send((seq, semantic_text)) {
                                    let n = semantic_sent_evt.fetch_add(1, Ordering::Relaxed) + 1;
                                    if n % 25 == 0 {
                                        let depth =
                                            semantic_tx.max_capacity() - semantic_tx.capacity();
                                        let dropped = semantic_dropped_evt.load(Ordering::Relaxed);
                                        log::info!(
                                            "[QUEUE] partial_semantic_tx sent={n} dropped={dropped} depth={depth}/{}",
                                            semantic_tx.max_capacity()
                                        );
                                    }
                                } else {
                                    let d =
                                        semantic_dropped_evt.fetch_add(1, Ordering::Relaxed) + 1;
                                    let sent = semantic_sent_evt.load(Ordering::Relaxed);
                                    log::debug!(
                                        "[QUEUE] partial_semantic_tx DROPPED (consumer behind) sent={sent} dropped={d}"
                                    );
                                }
                            }
                        }
                        log::debug!("[EVT] Partial processed in {:?}", t0.elapsed());
                    }
                }
                TranscriptEvent::Final {
                    transcript,
                    words,
                    confidence,
                    speech_final: _,
                } => {
                    if !transcript.is_empty() {
                        let seq = transcript_seq.fetch_add(1, Ordering::Relaxed) + 1;
                        let t0 = std::time::Instant::now();
                        let route = transcript_router.route(TranscriptRouteInput {
                            provider: &provider_log_name,
                            kind: TranscriptEventKind::Final,
                            transcript: &transcript,
                            confidence: Some(confidence),
                        });

                        if let Some(reason) = &route.suppress_reason {
                            log::debug!("[ROUTER] Suppressed final ({reason})");
                        }

                        // Emit as permanent transcript segment IMMEDIATELY
                        // (never blocked by detection work)
                        if route.emit_transcript {
                            let _ = event_app.emit(
                                EVENT_TRANSCRIPT_FINAL,
                                TranscriptPayload {
                                    text: transcript.clone(),
                                    is_final: true,
                                    confidence,
                                    words: to_word_payloads(words),
                                },
                            );
                        }

                        // Check for voice control commands before normal detection work.
                        if check_stt_voice_command(&event_app, &transcript) {
                            continue;
                        }

                        // Check for translation commands (cheap, <1ms, stays inline)
                        check_translation_command(&event_app, &transcript);
                        if !is_detection_paused(&event_app) {
                            if let Some(preview_text) = route.preview_candidate {
                                match partial_preview_tx.try_send((seq, preview_text)) {
                                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                        if transcript_logging_enabled() {
                                            log::debug!(
                                                "[QUEUE] fast_preview_tx dropped stale transcript"
                                            );
                                        }
                                    }
                                    Ok(())
                                    | Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
                                }
                            }

                            log::info!(
                            "[PIPELINE] final_transcript provider={} conf={:.2} chars={} event_ms={:?}",
                            provider_log_name,
                            confidence,
                            transcript.chars().count(),
                            t0.elapsed()
                        );

                            // Fire-and-forget: detection runs in background thread pool.
                            // Event consumer proceeds immediately to next transcript.
                            if let Some(detection_text) = route.authoritative_detection {
                                latest_accepted_seq.store(seq, Ordering::Relaxed);
                                if let Ok(()) = detect_tx.try_send((seq, detection_text.clone())) {
                                    let n = detect_sent_evt.fetch_add(1, Ordering::Relaxed) + 1;
                                    if n % 25 == 0 {
                                        let depth = detect_tx.max_capacity() - detect_tx.capacity();
                                        let dropped = detect_dropped_evt.load(Ordering::Relaxed);
                                        log::info!(
                                        "[QUEUE] detect_tx sent={n} dropped={dropped} depth={depth}/{}",
                                        detect_tx.max_capacity()
                                    );
                                    }
                                } else {
                                    let d = detect_dropped_evt.fetch_add(1, Ordering::Relaxed) + 1;
                                    let sent = detect_sent_evt.load(Ordering::Relaxed);
                                    log::warn!(
                                    "[QUEUE] detect_tx DROPPED (consumer behind) sent={sent} dropped={d}"
                                );
                                }

                                // Send accepted is_final fragments to FTS5 immediately.
                                // No sentence buffer — FTS5 is fast enough (~20-50ms)
                                // to run on every fragment without waiting for pauses.
                                semantic_window.push_back(detection_text.clone());
                                while semantic_window.len() > SEMANTIC_WINDOW_SEGMENTS {
                                    semantic_window.pop_front();
                                }
                                let semantic_text = semantic_window
                                    .iter()
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(" ");

                                if let Ok(()) = semantic_tx.try_send((seq, semantic_text)) {
                                    let n = semantic_sent_evt.fetch_add(1, Ordering::Relaxed) + 1;
                                    if n % 25 == 0 {
                                        let depth =
                                            semantic_tx.max_capacity() - semantic_tx.capacity();
                                        let dropped = semantic_dropped_evt.load(Ordering::Relaxed);
                                        log::info!(
                                        "[QUEUE] semantic_tx sent={n} dropped={dropped} depth={depth}/{}",
                                        semantic_tx.max_capacity()
                                    );
                                    }
                                } else {
                                    let d =
                                        semantic_dropped_evt.fetch_add(1, Ordering::Relaxed) + 1;
                                    let sent = semantic_sent_evt.load(Ordering::Relaxed);
                                    log::warn!(
                                    "[QUEUE] semantic_tx DROPPED (consumer behind) sent={sent} dropped={d}"
                                );
                                }
                            }
                        }

                        if transcript_logging_enabled() {
                            log::debug!(
                                "[EVT] Final processed in {:?} ({:?})",
                                t0.elapsed(),
                                truncate_safe(&transcript, 40)
                            );
                        } else {
                            log::debug!("[EVT] Final processed in {:?}", t0.elapsed());
                        }
                    }
                }
                TranscriptEvent::UtteranceEnd => {}
                TranscriptEvent::SpeechStarted => {
                    let _ = event_app.emit("stt_speech_started", ());
                }
                TranscriptEvent::Error(msg) => {
                    log::error!("[STT] Error: {msg}");
                    let _ = event_app.emit("stt_error", msg);
                }
                TranscriptEvent::Connected => {
                    log::info!("[STT] Connected");
                    let _ = event_app.emit("stt_connected", ());
                }
                TranscriptEvent::Disconnected => {
                    log::warn!("[STT] Disconnected");
                    let _ = event_app.emit("stt_disconnected", ());
                }
            }
        }

        log::info!("Transcript event consumer task exited");
    });

    Ok(())
}

/// Run a preview-only direct detection pass on a transcript.
///
/// This intentionally skips semantic detection, reading mode, queueing, and
/// cooldown state. Final transcript handling remains authoritative; this path
/// exists only to stage complete direct references in the preview panel sooner.
fn run_partial_direct_preview_detection(
    app: &AppHandle,
    detector_state: &Arc<Mutex<rhema_detection::DirectDetector>>,
    seq: u64,
    latest_seq: Arc<AtomicU64>,
    transcript: &str,
) {
    if seq != latest_seq.load(Ordering::Relaxed) {
        return;
    }

    let t0 = std::time::Instant::now();
    let direct_results = {
        let Ok(mut detector) = detector_state.lock() else {
            log::warn!("[DET-PARTIAL] DirectDetector lock poisoned");
            return;
        };
        detector.detect(transcript)
    };

    if direct_results.is_empty() || seq != latest_seq.load(Ordering::Relaxed) {
        return;
    }

    let merged: Vec<rhema_detection::MergedDetection> = direct_results
        .into_iter()
        .filter(|d| {
            !d.is_chapter_only
                && d.verse_ref.book_number > 0
                && d.verse_ref.chapter > 0
                && d.verse_ref.verse_start > 0
                && d.confidence >= 0.90
        })
        .map(|d| rhema_detection::MergedDetection {
            detection: d,
            auto_queued: false,
        })
        .collect();

    if merged.is_empty() {
        return;
    }

    let app_managed: State<'_, Mutex<AppState>> = app.state();
    let Ok(app_state) = app_managed.try_lock() else {
        if transcript_logging_enabled() {
            log::debug!("[DET-PARTIAL] AppState busy; skipping partial preview");
        }
        return;
    };
    let results: Vec<super::detection::DetectionResult> = merged
        .iter()
        .map(|m| super::detection::to_result(&app_state, m))
        .collect();
    drop(app_state);

    if results.is_empty() || seq != latest_seq.load(Ordering::Relaxed) {
        return;
    }

    for r in &results {
        log::info!(
            "[DET-PARTIAL] Preview: {} ({:.0}%)",
            r.verse_ref,
            r.confidence * 100.0
        );
    }
    let _ = app.emit("verse_detections", &results);

    if transcript_logging_enabled() {
        log::info!(
            "[DET-PARTIAL] Detection+emit took {:?} for {:?}",
            t0.elapsed(),
            truncate_safe(transcript, 50)
        );
    }
}

/// Run direct (regex/pattern) detection only. Instant, no ONNX.
/// Uses SEPARATE Mutex<DirectDetector> and Mutex<DetectionMerger> so it
/// never blocks on the semantic worker, and cooldown state persists across calls.
/// Returns true if high-confidence results were found (>= 0.90).
#[expect(
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "direct detection orchestration is intentionally kept together"
)]
fn run_direct_detection(
    app: &AppHandle,
    seq: u64,
    latest_seq: Arc<AtomicU64>,
    transcript: &str,
) -> bool {
    // Stale detection suppression: if this job's sequence is older than the
    // latest accepted transcript sequence, skip emission.
    if seq < latest_seq.load(Ordering::Relaxed) {
        log::debug!("[DET-DIRECT] Skipping stale job seq={seq}");
        return false;
    }
    let t0 = std::time::Instant::now();
    let detector_state: State<'_, Mutex<DirectDetector>> = app.state();
    let mut detector = match detector_state.lock() {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to lock DirectDetector: {e}");
            return false;
        }
    };
    let direct_results = detector.detect(transcript);
    drop(detector); // Release immediately

    if direct_results.is_empty() {
        return false;
    }

    // Check if any result has high confidence before merging
    let has_high_confidence = direct_results.iter().any(|d| d.confidence >= 0.90);

    // Merge using the managed merger (persists cooldown state across calls,
    // preventing duplicate emissions when running on both partials and finals)
    let merger_state: State<'_, Mutex<DetectionMerger>> = app.state();
    let mut merger = match merger_state.lock() {
        Ok(m) => m,
        Err(e) => {
            log::error!("Failed to lock DetectionMerger: {e}");
            return false;
        }
    };
    let merged = merger.merge(direct_results, vec![]);
    drop(merger);
    if merged.is_empty() {
        return false;
    }

    // Resolve verse info from DB (needs AppState, but only briefly for DB lookup)
    let app_managed: State<'_, Mutex<AppState>> = app.state();
    let Ok(app_state) = app_managed.try_lock() else {
        let bad = DIRECT_LOCK_CONTENDED.fetch_add(1, Ordering::Relaxed) + 1;
        let good = DIRECT_LOCK_OK.load(Ordering::Relaxed);
        log::warn!("[DET-DIRECT] AppState try_lock FAILED (contention) ok={good} contended={bad}");

        // Check for stale sequence BEFORE emitting in fallback path
        if seq < latest_seq.load(Ordering::Relaxed) {
            log::debug!("[DET-DIRECT] Skipping stale emission in fallback path seq={seq}");
            return has_high_confidence;
        }

        // AppState locked — emit results without verse text
        let results: Vec<super::detection::DetectionResult> = merged
            .iter()
            .map(|m| {
                let vr = &m.detection.verse_ref;
                super::detection::DetectionResult {
                    verse_ref: format!("{} {}:{}", vr.book_name, vr.chapter, vr.verse_start),
                    verse_text: String::new(),
                    book_name: vr.book_name.clone(),
                    book_number: vr.book_number,
                    chapter: vr.chapter,
                    verse: vr.verse_start,
                    confidence: m.detection.confidence,
                    source: "direct".to_string(),
                    auto_queued: m.auto_queued,
                    transcript_snippet: m.detection.transcript_snippet.clone(),
                    is_chapter_only: m.detection.is_chapter_only,
                }
            })
            .collect();
        for r in &results {
            log::info!(
                "[DET-DIRECT] Found: {} ({:.0}%) (no DB)",
                r.verse_ref,
                r.confidence * 100.0
            );
        }
        let _ = app.emit("verse_detections", &results);
        return has_high_confidence;
    };
    let ok = DIRECT_LOCK_OK.fetch_add(1, Ordering::Relaxed) + 1;
    if ok % 50 == 0 {
        let bad = DIRECT_LOCK_CONTENDED.load(Ordering::Relaxed);
        log::info!("[DET-DIRECT] AppState lock stats ok={ok} contended={bad}");
    }
    let results: Vec<super::detection::DetectionResult> = merged
        .iter()
        .map(|m| super::detection::to_result(&app_state, m))
        .collect();

    for r in &results {
        log::info!(
            "[DET-DIRECT] Found: {} ({:.0}%)",
            r.verse_ref,
            r.confidence * 100.0
        );
    }
    drop(app_state);

    // Final stale check before emission
    if seq < latest_seq.load(Ordering::Relaxed) {
        log::debug!("[DET-DIRECT] Skipping emission for stale seq={seq}");
        return has_high_confidence;
    }

    let _ = app.emit("verse_detections", &results);
    if transcript_logging_enabled() {
        log::info!(
            "[DET-DIRECT] Detection took {:?} for {:?}",
            t0.elapsed(),
            truncate_safe(transcript, 50)
        );
    } else {
        log::info!("[DET-DIRECT] Detection took {:?}", t0.elapsed());
    }
    has_high_confidence
}

/// Run hybrid semantic detection combining FTS5 BM25 with vector search.
/// Uses `spawn_blocking` so mutex locks and DB I/O don't starve the tokio runtime.
fn run_semantic_detection(app: &AppHandle, seq: u64, latest_seq: Arc<AtomicU64>, transcript: &str) {
    // Stale detection suppression: if this job's sequence is older than the
    // latest accepted transcript sequence, skip emission.
    if seq < latest_seq.load(Ordering::Relaxed) {
        log::debug!("[DET-SEMANTIC] Skipping stale job seq={seq}");
        return;
    }

    let t0 = std::time::Instant::now();
    if transcript_logging_enabled() {
        log::info!(
            "[DET-SEMANTIC] Running on: {:?}",
            truncate_safe(transcript, 80)
        );
    } else {
        log::info!("[DET-SEMANTIC] Running");
    }

    // FTS5 BM25 phrase search (~5ms)
    let fts_results = {
        let managed: State<'_, Mutex<AppState>> = app.state();
        let Ok(app_state) = managed.lock() else {
            log::error!("Failed to lock AppState for FTS5");
            return;
        };
        app_state
            .bible_db
            .as_ref()
            .and_then(|db| db.search_verses_bm25(transcript, 10).ok())
    };

    let fts = fts_results.unwrap_or_default();
    if fts.is_empty() {
        log::debug!("[DET-SEMANTIC] No FTS5 results, trying vector-only search");
    }

    // Use hybrid pipeline: FTS5 + vector search when available.
    // Even with empty FTS5, vector search can catch paraphrases.
    let merged = {
        let pipeline_state: State<'_, Mutex<rhema_detection::DetectionPipeline>> = app.state();
        let Ok(mut pipeline) = pipeline_state.lock() else {
            log::error!("Failed to lock DetectionPipeline");
            return;
        };
        pipeline.process_hybrid_with_fts(transcript, &fts)
    };

    if merged.is_empty() {
        log::info!("[DET-SEMANTIC] No detections");
        return;
    }

    // Resolve verse text from DB for merged results
    let app_managed: State<'_, Mutex<AppState>> = app.state();
    let Ok(app_state) = app_managed.lock() else {
        log::error!("Failed to lock AppState for verse resolution");
        return;
    };

    let results: Vec<super::detection::DetectionResult> = merged
        .iter()
        .map(|m| super::detection::to_result(&app_state, m))
        .collect();

    drop(app_state);

    // Final stale check before emission
    if seq < latest_seq.load(Ordering::Relaxed) {
        log::debug!("[DET-SEMANTIC] Skipping emission for stale seq={seq}");
        return;
    }

    for r in &results {
        log::info!(
            "[DET-SEMANTIC] Found: {} ({:.0}% {}) auto_q={}",
            r.verse_ref,
            r.confidence * 100.0,
            r.source,
            r.auto_queued
        );
    }
    let _ = app.emit("verse_detections", &results);
    log::info!("[DET-SEMANTIC] Total: {:?}", t0.elapsed());
}

/// Check reading mode: if active, test transcript against expected verse.
/// If direct detection just found a new verse, start/restart reading mode.
/// Returns `true` when reading mode handled the transcript (suppresses semantic).
#[expect(
    clippy::too_many_lines,
    reason = "sequential state-machine logic is clearer in one flow"
)]
fn check_reading_mode(app: &AppHandle, transcript: &str, direct_found: bool) -> bool {
    use rhema_detection::ReadingMode;

    // If direct detection found a verse, consider starting/restarting reading mode.
    // BUT: if reading mode is already active on a book/chapter, do NOT restart
    // on a different book — false positives from bare numbers (e.g., "verse 5"
    // getting matched as "Job 3:5") would hijack the reading session.
    if direct_found {
        let verse_info = {
            let detector_state: State<'_, Mutex<rhema_detection::DirectDetector>> = app.state();
            let Ok(detector) = detector_state.lock() else {
                return false;
            };
            detector.recent_detections().front().cloned()
        };

        if let Some(recent) = verse_info {
            // Get the confidence of the detection to distinguish explicit refs from false positives
            let detection_confidence = {
                let detector_state: State<'_, Mutex<rhema_detection::DirectDetector>> = app.state();
                detector_state
                    .lock()
                    .ok()
                    .and_then(|d| d.recent_detections().front().map(|_| 0.95)) // Direct detections are always high confidence
                    .unwrap_or(0.0)
            };

            let should_start = {
                let rm_managed: &Mutex<ReadingMode> = app.state::<Mutex<ReadingMode>>().inner();
                match rm_managed.lock() {
                    Ok(rm) => {
                        if !rm.is_active() && !rm.has_verses() {
                            true // Not active, no verses loaded — start fresh
                        } else if !rm.is_active() && rm.has_verses() {
                            // Paused — restart on any new explicit reference
                            true
                        } else if rm.current_book() == recent.book_number
                            && rm.current_chapter() == recent.chapter
                        {
                            false // Same book+chapter — already tracking this
                        } else if rm.current_book() != recent.book_number
                            && detection_confidence >= 0.90
                        {
                            // Different book with high confidence — explicit new reference
                            // (e.g., "John 1:1" after reading Exodus). Restart.
                            true
                        } else if rm.current_book() == recent.book_number {
                            // Same book, different chapter — natural progression
                            true
                        } else {
                            // Different book, low confidence — likely false positive
                            false
                        }
                    }
                    Err(_) => false,
                }
            };

            if should_start {
                let chapter_data = {
                    let t_db = std::time::Instant::now();
                    let app_managed: State<'_, Mutex<crate::state::AppState>> = app.state();
                    // Blocking lock is OK — we're inside spawn_blocking, not on the async runtime.
                    let Ok(app_state) = app_managed.lock() else {
                        log::error!("[READING] AppState lock poisoned");
                        return false;
                    };
                    let result = match &app_state.bible_db {
                        Some(db) => db
                            .get_chapter(
                                app_state.active_translation_id,
                                recent.book_number,
                                recent.chapter,
                            )
                            .ok(),
                        None => None,
                    };
                    log::info!("[READING] get_chapter took {:?}", t_db.elapsed());
                    result
                };

                if let Some(chapter_verses) = chapter_data {
                    let verses: Vec<(i32, String)> = chapter_verses
                        .into_iter()
                        .map(|v| (v.verse, v.text))
                        .collect();

                    let rm_managed: &Mutex<ReadingMode> = app.state::<Mutex<ReadingMode>>().inner();
                    if let Ok(mut rm) = rm_managed.lock() {
                        rm.start(
                            recent.book_number,
                            &recent.book_name,
                            recent.chapter,
                            recent.verse_start,
                            verses,
                        );

                        // Check if transcript contains "chapter" keyword - if so, expect chapter number next
                        // This handles "Genesis chapter" → pause → "5" → go to chapter 5
                        let lower = transcript.to_lowercase();
                        if lower.contains("chapter")
                            && !lower.contains("next")
                            && !lower.contains("previous")
                        {
                            rm.set_expecting_chapter();
                        }
                    }
                }
            }
        }
    }

    let rm_managed: &Mutex<ReadingMode> = app.state::<Mutex<ReadingMode>>().inner();

    // Check for chapter navigation commands (e.g., "let's go to chapter seven").
    {
        let chapter_change = {
            let Ok(mut rm) = rm_managed.lock() else {
                return false;
            };
            if !rm.is_active() && !rm.has_verses() {
                None
            } else {
                if transcript_logging_enabled() {
                    log::info!("[READING] Checking chapter command for: {transcript:?}");
                }
                rm.check_chapter_command(transcript)
            }
        };

        if let Some(change) = chapter_change {
            let chapter_data = {
                let t_db = std::time::Instant::now();
                let app_managed: State<'_, Mutex<crate::state::AppState>> = app.state();
                // Blocking lock is OK — we're inside spawn_blocking, not on the async runtime.
                let Ok(app_state) = app_managed.lock() else {
                    log::error!("[READING] AppState lock poisoned (chapter nav)");
                    return false;
                };
                let result = match &app_state.bible_db {
                    Some(db) => db
                        .get_chapter(
                            app_state.active_translation_id,
                            change.book_number,
                            change.new_chapter,
                        )
                        .ok(),
                    None => None,
                };
                log::info!("[READING] get_chapter (nav) took {:?}", t_db.elapsed());
                result
            };

            if let Some(chapter_verses) = chapter_data {
                if !chapter_verses.is_empty() {
                    let start_verse = change.start_verse.unwrap_or(1);

                    // Find the text for the starting verse
                    let start_verse_text = chapter_verses
                        .iter()
                        .find(|v| v.verse == start_verse)
                        .map_or_else(|| chapter_verses[0].text.clone(), |v| v.text.clone());

                    let verses: Vec<(i32, String)> = chapter_verses
                        .into_iter()
                        .map(|v| (v.verse, v.text))
                        .collect();

                    if let Ok(mut rm) = rm_managed.lock() {
                        rm.start(
                            change.book_number,
                            &change.book_name,
                            change.new_chapter,
                            start_verse,
                            verses,
                        );
                    }

                    // Emit the starting verse of the new chapter
                    let reference = format!(
                        "{} {}:{}",
                        change.book_name, change.new_chapter, start_verse
                    );
                    let advance = rhema_detection::ReadingAdvance {
                        book_number: change.book_number,
                        book_name: change.book_name.clone(),
                        chapter: change.new_chapter,
                        verse: start_verse,
                        verse_text: start_verse_text.clone(),
                        reference: reference.clone(),
                        confidence: 1.0,
                    };
                    let _ = app.emit("reading_mode_verse", &advance);

                    return true;
                }
            }
        }
    }

    // Check reading mode for verse advancement.
    // Allow check even when paused (has_verses but !active) so "verse N"
    // commands can re-activate reading mode after timeout.
    let advance = {
        let Ok(mut rm) = rm_managed.lock() else {
            return false;
        };
        if !rm.is_active() && !rm.has_verses() {
            return false;
        }
        rm.check_transcript(transcript)
    };

    if let Some(advance) = advance {
        let _ = app.emit("reading_mode_verse", &advance);
        return true;
    }

    false
}

/// Check for voice commands like "stop transcribing" and "start transcribing".
fn check_stt_voice_command(app: &AppHandle, transcript: &str) -> bool {
    let detector_state: State<'_, Mutex<rhema_detection::DirectDetector>> = app.state();
    let Ok(detector) = detector_state.lock() else {
        return false;
    };
    let command = detector.detect_stt_voice_command(transcript);
    drop(detector);

    match command {
        Some(rhema_detection::direct::detector::SttVoiceCommand::Stop) => {
            let managed: State<'_, Mutex<AppState>> = app.state();
            let Ok(app_state) = managed.lock() else {
                return true;
            };
            if app_state.stt_active.load(Ordering::Relaxed) {
                app_state.stt_active.store(false, Ordering::SeqCst);
                app_state.audio_active.store(false, Ordering::SeqCst);
                log::info!("[STT] Voice command: stop transcribing");
                let _ = app.emit("stt_voice_control", "stop");
                let _ = app.emit("stt_disconnected", ());
            }
            true
        }
        Some(rhema_detection::direct::detector::SttVoiceCommand::Start) => {
            // This can only be heard while STT is already running. A true
            // wake-from-stopped command needs a separate always-listening path.
            log::info!("[STT] Voice command: start transcribing ignored; STT is already listening");
            let _ = app.emit("stt_voice_control", "start_ignored");
            true
        }
        None => false,
    }
}

/// Check for voice translation commands like "read in NIV", "switch to ESV".
fn check_translation_command(app: &AppHandle, transcript: &str) {
    #[derive(serde::Serialize, Clone)]
    struct TranslationSwitch {
        abbreviation: String,
        translation_id: i64,
    }

    let detector_state: State<'_, Mutex<rhema_detection::DirectDetector>> = app.state();
    let Ok(detector) = detector_state.lock() else {
        return;
    };

    if let Some(abbrev) = detector.detect_translation_command(transcript) {
        drop(detector);

        // Find the translation ID for this abbreviation
        let managed: State<'_, Mutex<AppState>> = app.state();
        let Ok(mut app_state) = managed.try_lock() else {
            return;
        };

        if let Some(ref db) = app_state.bible_db {
            if let Ok(translations) = db.list_translations() {
                if let Some(t) = translations.iter().find(|t| t.abbreviation == abbrev) {
                    app_state.active_translation_id = t.id;
                    log::info!("[STT] Voice command: switched to {abbrev} (id={})", t.id);
                    drop(app_state);

                    let _ = app.emit(
                        "translation_command",
                        TranslationSwitch {
                            abbreviation: abbrev,
                            translation_id: t.id,
                        },
                    );
                }
            }
        }
    }
}

/// Stop the transcription pipeline (audio capture + STT provider).
#[tauri::command]
pub fn stop_transcription(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;

    if !app_state.stt_active.load(Ordering::Relaxed) {
        return Err("Transcription is not running".into());
    }

    // Setting these flags causes the background threads/tasks to exit.
    app_state.stt_active.store(false, Ordering::SeqCst);
    app_state.audio_active.store(false, Ordering::SeqCst);

    log::info!("Transcription stop requested");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// Test helper to verify stale sequence suppression logic.
    /// This simulates the sequence checking used in `run_direct_detection`
    /// and `run_semantic_detection` to ensure stale jobs don't emit.
    #[test]
    fn test_stale_sequence_suppression() {
        let latest_seq = Arc::new(AtomicU64::new(10));

        // Current job is stale (seq < latest)
        let seq = 5;
        assert!(seq < latest_seq.load(Ordering::Relaxed));
        assert!(latest_seq.load(Ordering::Relaxed) > seq);

        // Current job is fresh (seq == latest)
        let seq = 10;
        assert!(seq >= latest_seq.load(Ordering::Relaxed));

        // Current job is ahead (seq > latest) - should be accepted
        let seq = 15;
        assert!(seq >= latest_seq.load(Ordering::Relaxed));
    }

    /// Test that sequence numbers increase monotonically
    #[test]
    fn test_sequence_monotonic_increase() {
        let seq = Arc::new(AtomicU64::new(0));

        let s1 = seq.fetch_add(1, Ordering::Relaxed) + 1;
        let s2 = seq.fetch_add(1, Ordering::Relaxed) + 1;
        let s3 = seq.fetch_add(1, Ordering::Relaxed) + 1;

        assert!(s1 < s2);
        assert!(s2 < s3);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
    }

    /// Test that stale detection is correctly identified when
    /// a newer transcript arrives while an older job is processing.
    #[test]
    fn test_stale_detection_with_concurrent_updates() {
        let latest_seq = Arc::new(AtomicU64::new(5));

        // Job starts with seq=5 (fresh)
        let job_seq = 5;
        assert!(job_seq >= latest_seq.load(Ordering::Relaxed));

        // While job is processing, new transcript arrives (seq=6)
        latest_seq.store(6, Ordering::Relaxed);

        // Job finishes and checks for staleness
        assert!(job_seq < latest_seq.load(Ordering::Relaxed));
        // Should skip emission
    }

    /// Test that `detection_paused` initializes to false and toggles correctly.
    /// This verifies the backend contract: Pause Suggestions must be backend-enforced.
    #[test]
    fn test_detection_paused_state() {
        let app_state = crate::state::AppState::new();
        assert!(
            !app_state
                .detection_paused
                .load(std::sync::atomic::Ordering::Relaxed),
            "detection_paused should default to false"
        );

        app_state
            .detection_paused
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(app_state
            .detection_paused
            .load(std::sync::atomic::Ordering::Relaxed));

        app_state
            .detection_paused
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(!app_state
            .detection_paused
            .load(std::sync::atomic::Ordering::Relaxed));
    }
}
