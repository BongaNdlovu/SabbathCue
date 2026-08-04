//! Live STT detection session: direct, semantic, and reading-mode orchestration.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use rhema_detection::{DetectionMerger, DirectDetector, ReadingMode};

use super::detection::{
    is_bible_detection_enabled, is_semantic_detection_enabled, FINAL_SEMANTIC_MIN_WORDS,
};
use super::detection_jobs::finalize_live_semantic_results;
use super::detection_logic::{
    choose_reading_candidate, direct_reading_candidates, filter_direct_results_to_scope_if_present,
    filter_semantic_results_to_reading_scope, live_pause_out_of_scope_bible_book,
    should_release_stale_reading_scope, should_restart_reading, spoken_book_hint,
    strip_reference_scaffolding, strong_out_of_scope_bible_book, DirectReadingCandidate,
    READING_SCOPE_RELEASE_STREAK,
};
use super::utils::{transcript_logging_enabled, truncate_safe};
fn active_reading_bible_scope(app: &AppHandle) -> Option<(i32, i32, String, u64)> {
    let reading_mode_state: State<'_, Mutex<ReadingMode>> = app.state();
    let Ok(reading_mode) = reading_mode_state.lock() else {
        log::warn!("[DET-SEMANTIC] ReadingMode busy; semantic scope filter skipped");
        return None;
    };

    if !reading_mode.is_active() {
        return None;
    }

    let book_number = reading_mode.current_book();
    let chapter = reading_mode.current_chapter();
    if book_number <= 0 || chapter <= 0 {
        return None;
    }

    Some((
        book_number,
        chapter,
        reading_mode.current_book_name().to_string(),
        reading_mode.seconds_since_last_match(),
    ))
}

fn pause_stale_reading_scope(app: &AppHandle) {
    let reading_mode_state: State<'_, Mutex<ReadingMode>> = app.state();
    if let Ok(mut reading_mode) = reading_mode_state.lock() {
        reading_mode.pause();
    };
}

fn note_out_of_scope_hit(app: &AppHandle, book_number: i32, chapter: i32) -> u32 {
    let reading_mode_state: State<'_, Mutex<ReadingMode>> = app.state();
    let streak = match reading_mode_state.lock() {
        Ok(mut reading_mode) => reading_mode.note_out_of_scope_hit(book_number, chapter),
        Err(_) => 0,
    };
    streak
}

fn filter_live_semantic_results_to_reading_scope(
    app: &AppHandle,
    results: Vec<crate::commands::detection::DetectionResult>,
    semantic_min_confidence: f64,
) -> Vec<crate::commands::detection::DetectionResult> {
    let Some((book_number, chapter, book_name, stale_secs)) = active_reading_bible_scope(app)
    else {
        return results;
    };

    if should_release_stale_reading_scope(
        &results,
        book_number,
        chapter,
        stale_secs,
        semantic_min_confidence,
    ) {
        log::info!(
            "[DET-SEMANTIC] Releasing stale reading scope {book_name} {chapter} \
             ({stale_secs}s since last verse match; out-of-scope semantic hit)"
        );
        pause_stale_reading_scope(app);
        return results;
    }

    // Live speech often pivots passages after a phrase-length pause. Once the
    // active chapter has been quiet for that short pause, require two repeated
    // operator-threshold hits on the same out-of-scope passage before releasing.
    if let Some((hit_book, hit_chapter)) = live_pause_out_of_scope_bible_book(
        &results,
        book_number,
        chapter,
        stale_secs,
        semantic_min_confidence,
    ) {
        let streak = note_out_of_scope_hit(app, hit_book, hit_chapter);
        if streak >= READING_SCOPE_RELEASE_STREAK {
            log::info!(
                "[DET-SEMANTIC] Releasing reading scope {book_name} {chapter} \
                 ({stale_secs}s since last verse match; {streak} repeated out-of-scope hits on {hit_book}:{hit_chapter})"
            );
            pause_stale_reading_scope(app);
            return results;
        }
    } else if let Some((hit_book, hit_chapter)) =
        strong_out_of_scope_bible_book(&results, book_number, chapter)
    {
        // Faster path than the staleness clock: several consecutive strong hits
        // on the same out-of-scope passage mean the speaker has moved on. Any
        // in-scope verse match resets the streak, so echoes during real reading
        // still get suppressed.
        let streak = note_out_of_scope_hit(app, hit_book, hit_chapter);
        if streak >= READING_SCOPE_RELEASE_STREAK {
            log::info!(
                "[DET-SEMANTIC] Releasing reading scope {book_name} {chapter} \
                 ({streak} consecutive strong hits on {hit_book}:{hit_chapter})"
            );
            pause_stale_reading_scope(app);
            return results;
        }
    }

    let before = results.len();
    let results = filter_semantic_results_to_reading_scope(results, Some((book_number, chapter)));
    let suppressed = before.saturating_sub(results.len());
    if suppressed > 0 {
        log::info!(
            "[DET-SEMANTIC] Suppressed {suppressed} out-of-scope Bible result(s) while reading {book_name} {chapter}"
        );
    }

    results
}

