//! Semantic/direct job scheduling + buffering for the live detection loop.
//!
//! This is the "latest-wins slot" and utterance-buffering machinery that the
//! transcript event loop in `mod.rs` feeds and the detection workers drain. It
//! owns no `AppHandle` or IPC — only the shared `Arc<Mutex<…>>` slots, the
//! `Notify`, the mpsc sender, and the atomic counters passed in by the caller.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use super::detection::{FINAL_SEMANTIC_MIN_WORDS, LIVE_SEMANTIC_CAP, LIVE_SEMANTIC_OVERLAP_BOOST};
use super::detection_logic::transcript_defers_to_direct;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SemanticJob {
    pub(crate) seq: u64,
    pub(crate) text: String,
    /// Wider trailing window used only for EGW quote run-matching. See
    /// `LIVE_EGW_QUOTE_WINDOW_WORDS` — Bible detection keeps the tight `text`.
    pub(crate) egw_text: String,
    pub(crate) stt_confidence: f64,
}

/// Take the latest pending semantic job from a shared slot, recovering from
/// poisoned locks so the worker doesn't die permanently.
pub(crate) fn take_semantic_job(
    slot: &Arc<Mutex<Option<SemanticJob>>>,
    label: &str,
) -> Option<SemanticJob> {
    match slot.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => {
            log::error!("[DET-SEMANTIC] {label} semantic slot lock poisoned; recovering");
            let mut guard = poisoned.into_inner();
            guard.take()
        }
    }
}

/// Replace the latest pending semantic job in a shared slot, recovering from
/// poisoned locks. Returns true if a previous job was replaced.
pub(crate) fn replace_semantic_job(
    slot: &Arc<Mutex<Option<SemanticJob>>>,
    job: SemanticJob,
    label: &str,
) -> bool {
    match slot.lock() {
        Ok(mut guard) => guard.replace(job).is_some(),
        Err(poisoned) => {
            log::error!("[DET-SEMANTIC] {label} semantic slot lock poisoned; recovering");
            let mut guard = poisoned.into_inner();
            guard.replace(job).is_some()
        }
    }
}

pub(crate) fn enqueue_final_semantic_job(
    job_slot: &Arc<Mutex<Option<SemanticJob>>>,
    notify: &Arc<Notify>,
    sent_counter: &Arc<AtomicU64>,
    replaced_counter: &Arc<AtomicU64>,
    seq: u64,
    text: String,
    egw_text: String,
    stt_confidence: f64,
) {
    if text.trim().is_empty() {
        return;
    }

    if text.split_whitespace().count() < FINAL_SEMANTIC_MIN_WORDS {
        log::debug!("[DET-TRACE] seq={seq} skip=semantic_enqueue reason=tiny_window label=final");
        return;
    }

    // Explicit references and voice commands are owned by the direct + command
    // paths. Skipping the enqueue (rather than suppressing later in the worker)
    // also prevents these utterances from evicting a pending prose job from the
    // latest-wins slot, so genuine paraphrase detections still run.
    if transcript_defers_to_direct(&text) {
        log::debug!(
            "[DET-TRACE] seq={seq} skip=semantic_enqueue reason=reference_or_command label=final"
        );
        return;
    }

    let replaced = replace_semantic_job(
        job_slot,
        SemanticJob {
            seq,
            text,
            egw_text,
            stt_confidence,
        },
        "final",
    );
    let n = sent_counter.fetch_add(1, Ordering::Relaxed) + 1;

    if replaced {
        let replaced_count = replaced_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let sent = sent_counter.load(Ordering::Relaxed);
        log::debug!(
            "[QUEUE] final_semantic latest-wins replaced stale work sent={sent} replaced={replaced_count}"
        );
    } else if n % 25 == 0 {
        let replaced_count = replaced_counter.load(Ordering::Relaxed);
        log::info!("[QUEUE] final_semantic latest-wins sent={n} replaced={replaced_count}");
    }

    notify.notify_one();
}

pub(crate) fn enqueue_partial_semantic_job(
    job_slot: &Arc<Mutex<Option<SemanticJob>>>,
    notify: &Arc<Notify>,
    sent_counter: &Arc<AtomicU64>,
    replaced_counter: &Arc<AtomicU64>,
    seq: u64,
    text: String,
    egw_text: String,
    stt_confidence: f64,
) {
    if text.trim().is_empty() {
        return;
    }

    if transcript_defers_to_direct(&text) {
        log::debug!(
            "[DET-TRACE] seq={seq} skip=semantic_enqueue reason=reference_or_command label=partial"
        );
        return;
    }

    let replaced = replace_semantic_job(
        job_slot,
        SemanticJob {
            seq,
            text,
            egw_text,
            stt_confidence,
        },
        "partial",
    );
    let n = sent_counter.fetch_add(1, Ordering::Relaxed) + 1;

    if replaced {
        let replaced_count = replaced_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let sent = sent_counter.load(Ordering::Relaxed);
        log::debug!(
            "[QUEUE] partial_semantic latest-wins replaced stale work sent={sent} replaced={replaced_count}"
        );
    } else if n % 25 == 0 {
        let replaced_count = replaced_counter.load(Ordering::Relaxed);
        log::info!("[QUEUE] partial_semantic latest-wins sent={n} replaced={replaced_count}");
    }

    notify.notify_one();
}

