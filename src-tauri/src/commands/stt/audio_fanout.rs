//! Microphone capture and fan-out thread for the STT pipeline.
//!
//! `cpal`'s `Stream` is `!Send`, so capture lives on its own OS thread rather
//! than the tokio runtime. The thread rebuilds the capture whenever the OS
//! device disappears and reappears, emits the level meter, and forwards every
//! frame to the STT provider channel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use rhema_audio::{AudioConfig, AudioFrame, GainHandle};

use super::session::AudioSessionGuard;
use crate::events::{
    AudioLevelPayload, EVENT_AUDIO_LEVEL, EVENT_AUDIO_SOURCE_LOST, EVENT_AUDIO_SOURCE_RECOVERED,
};

/// Spawn the audio-capture + fan-out thread.
///
/// Clears `stt_active`/`audio_active` and returns `Err` if the thread cannot be
/// spawned, so the caller can abort startup without leaking the active flags.
pub(super) fn spawn(
    app: AppHandle,
    session: AudioSessionGuard,
    device_id: Option<String>,
    gain_handle: GainHandle,
    audio_send_tx: crossbeam_channel::Sender<Vec<i16>>,
    stt_active: Arc<AtomicBool>,
    audio_active: Arc<AtomicBool>,
) -> Result<(), String> {
    let fan_active = stt_active.clone();
    let fan_app = app;
    let fan_session = session;

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
                if !fan_active.load(Ordering::SeqCst) || !fan_session.is_current() {
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
                        }
                        announce_source_lost(&fan_app, &mut announced_lost);
                        if !fan_session.sleep_interruptible(
                            Duration::from_millis(750),
                            Duration::from_millis(50),
                        ) {
                            break 'outer;
                        }
                        continue 'outer;
                    }
                };

                log::info!("Audio capture started on fanout thread");

                let mut last_frame_at = Instant::now();

                // Inner loop: pump frames until loss is detected or stop is requested.
                loop {
                    if !fan_active.load(Ordering::SeqCst) || !fan_session.is_current() {
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
                        announce_source_lost(&fan_app, &mut announced_lost);
                        break; // drop `capture`, outer loop rebuilds
                    }

                    match audio_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(frame) => {
                            last_frame_at = Instant::now();
                            frame_count += 1;

                            // (a) Compute audio levels at ~15 Hz
                            //     At 16 kHz with ~1024-sample frames, every 4th frame is ~15 Hz.
                            if frame_count % 4 == 0 {
                                emit_level(&fan_app, &frame.samples);
                            }

                            // (b) Forward all audio to the STT provider.
                            if !forward_frame(&audio_send_tx, frame.samples) {
                                break 'outer;
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

            log::info!(
                "[AUDIO] Capture stopped on fanout thread (session_retired={})",
                !fan_session.is_current()
            );
        })
        .map_err(|e| {
            stt_active.store(false, Ordering::SeqCst);
            audio_active.store(false, Ordering::SeqCst);
            format!("Failed to spawn audio fanout thread: {e}")
        })?;

    Ok(())
}

/// Announce source loss once per outage and zero the level meter, so the UI
/// shows the gap instead of freezing on the last reading.
fn announce_source_lost(app: &AppHandle, announced: &mut bool) {
    if *announced {
        return;
    }
    let _ = app.emit(EVENT_AUDIO_SOURCE_LOST, ());
    let _ = app.emit(
        EVENT_AUDIO_LEVEL,
        AudioLevelPayload {
            rms: 0.0,
            peak: 0.0,
        },
    );
    *announced = true;
}

fn emit_level(app: &AppHandle, samples: &[i16]) {
    let level = rhema_audio::meter::compute_level(samples);
    let _ = app.emit(
        EVENT_AUDIO_LEVEL,
        AudioLevelPayload {
            rms: level.rms,
            peak: level.peak,
        },
    );
}

/// Forward one frame to the STT provider. A short timeout avoids silently
/// dropping speech during transient provider backpressure.
///
/// Returns `false` when the provider channel is gone and fan-out must stop.
fn forward_frame(tx: &crossbeam_channel::Sender<Vec<i16>>, samples: Vec<i16>) -> bool {
    match tx.send_timeout(samples, Duration::from_millis(20)) {
        Ok(()) => true,
        Err(crossbeam_channel::SendTimeoutError::Timeout(_)) => {
            log::warn!("[AUDIO] Dropped STT frame: provider queue full");
            true
        }
        Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => {
            log::info!("[AUDIO] Provider channel disconnected; stopping fanout");
            false
        }
    }
}