fn filter_live_direct_results_to_reading_scope(
    app: &AppHandle,
    results: Vec<crate::commands::detection::DetectionResult>,
) -> Vec<crate::commands::detection::DetectionResult> {
    let Some((book_number, chapter, book_name, _)) = active_reading_bible_scope(app) else {
        return results;
    };

    let before = results.len();
    let results = filter_direct_results_to_scope_if_present(results, Some((book_number, chapter)));
    let suppressed = before.saturating_sub(results.len());
    if suppressed > 0 {
        log::info!(
            "[DET-DIRECT] Suppressed {suppressed} out-of-scope Bible result(s) while reading {book_name} {chapter}"
        );
    }

    results
}

fn mark_egw_auto_queue(
    app: &AppHandle,
    results: &mut [crate::commands::detection::DetectionResult],
) {
    let merger_state: State<'_, Mutex<DetectionMerger>> = app.state();
    let Ok(mut merger) = merger_state.lock() else {
        for result in results {
            result.auto_queued = false;
        }
        log::warn!("[DET-EGW] DetectionMerger busy; EGW auto-queue skipped");
        return;
    };
    crate::commands::detection::apply_egw_auto_queue(results, &mut merger);
}

fn emit_egw_direct_detections(
    app: &AppHandle,
    seq: u64,
    latest_seq: &Arc<AtomicU64>,
    transcript: &str,
) {
    let app_managed: State<'_, Mutex<AppState>> = app.state();
    let Ok(app_state) = app_managed.lock() else {
        if transcript_logging_enabled() {
            log::debug!("[DET-EGW] AppState busy; skipping direct EGW detection");
        }
        return;
    };
    let mut results = crate::commands::detection::detect_egw_references(&app_state, transcript);
    drop(app_state);

    if results.is_empty() || seq < latest_seq.load(Ordering::Acquire) {
        return;
    }

    mark_egw_auto_queue(app, &mut results);
    for result in &results {
        log::info!(
            "[DET-EGW] Found: {} ({:.0}%) auto_q={}",
            result.verse_ref,
            result.confidence * 100.0,
            result.auto_queued
        );
    }
    let _ = app.emit("verse_detections", &results);
}

