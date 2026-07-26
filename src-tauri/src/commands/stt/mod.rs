#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]

mod detection;
mod detection_jobs;
mod detection_logic;
mod live_session;
mod provider;
mod tasks;
mod utils;
mod voice;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Notify;

use crate::events::{
    AudioLevelPayload, TranscriptPayload, EVENT_AUDIO_LEVEL, EVENT_AUDIO_SOURCE_LOST,
    EVENT_AUDIO_SOURCE_RECOVERED, EVENT_TRANSCRIPT_FINAL, EVENT_TRANSCRIPT_PARTIAL,
};
use crate::state::AppState;
use rhema_audio::{set_gain, AudioConfig, AudioFrame};
use rhema_detection::DirectDetector;
use rhema_stt::TranscriptEvent;

use self::detection::{
    is_detection_paused, is_semantic_detection_enabled, LIVE_DETECTION_WINDOW_WORDS,
    PARTIAL_SEMANTIC_DEBOUNCE, PARTIAL_SEMANTIC_MIN_WORDS, SEMANTIC_WINDOW_SEGMENTS,
    WINDOW_RESET_GAP,
};
use self::detection_jobs::{
    enqueue_direct_detection_job, enqueue_final_semantic_job, enqueue_partial_semantic_job,
    DeepgramSemanticBuffer,
};
use self::detection_logic::{
    clamp_to_recent_words, trim_to_sentence_start, SENTENCE_TRIM_MIN_WORDS,
};
use self::live_session::{check_reading_mode, run_direct_detection};
use self::provider::build_stt_provider;
use self::tasks::{live_input_gain, spawn_latest_wins_semantic_worker, spawn_stt_task};
use self::utils::{
    average_word_confidence, final_semantic_detection_allowed_by_settings,
    partial_semantic_detection_enabled_for_provider, to_word_payloads, transcript_logging_enabled,
    truncate_safe, word_count,
};
use self::voice::{check_stt_voice_command, check_translation_command};
use crate::commands::transcript_router::{
    TranscriptEventKind, TranscriptRouteInput, TranscriptRouter,
};

