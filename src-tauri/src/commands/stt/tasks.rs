//! Background task spawning helpers for the STT pipeline: panic-guarded
//! task wrappers, the latest-wins semantic detection worker loop, and the
//! shared live input gain handle.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use futures_util::FutureExt;
use tauri::AppHandle;
use tokio::sync::Notify;

use rhema_audio::{new_gain_handle, GainHandle};

use super::detection_jobs::{take_semantic_job, SemanticJob};
use super::live_session::run_semantic_detection;

static LIVE_INPUT_GAIN: OnceLock<GainHandle> = OnceLock::new();

pub(super) fn live_input_gain() -> GainHandle {
    LIVE_INPUT_GAIN.get_or_init(|| new_gain_handle(1.0)).clone()
}

pub(super) fn spawn_stt_task<F>(
    name: &'static str,
    future: F,
) -> tauri::async_runtime::JoinHandle<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        if AssertUnwindSafe(future).catch_unwind().await.is_err() {
            log::error!("[STT] Task {name} panicked");
        } else {
            log::debug!("[STT] Task {name} exited");
        }
    })
}

pub(super) fn spawn_latest_wins_semantic_worker(
    task_name: &'static str,
    job_label: &'static str,
    app: AppHandle,
    latest_seq: Arc<AtomicU64>,
    egw_cue_at_ms: Arc<AtomicU64>,
    job_slot: Arc<Mutex<Option<SemanticJob>>>,
    notify: Arc<Notify>,
) -> tauri::async_runtime::JoinHandle<()> {
    spawn_stt_task(task_name, async move {
        loop {
            notify.notified().await;

            while let Some(job) = take_semantic_job(&job_slot, job_label) {
                let SemanticJob {
                    seq,
                    text,
                    egw_text,
                    stt_confidence,
                } = job;
                let check_seq = latest_seq.load(Ordering::Acquire);
                if seq < check_seq {
                    log::debug!(
                        "[DET-SEMANTIC] Skipping stale {job_label} job seq={seq} latest={check_seq}",
                    );
                    continue;
                }

                let app_clone = app.clone();
                let latest_seq = latest_seq.clone();
                let egw_cue_at_ms = egw_cue_at_ms.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    run_semantic_detection(
                        &app_clone,
                        seq,
                        &latest_seq,
                        &egw_cue_at_ms,
                        &text,
                        &egw_text,
                        stt_confidence,
                    );
                })
                .await;
            }
        }
    })
}