fn detect_live_egw_quotes(
    app: &AppHandle,
    egw_cue_at_ms: &AtomicU64,
    transcript: &str,
    stt_confidence: f64,
) -> Vec<crate::commands::detection::DetectionResult> {
    let app_managed: State<'_, Mutex<AppState>> = app.state();
    let mut results = if let Ok(app_state) = app_managed.lock() {
        let books = app_state
            .bible_db
            .as_ref()
            .and_then(|db| db.list_egw_books().ok())
            .unwrap_or_default();
        if books.is_empty() {
            Vec::new()
        } else {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| {
                    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
                });
            let cue_active = crate::commands::detection::note_and_check_egw_cue(
                &books,
                transcript,
                now_ms,
                egw_cue_at_ms,
            );
            crate::commands::detection::detect_egw_quotes(&app_state, transcript, cue_active)
        }
    } else {
        log::warn!("[DET-EGW-QUOTE] AppState busy; skipping EGW quote pass");
        Vec::new()
    };

    crate::commands::detection::dampen_egw_for_low_stt_confidence(&mut results, stt_confidence);
    mark_egw_auto_queue(app, &mut results);
    results
}

fn retain_results_allowed_by_bible_mode(
    results: &mut Vec<crate::commands::detection::DetectionResult>,
    bible_detection_enabled: bool,
) {
    if !bible_detection_enabled {
        results.retain(|result| result.content_type != "bible");
    }
}