/// Start the audio-capture-to-transcription pipeline: mic capture, STT provider,
/// transcript events, and background detection workers.
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
    stt_language: Option<String>,
    low_power: Option<bool>,
) -> Result<(), String> {
    // Guard: already running?
    let (stt_active, audio_active) = {
        let app_state = state.lock().map_err(|e| e.to_string())?;
        (app_state.stt_active.clone(), app_state.audio_active.clone())
    };

    if stt_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Transcription is already running".into());
    }

    let provider_name = provider.as_deref().unwrap_or("vosk");
    let stt_language = stt_language.as_deref().unwrap_or("en");

    {
        let detector_state: State<'_, Mutex<DirectDetector>> = app.state();
        if let Ok(mut detector) = detector_state.lock() {
            detector.set_stt_language(stt_language);
        };
    }

    // Build the STT provider.
    let stt_provider = match build_stt_provider(
        provider_name,
        &app,
        device_id.as_deref(),
        gain,
        Some(stt_language),
    )
    .await
    {
        Ok(provider) => provider,
        Err(error) => {
            log::error!(
                "[STT] start_transcription failed to build provider {provider_name}: {error}"
            );
            stt_active.store(false, Ordering::SeqCst);
            return Err(error);
        }
    };

    audio_active.store(true, Ordering::SeqCst);

    // Prepare channels.
    let (audio_send_tx, audio_send_rx) = crossbeam_channel::bounded::<Vec<i16>>(128);

    // Spawn audio-capture + fan-out thread (cpal `Stream` is !Send).
    let gain_val = gain.unwrap_or(1.0).clamp(0.0, 2.0);
    let gain_handle = live_input_gain();
    set_gain(&gain_handle, gain_val);
    let fan_active = stt_active.clone();
    let fan_app = app.clone();

    std::thread::Builder::new()
        .name("audio-fanout".into())
        .spawn(move || {
            // Watchdog flag — set by cpal's stream-error callback when the OS
            // device vanishes. The outer loop polls this (and frame silence)
            // to detect loss and rebuild the capture once the device returns.
            let device_lost = Arc::new(std::sync::atomic::AtomicBool::new(false));
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
                    gain: rhema_audio::read_gain(&gain_handle),
                };

                let (audio_tx, audio_rx) = crossbeam_channel::bounded::<AudioFrame>(128);
                device_lost.store(false, Ordering::SeqCst);

                let capture = match rhema_audio::capture::start(
                    config,
                    audio_tx,
                    device_lost.clone(),
                    gain_handle.clone(),
                ) {
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
                            log::warn!("[AUDIO] Source unavailable: {e} — waiting for reconnect");
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

    // Spawn STT provider and transcript event workers on the tokio runtime.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TranscriptEvent>(128);
    let mut task_handles = Vec::new();

    let conn_active = stt_active.clone();
    let conn_audio_active = audio_active.clone();
    let provider_app = app.clone();
    let provider_log_name = stt_provider.name().to_string();
    let provider_log_name_task_a = provider_log_name.clone();

    // Task A: run the STT provider (Deepgram WS+REST or Vosk local).
    task_handles.push(spawn_stt_task("provider", async move {
        let result = stt_provider.start(audio_send_rx, event_tx).await;
        if let Err(e) = result {
            log::error!("[STT-{provider_log_name_task_a}] Provider failed: {e}");
            let _ = provider_app.emit("stt_error", e.to_string());
            let _ = provider_app.emit("stt_disconnected", ());
        }
        conn_active.store(false, Ordering::SeqCst);
        conn_audio_active.store(false, Ordering::SeqCst);
        log::info!("[STT-{provider_log_name_task_a}] Provider task exited");
    }));

    // Task B: consume TranscriptEvents, emit to frontend, run detection
    let evt_active = stt_active.clone();
    let event_app = app.clone();

    // Final and partial semantic detection each use latest-wins storage so
    // fresh speech can replace stale queued work instead of being dropped.
    let final_semantic_job = Arc::new(Mutex::new(None::<detection_jobs::SemanticJob>));
    let final_semantic_notify = Arc::new(Notify::new());
    let partial_semantic_job = Arc::new(Mutex::new(None::<detection_jobs::SemanticJob>));
    let partial_semantic_notify = Arc::new(Notify::new());

    // Background detection channel — direct + reading mode, non-blocking
    let (detect_tx, mut detect_rx) = tokio::sync::mpsc::channel::<(u64, String)>(64);

    // [DIAG] Counters so we can see whether transcripts are being dropped
    // because the detection workers can't keep up. Logged every 25 sends
    // alongside current queue depth.
    let detect_sent = Arc::new(AtomicU64::new(0));
    let detect_dropped = Arc::new(AtomicU64::new(0));
    let semantic_sent = Arc::new(AtomicU64::new(0));
    let semantic_dropped = Arc::new(AtomicU64::new(0));
    let transcript_seq = Arc::new(AtomicU64::new(0));
    let latest_accepted_seq = Arc::new(AtomicU64::new(0));
    let egw_cue_at_ms = Arc::new(AtomicU64::new(0));

    task_handles.push(spawn_latest_wins_semantic_worker(
        "final-semantic",
        "final",
        app.clone(),
        latest_accepted_seq.clone(),
        egw_cue_at_ms.clone(),
        final_semantic_job.clone(),
        final_semantic_notify.clone(),
    ));
    task_handles.push(spawn_latest_wins_semantic_worker(
        "partial-semantic",
        "partial",
        app.clone(),
        transcript_seq.clone(),
        egw_cue_at_ms,
        partial_semantic_job.clone(),
        partial_semantic_notify.clone(),
    ));

    // Detection worker: direct detection + reading mode on spawn_blocking.
    let det_app = app.clone();
    let det_latest_seq = latest_accepted_seq.clone();
    task_handles.push(spawn_stt_task("detection", async move {
        while let Some((seq, transcript)) = detect_rx.recv().await {
            let app_clone = det_app.clone();
            let latest_seq = det_latest_seq.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let direct_candidates =
                    run_direct_detection(&app_clone, seq, &latest_seq, &transcript);
                check_reading_mode(&app_clone, &transcript, direct_candidates);
            })
            .await;
        }
    }));

    let detect_sent_evt = detect_sent.clone();
    let detect_dropped_evt = detect_dropped.clone();
    let semantic_sent_evt = semantic_sent.clone();
    let semantic_dropped_evt = semantic_dropped.clone();
    let final_semantic_job_evt = final_semantic_job.clone();
    let final_semantic_notify_evt = final_semantic_notify.clone();
    let partial_semantic_job_evt = partial_semantic_job.clone();
    let partial_semantic_notify_evt = partial_semantic_notify.clone();

    task_handles.push(spawn_stt_task("event-router", async move {
        let mut transcript_router = TranscriptRouter::default();
        let mut semantic_window: VecDeque<String> =
            VecDeque::with_capacity(SEMANTIC_WINDOW_SEGMENTS);
        let mut last_final_at: Option<Instant> = None;
        let partial_semantic_enabled =
            partial_semantic_detection_enabled_for_provider(low_power, &provider_log_name);
        let deepgram_semantic_on_speech_final = false;
        let mut deepgram_semantic_buffer = DeepgramSemanticBuffer::default();
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
                                    provider: provider_log_name.clone(),
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
                            if let Some(detection_text) = route.authoritative_detection {
                                enqueue_direct_detection_job(
                                    &detect_tx,
                                    &latest_accepted_seq,
                                    &detect_sent_evt,
                                    &detect_dropped_evt,
                                    seq,
                                    detection_text,
                                    "deepgram_partial",
                                );
                            }

                            if partial_semantic_enabled
                                && is_semantic_detection_enabled(&event_app)
                                && word_count(&transcript) >= PARTIAL_SEMANTIC_MIN_WORDS
                                && last_partial_semantic_at.elapsed() >= PARTIAL_SEMANTIC_DEBOUNCE
                            {
                                last_partial_semantic_at = Instant::now();
                                let mut parts = semantic_window.iter().cloned().collect::<Vec<_>>();
                                parts.push(transcript.clone());
                                let semantic_text = trim_to_sentence_start(
                                    &clamp_to_recent_words(
                                        &parts.join(" "),
                                        LIVE_DETECTION_WINDOW_WORDS,
                                    ),
                                    SENTENCE_TRIM_MIN_WORDS,
                                );
                                enqueue_partial_semantic_job(
                                    &partial_semantic_job_evt,
                                    &partial_semantic_notify_evt,
                                    &semantic_sent_evt,
                                    &semantic_dropped_evt,
                                    seq,
                                    semantic_text,
                                    confidence,
                                );
                            }
                        }
                        log::debug!("[EVT] Partial processed in {:?}", t0.elapsed());
                    }
                }
                TranscriptEvent::Final {
                    transcript,
                    words,
                    confidence,
                    speech_final,
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
                                    provider: provider_log_name.clone(),
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
                        let detection_paused = is_detection_paused(&event_app);
                        let semantic_detection_enabled = is_semantic_detection_enabled(&event_app);
                        if (!semantic_detection_enabled || detection_paused)
                            && deepgram_semantic_on_speech_final
                            && speech_final
                        {
                            deepgram_semantic_buffer.clear();
                        }
                        if !detection_paused {
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
                                let final_semantic_allowed =
                                    final_semantic_detection_allowed_by_settings(
                                        semantic_detection_enabled,
                                        &provider_log_name,
                                        confidence,
                                        transcript.chars().count(),
                                    );
                                enqueue_direct_detection_job(
                                    &detect_tx,
                                    &latest_accepted_seq,
                                    &detect_sent_evt,
                                    &detect_dropped_evt,
                                    seq,
                                    detection_text.clone(),
                                    "final",
                                );

                                // Deepgram waits for speech_final before semantic search.
                                // Non-Deepgram providers keep the rolling final window.
                                if !final_semantic_allowed {
                                    if semantic_detection_enabled {
                                        log::debug!(
                                            "[DET-TRACE] seq={seq} skip=semantic_enqueue reason=low_confidence provider={provider_log_name} confidence={confidence:.2}"
                                        );
                                    }
                                } else if deepgram_semantic_on_speech_final {
                                    if let Some((semantic_seq, semantic_text)) =
                                        deepgram_semantic_buffer.push_final(
                                            seq,
                                            detection_text,
                                            speech_final,
                                        )
                                    {
                                        enqueue_final_semantic_job(
                                            &final_semantic_job_evt,
                                            &final_semantic_notify_evt,
                                            &semantic_sent_evt,
                                            &semantic_dropped_evt,
                                            semantic_seq,
                                            semantic_text,
                                            confidence,
                                        );
                                    }
                                } else {
                                    if last_final_at.is_some_and(|t| t.elapsed() >= WINDOW_RESET_GAP) {
                                        semantic_window.clear();
                                    }
                                    last_final_at = Some(Instant::now());
                                    semantic_window.push_back(detection_text.clone());
                                    while semantic_window.len() > SEMANTIC_WINDOW_SEGMENTS {
                                        semantic_window.pop_front();
                                    }
                                    let semantic_text = trim_to_sentence_start(
                                        &clamp_to_recent_words(
                                            &semantic_window
                                                .iter()
                                                .cloned()
                                                .collect::<Vec<_>>()
                                                .join(" "),
                                            LIVE_DETECTION_WINDOW_WORDS,
                                        ),
                                        SENTENCE_TRIM_MIN_WORDS,
                                    );
                                    enqueue_final_semantic_job(
                                        &final_semantic_job_evt,
                                        &final_semantic_notify_evt,
                                        &semantic_sent_evt,
                                        &semantic_dropped_evt,
                                        seq,
                                        semantic_text,
                                        confidence,
                                    );
                                }
                            } else if semantic_detection_enabled
                                && deepgram_semantic_on_speech_final
                                && speech_final
                                && !deepgram_semantic_buffer.is_empty()
                            {
                                // A duplicate speech_final result can be suppressed by the
                                // transcript router; it still marks the buffered utterance ready.
                                if let Some((semantic_seq, semantic_text)) =
                                    deepgram_semantic_buffer.flush_with_seq(seq)
                                {
                                    enqueue_final_semantic_job(
                                        &final_semantic_job_evt,
                                        &final_semantic_notify_evt,
                                        &semantic_sent_evt,
                                        &semantic_dropped_evt,
                                        semantic_seq,
                                        semantic_text,
                                        confidence,
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
                TranscriptEvent::UtteranceEnd => {
                    if deepgram_semantic_on_speech_final {
                        let pending = deepgram_semantic_buffer
                            .flush_when_enabled(is_semantic_detection_enabled(&event_app));
                        if !is_detection_paused(&event_app) {
                            if let Some((semantic_seq, semantic_text)) = pending {
                                enqueue_final_semantic_job(
                                    &final_semantic_job_evt,
                                    &final_semantic_notify_evt,
                                    &semantic_sent_evt,
                                    &semantic_dropped_evt,
                                    semantic_seq,
                                    semantic_text,
                                    0.0,
                                );
                            }
                        }
                    }
                }
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
    }));

    let stale_handles = match state.lock() {
        Ok(mut app_state) => app_state.replace_stt_task_handles(task_handles),
        Err(e) => {
            for handle in task_handles {
                handle.abort();
            }
            stt_active.store(false, Ordering::SeqCst);
            audio_active.store(false, Ordering::SeqCst);
            return Err(e.to_string());
        }
    };
    for handle in stale_handles {
        handle.abort();
    }

    Ok(())
}

/// Update input gain for an active capture without restarting transcription.
#[tauri::command]
pub fn set_input_gain(gain: f32) {
    let handle = live_input_gain();
    set_gain(&handle, gain);
}

/// Stop the transcription pipeline (audio capture + STT provider).
#[tauri::command]
pub fn stop_transcription(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;

    if !app_state.stt_active.swap(false, Ordering::SeqCst) {
        return Err("Transcription is not running".into());
    }

    // Setting these flags causes the background threads/tasks to exit.
    app_state.audio_active.store(false, Ordering::SeqCst);
    let task_handles = app_state.take_stt_task_handles();
    drop(app_state);

    for handle in task_handles {
        handle.abort();
    }

    log::info!("Transcription stop requested");
    Ok(())
}