pub(crate) fn enqueue_direct_detection_job(
    detect_tx: &tokio::sync::mpsc::Sender<(u64, String)>,
    latest_accepted_seq: &Arc<AtomicU64>,
    sent_counter: &Arc<AtomicU64>,
    dropped_counter: &Arc<AtomicU64>,
    seq: u64,
    text: String,
    source: &str,
) {
    match detect_tx.try_send((seq, text)) {
        Ok(()) => {
            latest_accepted_seq.store(seq, Ordering::Release);
            let n = sent_counter.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 25 == 0 {
                let depth = detect_tx.max_capacity() - detect_tx.capacity();
                let dropped = dropped_counter.load(Ordering::Relaxed);
                log::info!(
                    "[QUEUE] detect_tx source={source} sent={n} dropped={dropped} depth={depth}/{}",
                    detect_tx.max_capacity()
                );
            }
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            let dropped = dropped_counter.fetch_add(1, Ordering::Relaxed) + 1;
            let sent = sent_counter.load(Ordering::Relaxed);
            log::warn!(
                "[QUEUE] detect_tx DROPPED source={source} (consumer behind) sent={sent} dropped={dropped}"
            );
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
    }
}

#[derive(Debug, Default)]
pub(crate) struct DeepgramSemanticBuffer {
    parts: Vec<String>,
    seq: u64,
}

impl DeepgramSemanticBuffer {
    pub(crate) fn push_final(
        &mut self,
        seq: u64,
        text: String,
        speech_final: bool,
    ) -> Option<(u64, String)> {
        self.parts.push(text);
        self.seq = seq;
        if speech_final {
            self.flush_with_seq(seq)
        } else {
            None
        }
    }

    pub(crate) fn flush(&mut self) -> Option<(u64, String)> {
        self.flush_with_seq(self.seq)
    }

    pub(crate) fn flush_when_enabled(&mut self, enabled: bool) -> Option<(u64, String)> {
        if enabled {
            self.flush()
        } else {
            self.clear();
            None
        }
    }

    pub(crate) fn flush_with_seq(&mut self, seq: u64) -> Option<(u64, String)> {
        if self.parts.is_empty() || seq == 0 {
            return None;
        }

        let text = self.parts.join(" ");
        self.clear();
        Some((seq, text))
    }

    pub(crate) fn clear(&mut self) {
        self.parts.clear();
        self.seq = 0;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

fn semantic_result_key(result: &crate::commands::detection::DetectionResult) -> String {
    if result.content_type == "egw" {
        return format!(
            "egw:{}:{}:{}",
            result.book_number, result.chapter, result.verse
        );
    }
    if result.book_number > 0 && result.chapter > 0 && result.verse > 0 {
        format!("{}:{}:{}", result.book_number, result.chapter, result.verse)
    } else {
        result.verse_ref.clone()
    }
}

pub(crate) fn finalize_live_semantic_results(
    results: Vec<crate::commands::detection::DetectionResult>,
    min_confidence: f64,
) -> Vec<crate::commands::detection::DetectionResult> {
    let mut grouped: HashMap<String, (crate::commands::detection::DetectionResult, usize)> =
        HashMap::new();

    for result in results {
        if result.confidence < min_confidence {
            continue;
        }
        let key = semantic_result_key(&result);
        match grouped.get_mut(&key) {
            Some((existing, overlap_count)) => {
                *overlap_count += 1;
                existing.confidence = existing.confidence.max(result.confidence);
                existing.auto_queued |= result.auto_queued;
                if existing.verse_text.is_empty() && !result.verse_text.is_empty() {
                    existing.verse_text.clone_from(&result.verse_text);
                }
                if existing.transcript_snippet.is_empty() && !result.transcript_snippet.is_empty() {
                    existing
                        .transcript_snippet
                        .clone_from(&result.transcript_snippet);
                }
                if existing.book_number <= 0 && result.book_number > 0 {
                    *existing = result;
                }
            }
            None => {
                grouped.insert(key, (result, 1));
            }
        }
    }

    let mut merged = grouped
        .into_values()
        .map(|(mut result, overlap_count)| {
            if overlap_count > 1 {
                result.confidence = (result.confidence + LIVE_SEMANTIC_OVERLAP_BOOST).min(0.98);
            }
            result
        })
        .collect::<Vec<_>>();

    merged.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.verse_ref.cmp(&b.verse_ref))
    });
    merged.truncate(LIVE_SEMANTIC_CAP);
    merged
}