/// Run direct (regex/pattern) detection only. Instant, no ONNX.
/// Uses SEPARATE Mutex<DirectDetector> and Mutex<DetectionMerger> so it
/// never blocks on the semantic worker, and cooldown state persists across calls.
/// Returns direct references that are strong enough to hand reading mode to.
#[expect(
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "direct detection orchestration is intentionally kept together"
)]
pub(crate) fn run_direct_detection(
    app: &AppHandle,
    seq: u64,
    latest_seq: &Arc<AtomicU64>,
    transcript: &str,
) -> Vec<DirectReadingCandidate> {
    // [DIAG] AppState mutex contention on the direct-detection hot path.
    static LOCK_OK: AtomicU64 = AtomicU64::new(0);
    static LOCK_CONTENDED: AtomicU64 = AtomicU64::new(0);

    // Stale detection suppression: if this job's sequence is older than the
    // latest accepted transcript sequence, skip emission.
    if seq < latest_seq.load(Ordering::Acquire) {
        log::debug!("[DET-DIRECT] Skipping stale job seq={seq}");
        return Vec::new();
    }
    if !is_bible_detection_enabled(app) {
        emit_egw_direct_detections(app, seq, latest_seq, transcript);
        return Vec::new();
    }
    let t0 = std::time::Instant::now();
    let detector_state: State<'_, Mutex<DirectDetector>> = app.state();
    let mut detector = match detector_state.lock() {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to lock DirectDetector: {e}");
            return Vec::new();
        }
    };
    let direct_results = detector.detect(transcript);
    drop(detector); // Release immediately

    if !is_bible_detection_enabled(app) {
        emit_egw_direct_detections(app, seq, latest_seq, transcript);
        return Vec::new();
    }

    if direct_results.is_empty() {
        emit_egw_direct_detections(app, seq, latest_seq, transcript);
        return Vec::new();
    }

    // Merge using the managed merger (persists cooldown state across calls,
    // preventing duplicate emissions when running on both partials and finals)
    let merger_state: State<'_, Mutex<DetectionMerger>> = app.state();
    let mut merger = match merger_state.lock() {
        Ok(m) => m,
        Err(e) => {
            log::error!("Failed to lock DetectionMerger: {e}");
            return Vec::new();
        }
    };
    let merged = merger.merge(direct_results, vec![]);
    drop(merger);
    let mut reading_candidates = direct_reading_candidates(&merged);
    if merged.is_empty() {
        emit_egw_direct_detections(app, seq, latest_seq, transcript);
        return reading_candidates;
    }

    // Resolve verse info from DB (needs AppState, but only briefly for DB lookup)
    let app_managed: State<'_, Mutex<AppState>> = app.state();
    let Ok(app_state) = app_managed.lock() else {
        let bad = LOCK_CONTENDED.fetch_add(1, Ordering::Relaxed) + 1;
        let good = LOCK_OK.load(Ordering::Relaxed);
        log::warn!("[DET-DIRECT] AppState lock FAILED (contention) ok={good} contended={bad}");

        // Check for stale sequence BEFORE emitting in fallback path
        if seq < latest_seq.load(Ordering::Acquire) {
            log::debug!("[DET-DIRECT] Skipping stale emission in fallback path seq={seq}");
            return Vec::new();
        }
        if !is_bible_detection_enabled(app) {
            return Vec::new();
        }

        // AppState is locked, so emit results without verse text.
        let results: Vec<crate::commands::detection::DetectionResult> = merged
            .iter()
            .map(|m| {
                let vr = &m.detection.verse_ref;
                crate::commands::detection::DetectionResult {
                    content_type: "bible".to_string(),
                    verse_ref: format!("{} {}:{}", vr.book_name, vr.chapter, vr.verse_start),
                    verse_text: String::new(),
                    book_name: vr.book_name.clone(),
                    book_number: vr.book_number,
                    chapter: vr.chapter,
                    verse: vr.verse_start,
                    confidence: m.detection.confidence,
                    rank_score: m.detection.rank_score(),
                    source: "direct".to_string(),
                    auto_queued: m.auto_queued,
                    transcript_snippet: m.detection.transcript_snippet.clone(),
                    is_chapter_only: m.detection.is_chapter_only,
                    egw_paragraph: None,
                    match_char_start: None,
                }
            })
            .collect();
        let results = filter_live_direct_results_to_reading_scope(app, results);
        for r in &results {
            log::info!(
                "[DET-DIRECT] Found: {} ({:.0}%) (no DB)",
                r.verse_ref,
                r.confidence * 100.0
            );
        }
        let _ = app.emit("verse_detections", &results);
        return reading_candidates;
    };
    let ok = LOCK_OK.fetch_add(1, Ordering::Relaxed) + 1;
    if ok % 50 == 0 {
        let bad = LOCK_CONTENDED.load(Ordering::Relaxed);
        log::info!("[DET-DIRECT] AppState lock stats ok={ok} contended={bad}");
    }
    let mut results: Vec<crate::commands::detection::DetectionResult> = merged
        .iter()
        .map(|m| crate::commands::detection::to_result(&app_state, m))
        .collect();
    let egw_start = results.len();
    results.extend(crate::commands::detection::detect_egw_references(
        &app_state, transcript,
    ));
    drop(app_state);
    if results.len() > egw_start {
        mark_egw_auto_queue(app, &mut results[egw_start..]);
    }
    if !is_bible_detection_enabled(app) {
        retain_results_allowed_by_bible_mode(&mut results, false);
        reading_candidates.clear();
    }
    let mut results = filter_live_direct_results_to_reading_scope(app, results);

    for r in &results {
        log::info!(
            "[DET-DIRECT] Found: {} ({:.0}%)",
            r.verse_ref,
            r.confidence * 100.0
        );
    }

    // Final stale check before emission
    if seq < latest_seq.load(Ordering::Acquire) {
        log::debug!("[DET-DIRECT] Skipping emission for stale seq={seq}");
        return Vec::new();
    }

    log::info!(
        "[DET-TRACE] seq={seq} decision=direct emitted={} top={} took={:?}",
        results.len(),
        results.first().map_or("-", |r| r.verse_ref.as_str()),
        t0.elapsed()
    );
    if !is_bible_detection_enabled(app) {
        retain_results_allowed_by_bible_mode(&mut results, false);
        reading_candidates.clear();
    }
    if results.is_empty() {
        return reading_candidates;
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
    reading_candidates
}

/// Run hybrid semantic detection combining FTS5 BM25 with vector search.
/// Uses `spawn_blocking` so mutex locks and DB I/O don't starve the tokio runtime.
#[expect(
    clippy::too_many_lines,
    reason = "live semantic detection coordinates stale checks, explicit EGW routing, and emission in one pipeline"
)]
pub(crate) fn run_semantic_detection(
    app: &AppHandle,
    seq: u64,
    latest_seq: &Arc<AtomicU64>,
    egw_cue_at_ms: &AtomicU64,
    transcript: &str,
    egw_transcript: &str,
    stt_confidence: f64,
) {
    if !is_semantic_detection_enabled(app) {
        log::debug!("[DET-SEMANTIC] Skipping job seq={seq}; semantic detection disabled");
        return;
    }

    // Stale detection suppression: if this job's sequence is older than the
    // latest accepted transcript sequence, skip emission.
    if seq < latest_seq.load(Ordering::Acquire) {
        log::debug!("[DET-SEMANTIC] Skipping stale job seq={seq}");
        return;
    }

    // Reference and command windows never reach this worker — they are filtered
    // at enqueue (see `enqueue_*_semantic_job`). Remaining windows are EGW
    // references or sermon prose.
    //
    // Catch Ellen White references that endpointing fragmented across several
    // finals: the single-final direct pass misses them, but the rolling window
    // still holds the whole "Book page N paragraph M". Emit the explicit
    // paragraph and skip fuzzy search.
    let mut egw_explicit = {
        let app_managed: State<'_, Mutex<AppState>> = app.state();
        let Ok(app_state) = app_managed.lock() else {
            log::error!("[DET-SEMANTIC] AppState lock failed for EGW window catch");
            return;
        };
        crate::commands::detection::detect_egw_references(&app_state, transcript)
    };
    if !egw_explicit.is_empty() {
        if seq < latest_seq.load(Ordering::Acquire) {
            return;
        }
        mark_egw_auto_queue(app, &mut egw_explicit);
        for r in &egw_explicit {
            log::info!(
                "[DET-TRACE] seq={seq} decision=egw_explicit reason=window_reference {} ({:.0}%) auto_q={}",
                r.verse_ref,
                r.confidence * 100.0,
                r.auto_queued
            );
        }
        let _ = app.emit("verse_detections", &egw_explicit);
        return;
    }

    if !is_bible_detection_enabled(app) {
        let results = detect_live_egw_quotes(app, egw_cue_at_ms, egw_transcript, stt_confidence);
        if results.is_empty() || seq < latest_seq.load(Ordering::Acquire) {
            return;
        }
        let _ = app.emit("verse_detections", &results);
        log::info!(
            "[DET-TRACE] seq={seq} decision=bible_mode_off emitted_egw={}",
            results.len()
        );
        return;
    }

    // Build the paraphrase query from verse content only — reference framing
    // ("chapter 7 verse 9 it says") would otherwise dominate BM25 and the
    // embedding. A window that is nothing but scaffolding is a bare reference
    // already owned by the direct path, so there is nothing to search.
    let query = strip_reference_scaffolding(transcript);
    if query.split_whitespace().count() < FINAL_SEMANTIC_MIN_WORDS {
        log::debug!("[DET-TRACE] seq={seq} skip=semantic reason=scaffolding_only");
        return;
    }

    // A spoken book name is a scope, not a search term. Left in the text it
    // matches only verses that literally contain the name — which is why
    // saying "Malachi" surfaced Malachi 1:1, the one verse whose text
    // contains that word, instead of scoping to the book. Derive from the
    // raw transcript: scaffolding strip removes reference framing only.
    let book_hint = spoken_book_hint(transcript);

    let t0 = std::time::Instant::now();
    if transcript_logging_enabled() {
        log::info!("[DET-SEMANTIC] Running on: {:?}", truncate_safe(&query, 80));
    } else {
        log::info!("[DET-SEMANTIC] Running");
    }

    // FTS5 BM25 phrase search (~5ms)
    let (fts_results, active_translation_id) = {
        let managed: State<'_, Mutex<AppState>> = app.state();
        let Ok(app_state) = managed.lock() else {
            log::error!("Failed to lock AppState for FTS5");
            return;
        };
        (
            app_state
                .bible_db
                .as_ref()
                .and_then(|db| db.search_verses_bm25_scoped(&query, 10, book_hint).ok()),
            app_state.active_translation_id,
        )
    };

    let fts = fts_results.unwrap_or_default();
    if fts.is_empty() {
        log::debug!("[DET-SEMANTIC] No FTS5 results, trying vector-only search");
    } else if let Some(top) = fts.first() {
        log::debug!(
            "[DET-SEMANTIC] FTS5 hits={} top={} {}:{} rank={:.3}",
            fts.len(),
            top.book_name,
            top.chapter,
            top.verse,
            top.rank
        );
    }

    // Use hybrid pipeline: FTS5 + vector search when available.
    // Even with empty FTS5, vector search can catch paraphrases.
    let (merged, semantic_ready, paraphrase_enabled, semantic_min_confidence) = {
        let pipeline_state: State<'_, Mutex<rhema_detection::DetectionPipeline>> = app.state();
        let Ok(mut pipeline) = pipeline_state.lock() else {
            log::error!("Failed to lock DetectionPipeline");
            return;
        };
        let semantic_ready = pipeline.has_semantic();
        let paraphrase_enabled = pipeline.use_synonyms();
        let semantic_min_confidence = pipeline.semantic_confidence_threshold();
        let merged = pipeline.process_hybrid_with_fts(&query, &fts);
        (
            merged,
            semantic_ready,
            paraphrase_enabled,
            semantic_min_confidence,
        )
    };

    log::info!(
        "[DET-SEMANTIC] Workflow seq={} words={} fts_hits={} vector_ready={} paraphrase={} active_translation_id={} candidates={} elapsed={:?}",
        seq,
        transcript.split_whitespace().count(),
        fts.len(),
        semantic_ready,
        paraphrase_enabled,
        active_translation_id,
        merged.len(),
        t0.elapsed()
    );

    // Resolve verse text from DB for merged results. Explicit EGW references
    // are handled above. EGW quote matches are appended below: BM25 nominates,
    // but a candidate only survives if a long run of its words was actually
    // spoken. Flat-confidence BM25 hits are what made this noisy before.
    let app_managed: State<'_, Mutex<AppState>> = app.state();
    let Ok(app_state) = app_managed.lock() else {
        log::error!("Failed to lock AppState for verse resolution");
        return;
    };

    let results: Vec<crate::commands::detection::DetectionResult> = merged
        .iter()
        .map(|m| crate::commands::detection::to_result(&app_state, m))
        .collect();

    drop(app_state);
    let results =
        filter_live_semantic_results_to_reading_scope(app, results, semantic_min_confidence);
    let mut results = finalize_live_semantic_results(results, semantic_min_confidence);
    if stt_confidence > 0.0 && stt_confidence < 0.65 {
        for result in &mut results {
            result.rank_score *= 0.85;
            result.confidence = result.confidence.min(0.89);
        }
    }

    let mut egw_quotes =
        detect_live_egw_quotes(app, egw_cue_at_ms, egw_transcript, stt_confidence);
    crate::commands::detection::drop_egw_quotes_echoing_scripture(
        &mut egw_quotes,
        &results,
        egw_transcript,
    );
    results.extend(egw_quotes);

    if !is_bible_detection_enabled(app) {
        retain_results_allowed_by_bible_mode(&mut results, false);
    }

    if results.is_empty() {
        log::info!(
            "[DET-TRACE] seq={seq} decision=semantic_none emitted=0 fts_hits={} candidates={}",
            fts.len(),
            merged.len()
        );
        return;
    }

    // Final stale check before emission
    if seq < latest_seq.load(Ordering::Acquire) {
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
    if !is_bible_detection_enabled(app) {
        retain_results_allowed_by_bible_mode(&mut results, false);
    }
    if results.is_empty() {
        return;
    }
    let _ = app.emit("verse_detections", &results);
    log::info!(
        "[DET-TRACE] seq={seq} decision=semantic_fuzzy emitted={} top={} ({:.0}%)",
        results.len(),
        results.first().map_or("-", |r| r.verse_ref.as_str()),
        results.first().map_or(0.0, |r| r.confidence) * 100.0
    );
    log::info!("[DET-SEMANTIC] Total: {:?}", t0.elapsed());
}

/// Check reading mode: if active, test transcript against expected verse.
/// If direct detection just found a new verse, start/restart reading mode.
/// Returns `true` when reading mode handled the transcript (suppresses semantic).
#[expect(
    clippy::too_many_lines,
    reason = "sequential state-machine logic is clearer in one flow"
)]
pub(crate) fn check_reading_mode(
    app: &AppHandle,
    transcript: &str,
    direct_candidates: Vec<DirectReadingCandidate>,
) -> bool {
    use rhema_detection::ReadingMode;

    // If direct detection found a verse, consider starting/restarting reading mode.
    // BUT: if reading mode is already active on a book/chapter, do NOT restart
    // on a different book — false positives from bare numbers (e.g., "verse 5"
    // getting matched as "Job 3:5") would hijack the reading session.
    if !direct_candidates.is_empty() {
        let active_scope = {
            let rm_managed: &Mutex<ReadingMode> = app.state::<Mutex<ReadingMode>>().inner();
            rm_managed.lock().ok().and_then(|rm| {
                if rm.is_active() || rm.has_verses() {
                    Some((rm.current_book(), rm.current_chapter()))
                } else {
                    None
                }
            })
        };

        if let Some(candidate) = choose_reading_candidate(&direct_candidates, active_scope) {
            let recent = candidate.verse_ref.clone();

            let should_start = {
                let rm_managed: &Mutex<ReadingMode> = app.state::<Mutex<ReadingMode>>().inner();
                match rm_managed.lock() {
                    Ok(rm) => should_restart_reading(
                        rm.is_active(),
                        rm.current_book(),
                        rm.current_chapter(),
                        rm.current_verse(),
                        &candidate,
                    ),
                    Err(_) => false,
                }
            };

            if should_start {
                let chapter_data = {
                    let t_db = std::time::Instant::now();
                    let app_managed: State<'_, Mutex<AppState>> = app.state();
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
                            && !lower.contains("verse")
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
                let app_managed: State<'_, Mutex<AppState>> = app.state();
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

                    if !change.emit_start_verse {
                        log::info!(
                            "[READING] Chapter context moved to {} {}; waiting for verse before UI emit",
                            change.book_name,
                            change.new_chapter
                        );
                        return true;
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

#[cfg(test)]
mod bible_mode_tests {
    use super::retain_results_allowed_by_bible_mode;
    use crate::commands::detection::DetectionResult;

    fn result(content_type: &str) -> DetectionResult {
        DetectionResult {
            content_type: content_type.to_string(),
            verse_ref: "reference".to_string(),
            verse_text: "text".to_string(),
            book_name: "book".to_string(),
            book_number: 1,
            chapter: 1,
            verse: 1,
            confidence: 1.0,
            rank_score: 1.0,
            source: "direct".to_string(),
            auto_queued: false,
            transcript_snippet: "spoken words".to_string(),
            is_chapter_only: false,
            egw_paragraph: None,
            match_char_start: None,
        }
    }

    #[test]
    fn bible_mode_off_filters_bible_but_preserves_egw_results() {
        let mut results = vec![result("bible"), result("egw")];

        retain_results_allowed_by_bible_mode(&mut results, false);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content_type, "egw");
    }

    #[test]
    fn bible_mode_on_preserves_all_detection_results() {
        let mut results = vec![result("bible"), result("egw")];

        retain_results_allowed_by_bible_mode(&mut results, true);

        assert_eq!(results.len(), 2);
    }
}
