use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rhema_bible::EgwBook;
use tauri::{AppHandle, Manager, State};

use crate::state::AppState;

pub(crate) fn load_egw_cue_books(state: &State<'_, Mutex<AppState>>) -> Vec<EgwBook> {
    state
        .lock()
        .ok()
        .and_then(|app_state| {
            app_state
                .bible_db
                .as_ref()
                .and_then(|db| db.list_egw_books().ok())
        })
        .unwrap_or_default()
}

pub(crate) fn record_egw_cue(books: &[EgwBook], text: &str, cue_at_ms: &AtomicU64) {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        });
    crate::commands::detection::note_and_check_egw_cue(books, text, now_ms, cue_at_ms);
}

/// Check whether the operator has paused detection suggestions.
/// Uses a blocking lock so the pause flag is authoritative.
/// The lock is held only for an atomic load, so transcript events are not blocked.
pub(crate) fn is_detection_paused(app: &AppHandle) -> bool {
    let state: State<'_, Mutex<AppState>> = app.state();
    let paused = match state.lock() {
        Ok(s) => s.detection_paused.load(Ordering::Relaxed),
        Err(_) => true,
    };
    paused
}

pub(crate) fn is_bible_detection_enabled(app: &AppHandle) -> bool {
    let state: State<'_, Mutex<AppState>> = app.state();
    let enabled = match state.lock() {
        Ok(state) => state.bible_detection_enabled.load(Ordering::Relaxed),
        Err(_) => false,
    };
    enabled
}

pub(crate) fn is_semantic_detection_enabled(app: &AppHandle) -> bool {
    let state: State<'_, Mutex<AppState>> = app.state();
    let enabled = match state.lock() {
        Ok(s) => s.semantic_detection_enabled.load(Ordering::Relaxed),
        Err(_) => false,
    };
    enabled
}

pub(crate) const SEMANTIC_WINDOW_SEGMENTS: usize = 4;
pub(crate) const FINAL_SEMANTIC_MIN_WORDS: usize = 3;
pub(crate) const PARTIAL_SEMANTIC_DEBOUNCE: Duration = Duration::from_millis(100);
pub(crate) const PARTIAL_SEMANTIC_MIN_WORDS: usize = 3;
pub(crate) const LIVE_SEMANTIC_CAP: usize = 3;
pub(crate) const LIVE_SEMANTIC_OVERLAP_BOOST: f64 = 0.10;
/// Default minimum confidence for live semantic/FTS detections.
/// The active value is synced from the app settings; tests use this default.
#[cfg(test)]
pub(crate) const LIVE_SEMANTIC_MIN_CONFIDENCE: f64 = 0.70;

/// Maximum trailing words of the rolling transcript window fed to live
/// semantic + FTS5 detection.
pub(crate) const LIVE_DETECTION_WINDOW_WORDS: usize = 12;

/// How long an identical direct reference stays suppressed after being emitted.
///
/// Every dispatched partial re-runs direct detection, so one slowly-spoken
/// "John 3 verse 16" emits `John 3:1` once per partial while the verse number
/// is still arriving — 12 times in 2s on 2026-08-04. The frontend store keys
/// detections by reference so these collapse to one row, but each re-emission
/// refreshes its `received_at`, keeping a superseded reference as "recent" as
/// the verse the speaker actually reached.
pub(crate) const DIRECT_REPEAT_SUPPRESSION: Duration = Duration::from_secs(3);

/// Suppress re-emission of references already sent very recently.
///
/// Keyed on the resolved reference rather than the transcript, so a refined
/// reference (`John 3:16` after `John 3:1`) is always a distinct key and is
/// never suppressed by its own prefix.
///
/// `is_chapter_only` is part of the key: a chapter-only placeholder that
/// defaults to verse 1 (`Matthew 1:1` at 92%) must not suppress the later full
/// citation of the same verse (`Matthew 1:1` at 100%). Frontend preview and
/// auto-live ignore chapter-only hits, so suppressing the upgrade left verse 1
/// citations with no preview/live path.
#[derive(Default)]
pub(crate) struct RecentDirectEmissions {
    seen: std::collections::HashMap<DirectEmissionKey, DirectEmissionState>,
}

type DirectEmissionKey = (String, i32, i32, i32, bool);
type DirectEmissionState = (std::time::Instant, bool);

impl RecentDirectEmissions {
    pub(crate) fn suppress_repeats(
        &mut self,
        results: &mut Vec<crate::commands::detection::DetectionResult>,
        window: Duration,
        now: std::time::Instant,
    ) {
        self.suppress_repeats_with_finality(results, window, now, false);
    }

    /// A final transcript may be the first complete form of a single-digit
    /// citation. Let that refinement through even when the same reference was
    /// emitted provisionally from a partial transcript.
    pub(crate) fn suppress_repeats_final(
        &mut self,
        results: &mut Vec<crate::commands::detection::DetectionResult>,
        window: Duration,
        now: std::time::Instant,
    ) {
        self.suppress_repeats_with_finality(results, window, now, true);
    }

    fn suppress_repeats_with_finality(
        &mut self,
        results: &mut Vec<crate::commands::detection::DetectionResult>,
        window: Duration,
        now: std::time::Instant,
        allow_final_single_digit: bool,
    ) {
        // Bound growth on a long service without needing a separate sweep.
        self.seen
            .retain(|_, (seen_at, _)| now.duration_since(*seen_at) < window.saturating_mul(4));
        results.retain(|result| {
            let key = (
                result.content_type.clone(),
                result.book_number,
                result.chapter,
                result.verse,
                result.is_chapter_only,
            );
            let is_final_single_digit = allow_final_single_digit
                && result.content_type == "bible"
                && (1..=9).contains(&result.verse)
                && !result.is_chapter_only;
            match self.seen.get(&key) {
                Some((seen_at, seen_final))
                    if now.duration_since(*seen_at) < window
                        && (!is_final_single_digit || *seen_final) =>
                {
                    log::debug!(
                        "[DET-DIRECT] Suppressed repeat {} within {}ms",
                        result.verse_ref,
                        window.as_millis()
                    );
                    false
                }
                _ => {
                    self.seen.insert(key, (now, is_final_single_digit));
                    true
                }
            }
        });
    }
}

/// Maximum trailing words of the rolling window fed to live EGW quote matching.
///
/// Deliberately wider than `LIVE_DETECTION_WINDOW_WORDS`: Bible verses are
/// short and a tight window keeps vector search off adjacent sentences, but an
/// EGW paragraph sentence runs 25-40 words. At 12 words the shared run with the
/// spoken quote tops out around 5, which scores in the cued-hint band (75-80%)
/// and never reaches the fire (6) or auto-queue (8) tiers. On 2026-08-04 a
/// verbatim Great Controversy quote scored 75-80% for this reason while
/// keyword-floored Bible hits sat at 88% and buried it.
pub(crate) const LIVE_EGW_QUOTE_WINDOW_WORDS: usize = 40;

/// Clear the rolling detection window after this much silence between finals.
pub(crate) const WINDOW_RESET_GAP: Duration = Duration::from_secs(8);

/// Move auto-queue off provisional single-digit citations onto the digit-stable
/// one in the same batch.
///
/// STT partials emit `Matthew 6:3` before `6:33` finishes arriving, so a
/// single-digit full citation must never auto-fire. If stripping it left the
/// batch with no auto-queue at all, hand the flag to the strongest multi-digit
/// citation — live 2026-08-04 left `John 3:16` at `auto_q=false` after `3:1`
/// consumed the merger's single slot. The re-award still has to clear the
/// operator's threshold, so a hit the merger would have refused cannot inherit
/// it (Manual mode sets the threshold to infinity and blocks this entirely).
pub(crate) fn rebalance_auto_queue_for_digit_growth(
    results: &mut [crate::commands::detection::DetectionResult],
    auto_queue_threshold: f64,
    is_final_transcript: bool,
) {
    if is_final_transcript {
        let non_single_digit_already_queued = results.iter().any(|result| {
            result.auto_queued
                && !(result.content_type == "bible"
                    && (1..=9).contains(&result.verse)
                    && !result.is_chapter_only)
        });
        if non_single_digit_already_queued {
            for result in results.iter_mut().filter(|result| {
                result.content_type == "bible"
                    && (1..=9).contains(&result.verse)
                    && !result.is_chapter_only
            }) {
                result.auto_queued = false;
            }
            return;
        }

        for result in results.iter_mut().filter(|result| {
            result.content_type == "bible"
                && (1..=9).contains(&result.verse)
                && !result.is_chapter_only
        }) {
            result.auto_queued = false;
        }

        if let Some(best) = results
            .iter_mut()
            .filter(|result| {
                result.content_type == "bible"
                    && result.source == "direct"
                    && !result.is_chapter_only
                    && (1..=9).contains(&result.verse)
                    && result.confidence >= auto_queue_threshold
            })
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.verse.cmp(&b.verse))
            })
        {
            best.auto_queued = true;
        }
        return;
    }

    let mut stripped_auto = false;
    for result in results.iter_mut() {
        if result.content_type == "bible"
            && !result.is_chapter_only
            && (1..=9).contains(&result.verse)
            && result.auto_queued
        {
            result.auto_queued = false;
            stripped_auto = true;
        }
    }

    if !stripped_auto || results.iter().any(|result| result.auto_queued) {
        return;
    }

    if let Some(best) = results
        .iter_mut()
        .filter(|result| {
            result.content_type == "bible"
                && !result.is_chapter_only
                && result.verse >= 10
                && result.source == "direct"
                && result.confidence >= auto_queue_threshold
        })
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.verse.cmp(&b.verse))
        })
    {
        best.auto_queued = true;
    }
}

/// Extend EGW attribution while a reading block keeps matching.
///
/// Call this only with quotes that survived dampening and best-quote selection.
/// Refreshing on a raw BM25 match let a quote that was then discarded hold
/// Bible semantic detection off for the rest of the cue TTL.
pub(crate) fn refresh_egw_cue_for_surviving_quote(
    egw_cue_at_ms: &AtomicU64,
    now_ms: u64,
    cue_active: bool,
    surviving: &[crate::commands::detection::DetectionResult],
) {
    if cue_active && !surviving.is_empty() {
        egw_cue_at_ms.store(now_ms, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RecentDirectEmissions, DIRECT_REPEAT_SUPPRESSION, LIVE_DETECTION_WINDOW_WORDS,
        LIVE_EGW_QUOTE_WINDOW_WORDS, LIVE_SEMANTIC_CAP, LIVE_SEMANTIC_MIN_CONFIDENCE,
        PARTIAL_SEMANTIC_DEBOUNCE, PARTIAL_SEMANTIC_MIN_WORDS, SEMANTIC_WINDOW_SEGMENTS,
    };
    use crate::commands::stt::detection_jobs::{
        enqueue_final_semantic_job, enqueue_partial_semantic_job, finalize_live_semantic_results,
        replace_semantic_job, take_semantic_job, DeepgramSemanticBuffer, SemanticJob,
    };
    use crate::commands::stt::detection_logic;
    use crate::commands::stt::detection_logic::{
        choose_reading_candidate, clamp_to_recent_words, direct_reading_candidates,
        should_restart_reading, spoken_book_hint, strip_reference_scaffolding,
    };
    use rhema_detection::{Detection, DetectionSource, MergedDetection, VerseRef};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::Notify;

    #[test]
    fn strip_reference_scaffolding_drops_framing_keeps_verse_content() {
        // The Daniel window that polluted BM25 with "chapter/verse/says".
        assert_eq!(
            strip_reference_scaffolding(
                "chapter 7 verse 9 it says I watched till thrones were put in place"
            ),
            "I watched till thrones were put in place"
        );
        // Pure reference window collapses to nothing (direct path owns it).
        assert_eq!(strip_reference_scaffolding("chapter 20 verse 12"), "");
        // Verse prose with no framing is untouched, including spelled-out
        // numbers and comma-grouped digits.
        assert_eq!(
            strip_reference_scaffolding("ten thousand times ten thousand stood before him"),
            "ten thousand times ten thousand stood before him"
        );
        assert_eq!(
            strip_reference_scaffolding("the court was seated and the books were opened"),
            "the court was seated and the books were opened"
        );
    }

    #[test]
    fn spoken_book_hint_scopes_single_bare_book() {
        assert_eq!(
            spoken_book_hint("in Esther he says for such a time as this"),
            Some(17)
        );
    }

    #[test]
    fn spoken_book_hint_ignores_multi_book_and_complete_refs() {
        assert_eq!(
            spoken_book_hint("Malachi is speaking to Esther about such a time"),
            None
        );
        assert_eq!(
            spoken_book_hint("Turn with me to Exodus chapter 20 verse 8"),
            None
        );
        assert_eq!(spoken_book_hint("peace be still on the sea"), None);
    }

    #[test]
    fn spoken_book_hint_does_not_scope_a_bible_book_used_as_a_role_name() {
        assert_eq!(
            spoken_book_hint("the verse where John the Baptist baptizes Jesus"),
            None
        );
    }

    #[test]
    fn semantic_enqueue_skips_reference_and_command_windows() {
        let slot = Arc::new(Mutex::new(None));
        let notify = Arc::new(Notify::new());
        let sent = Arc::new(AtomicU64::new(0));
        let replaced = Arc::new(AtomicU64::new(0));

        // Explicit reference - direct path owns it; semantic must not enqueue
        // (so it cannot evict a pending prose job from the latest-wins slot).
        enqueue_final_semantic_job(
            &slot,
            &notify,
            &sent,
            &replaced,
            1,
            "John chapter 8 verse 9".to_string(),
            "John chapter 8 verse 9".to_string(),
            0.9,
        );
        assert!(
            slot.lock().unwrap().is_none(),
            "reference window must not enqueue a semantic job"
        );

        // Voice command - same.
        enqueue_partial_semantic_job(
            &slot,
            &notify,
            &sent,
            &replaced,
            2,
            "let's go to the next verse".to_string(),
            "let's go to the next verse".to_string(),
            0.9,
        );
        assert!(
            slot.lock().unwrap().is_none(),
            "command window must not enqueue a semantic job"
        );

        enqueue_final_semantic_job(
            &slot,
            &notify,
            &sent,
            &replaced,
            3,
            "one".to_string(),
            "one".to_string(),
            0.9,
        );
        assert!(
            slot.lock().unwrap().is_none(),
            "tiny final window must not enqueue a semantic job"
        );

        // Sermon prose - must enqueue so paraphrase detection still runs.
        enqueue_final_semantic_job(
            &slot,
            &notify,
            &sent,
            &replaced,
            4,
            "for God so loved the world that he gave his only begotten son".to_string(),
            "for God so loved the world that he gave his only begotten son".to_string(),
            0.73,
        );
        assert_eq!(
            slot.lock().unwrap().as_ref().map(|job| job.seq),
            Some(4),
            "prose window must enqueue a semantic job"
        );
    }

    fn make_detection_result(
        verse_ref: &str,
        book_number: i32,
        chapter: i32,
        verse: i32,
        confidence: f64,
    ) -> crate::commands::detection::DetectionResult {
        crate::commands::detection::DetectionResult {
            content_type: "bible".to_string(),
            verse_ref: verse_ref.to_string(),
            verse_text: "verse text".to_string(),
            book_name: "Book".to_string(),
            book_number,
            chapter,
            verse,
            confidence,
            rank_score: confidence,
            source: "semantic".to_string(),
            auto_queued: false,
            transcript_snippet: "snippet".to_string(),
            is_chapter_only: false,
            egw_paragraph: None,
            match_char_start: None,
        }
    }

    fn make_merged_direct(
        book_name: &str,
        book_number: i32,
        chapter: i32,
        verse: i32,
        confidence: f64,
        is_chapter_only: bool,
    ) -> MergedDetection {
        MergedDetection {
            detection: Detection {
                verse_ref: VerseRef {
                    book_number,
                    book_name: book_name.to_string(),
                    chapter,
                    verse_start: verse,
                    verse_end: None,
                },
                verse_id: None,
                confidence,
                source: DetectionSource::DirectReference,
                transcript_snippet: "snippet".to_string(),
                detected_at: 0,
                is_chapter_only,
            },
            auto_queued: false,
        }
    }

    #[test]
    fn direct_reading_candidates_include_chapter_only_handoffs_below_ninety_percent() {
        let merged = vec![make_merged_direct("Philippians", 50, 4, 1, 0.88, true)];

        let candidates = direct_reading_candidates(&merged);

        assert_eq!(
            candidates,
            vec![detection_logic::DirectReadingCandidate {
                verse_ref: VerseRef {
                    book_number: 50,
                    book_name: "Philippians".to_string(),
                    chapter: 4,
                    verse_start: 1,
                    verse_end: None,
                },
                confidence: 0.88,
                is_chapter_only: true,
            }]
        );
    }

    fn reading_candidate(
        book_number: i32,
        chapter: i32,
        verse: i32,
        confidence: f64,
        is_chapter_only: bool,
    ) -> detection_logic::DirectReadingCandidate {
        detection_logic::DirectReadingCandidate {
            verse_ref: VerseRef {
                book_number,
                book_name: "Book".to_string(),
                chapter,
                verse_start: verse,
                verse_end: None,
            },
            confidence,
            is_chapter_only,
        }
    }

    #[test]
    fn reanchors_to_specific_verse_within_active_chapter() {
        // Reading Malachi 3 anchored at the chapter-only default (3:1); a later
        // explicit "Malachi 3:16" must re-anchor forward, not be ignored.
        let candidate = reading_candidate(39, 3, 16, 1.0, false);
        assert!(should_restart_reading(true, 39, 3, Some(1), &candidate));
    }

    #[test]
    fn chapter_only_hit_does_not_reanchor_within_active_chapter() {
        // The repeated chapter-only "Malachi 3" (-> 3:1) must never drag the
        // cursor back to verse 1 once we are reading 3:16.
        let candidate = reading_candidate(39, 3, 1, 0.88, true);
        assert!(!should_restart_reading(true, 39, 3, Some(16), &candidate));
    }

    #[test]
    fn same_specific_verse_does_not_restart() {
        let candidate = reading_candidate(39, 3, 16, 1.0, false);
        assert!(!should_restart_reading(true, 39, 3, Some(16), &candidate));
    }

    #[test]
    fn stale_same_chapter_previous_verse_does_not_restart() {
        let candidate = reading_candidate(27, 7, 9, 1.0, false);
        assert!(!should_restart_reading(true, 27, 7, Some(10), &candidate));
    }

    #[test]
    fn inactive_reading_mode_always_restarts_on_reference() {
        let candidate = reading_candidate(39, 3, 16, 1.0, false);
        assert!(should_restart_reading(false, 39, 3, Some(16), &candidate));
        assert!(should_restart_reading(false, 0, 0, None, &candidate));
    }

    #[test]
    fn different_book_restarts_only_when_explicit() {
        let high = reading_candidate(43, 3, 16, 1.0, false);
        assert!(should_restart_reading(true, 39, 3, Some(16), &high));
        let low = reading_candidate(43, 3, 16, 0.70, false);
        assert!(!should_restart_reading(true, 39, 3, Some(16), &low));
    }

    #[test]
    fn single_digit_full_citation_does_not_reanchor_during_digit_growth() {
        // Live: Matthew 6:1 (chapter-only) then provisional Matthew 6:3 before 6:33.
        let provisional = reading_candidate(40, 6, 3, 1.0, false);
        assert!(!should_restart_reading(
            true,
            40,
            6,
            Some(1),
            &provisional
        ));
        let stable = reading_candidate(40, 6, 33, 1.0, false);
        assert!(should_restart_reading(true, 40, 6, Some(1), &stable));
    }

    #[test]
    fn inactive_mode_does_not_start_on_single_digit_full_citation() {
        let provisional = reading_candidate(40, 6, 3, 1.0, false);
        assert!(!should_restart_reading(false, 0, 0, None, &provisional));
        let chapter_only = reading_candidate(40, 6, 1, 0.92, true);
        assert!(should_restart_reading(false, 0, 0, None, &chapter_only));
        let stable = reading_candidate(40, 6, 33, 1.0, false);
        assert!(should_restart_reading(false, 0, 0, None, &stable));
    }

    #[test]
    fn digit_prefix_loser_is_dropped_from_reading_candidates() {
        let merged = vec![
            make_merged_direct("Matthew", 40, 6, 3, 1.0, false),
            make_merged_direct("Matthew", 40, 6, 33, 1.0, false),
        ];
        let candidates = direct_reading_candidates(&merged);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].verse_ref.verse_start, 33);
    }

    #[test]
    fn same_book_new_chapter_restarts() {
        let candidate = reading_candidate(39, 4, 1, 0.88, true);
        assert!(should_restart_reading(true, 39, 3, Some(16), &candidate));
    }

    #[test]
    fn choose_reading_candidate_prefers_active_scope_over_stale_first_candidate() {
        let candidates = direct_reading_candidates(&[
            make_merged_direct("Isaiah", 23, 4, 3, 1.0, false),
            make_merged_direct("Philippians", 50, 4, 3, 1.0, false),
            make_merged_direct("Revelation", 66, 1, 3, 1.0, false),
        ]);

        let selected = choose_reading_candidate(&candidates, Some((50, 4)));

        assert_eq!(
            selected.map(|candidate| candidate.verse_ref.book_name),
            Some("Philippians".to_string())
        );
    }

    #[test]
    fn direct_scope_filter_keeps_active_chapter_when_batch_contains_it() {
        let results = vec![
            make_detection_result("Isaiah 4:3", 23, 4, 3, 1.0),
            make_detection_result("Philippians 4:3", 50, 4, 3, 1.0),
            make_detection_result("Revelation 1:3", 66, 1, 3, 1.0),
        ];

        let filtered =
            detection_logic::filter_direct_results_to_scope_if_present(results, Some((50, 4)));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].verse_ref, "Philippians 4:3");
    }

    #[test]
    fn direct_scope_filter_allows_new_book_when_active_chapter_absent() {
        let results = vec![make_detection_result("Revelation 1:3", 66, 1, 3, 1.0)];

        let filtered =
            detection_logic::filter_direct_results_to_scope_if_present(results, Some((50, 4)));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].verse_ref, "Revelation 1:3");
    }

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

    #[test]
    fn defers_to_direct_for_explicit_references_and_commands() {
        use crate::commands::stt::detection_logic::transcript_defers_to_direct as defers;

        // Explicit scripture references - the direct path is authoritative.
        assert!(defers("John chapter 8 verse 9"));
        assert!(defers("Galatians 1 verse 1"));
        assert!(defers("genesis chapter 3 verse 15"));
        assert!(defers("1 Samuel 1 verse 3"));
        assert!(defers("Revelation 1 verse 1"));
        assert!(defers("Romans 8 verse 5"));
        // Voice/reading commands.
        assert!(defers("Hymn number 46"));
        assert!(defers("Adventist hymnal 100"));
        assert!(defers("Seventh-day Adventist hymnal one hundred"));
        assert!(defers("lied 12"));
        assert!(defers("Sewendedag Adventiste lied nommer een honderd"));
        assert!(defers("I need the new living translation."));
        assert!(defers("King James Version"));
        assert!(defers("let's go to the next verse"));
        assert!(defers("in the same chapter verse 17"));
    }

    #[test]
    fn does_not_defer_for_sermon_prose() {
        use crate::commands::stt::detection_logic::transcript_defers_to_direct as defers;

        // Spoken verse content must stay eligible for semantic paraphrase
        // detection (e.g. this should still surface John 3:16).
        assert!(!defers(
            "For God so loved the world that he gave his only begotten son"
        ));
        assert!(!defers("testing one two testing"));
        assert!(!defers("today we are talking about obedience and grace"));
    }

    #[test]
    fn live_semantic_workflow_matches_requested_speed_and_result_window() {
        assert_eq!(LIVE_SEMANTIC_CAP, 3);
        assert_eq!(SEMANTIC_WINDOW_SEGMENTS, 4);
        assert_eq!(PARTIAL_SEMANTIC_DEBOUNCE, Duration::from_millis(100));
        assert_eq!(PARTIAL_SEMANTIC_MIN_WORDS, 3);
    }

    #[test]
    fn clamp_to_recent_words_keeps_only_trailing_words() {
        assert_eq!(
            clamp_to_recent_words("one two three four five", 3),
            "three four five"
        );
    }

    #[test]
    fn full_egw_attribution_is_not_present_in_trailing_quote_window() {
        let full = "A statement by Illinois in Patriarchs and Prophets says the human race yet retained much of its early vigor but a few generations had passed since Adam had access to the tree";
        let trailing = clamp_to_recent_words(full, 12);

        assert!(
            !trailing.contains("Patriarchs and Prophets"),
            "the live cue must be recorded before window truncation: {trailing}"
        );
    }

    use crate::commands::detection::DetectionResult;

    fn direct_result(
        reference: &str,
        book: i32,
        chapter: i32,
        verse: i32,
        is_chapter_only: bool,
    ) -> DetectionResult {
        DetectionResult {
            content_type: "bible".to_string(),
            verse_ref: reference.to_string(),
            verse_text: String::new(),
            book_name: "John".to_string(),
            book_number: book,
            chapter,
            verse,
            confidence: if is_chapter_only { 0.92 } else { 1.0 },
            rank_score: if is_chapter_only { 0.92 } else { 1.0 },
            source: "direct".to_string(),
            auto_queued: false,
            transcript_snippet: String::new(),
            is_chapter_only,
            egw_paragraph: None,
            match_char_start: None,
        }
    }

    #[test]
    fn repeat_direct_reference_is_suppressed_within_the_window() {
        // 2026-08-04: one slow "John 3 verse 16" emitted John 3:1 twelve times
        // across consecutive partials, refreshing its recency each time.
        let mut recent = RecentDirectEmissions::default();
        let start = std::time::Instant::now();

        let mut first = vec![direct_result("John 3:1", 43, 3, 1, false)];
        recent.suppress_repeats(&mut first, DIRECT_REPEAT_SUPPRESSION, start);
        assert_eq!(first.len(), 1, "the first emission must go through");

        let mut repeat = vec![direct_result("John 3:1", 43, 3, 1, false)];
        recent.suppress_repeats(
            &mut repeat,
            DIRECT_REPEAT_SUPPRESSION,
            start + Duration::from_millis(400),
        );
        assert!(repeat.is_empty(), "an identical repeat must be suppressed");
    }

    #[test]
    fn final_single_digit_reference_can_replace_its_partial_repeat() {
        let mut recent = RecentDirectEmissions::default();
        let start = std::time::Instant::now();

        let mut partial = vec![direct_result("Genesis 2:8", 1, 2, 8, false)];
        recent.suppress_repeats(&mut partial, DIRECT_REPEAT_SUPPRESSION, start);
        assert_eq!(partial.len(), 1);

        let mut final_result = vec![direct_result("Genesis 2:8", 1, 2, 8, false)];
        recent.suppress_repeats_final(
            &mut final_result,
            DIRECT_REPEAT_SUPPRESSION,
            start + Duration::from_millis(100),
        );
        assert_eq!(
            final_result.len(),
            1,
            "the final citation must reach the frontend for auto-live"
        );

        let mut repeated_final = vec![direct_result("Genesis 2:8", 1, 2, 8, false)];
        recent.suppress_repeats_final(
            &mut repeated_final,
            DIRECT_REPEAT_SUPPRESSION,
            start + Duration::from_millis(200),
        );
        assert!(
            repeated_final.is_empty(),
            "only the first final citation may replace the provisional repeat"
        );
    }

    #[test]
    fn refined_reference_is_never_suppressed_by_its_own_prefix() {
        // The whole point of the partial race: John 3:16 arrives after John 3:1
        // and must always reach the operator.
        let mut recent = RecentDirectEmissions::default();
        let start = std::time::Instant::now();

        let mut first = vec![direct_result("John 3:1", 43, 3, 1, true)];
        recent.suppress_repeats(&mut first, DIRECT_REPEAT_SUPPRESSION, start);

        let mut refined = vec![direct_result("John 3:16", 43, 3, 16, false)];
        recent.suppress_repeats(
            &mut refined,
            DIRECT_REPEAT_SUPPRESSION,
            start + Duration::from_millis(200),
        );

        assert_eq!(refined.len(), 1, "John 3:16 must survive after John 3:1");
    }

    #[test]
    fn full_verse_one_is_not_suppressed_by_prior_chapter_only_placeholder() {
        // Live 2026-08-04: "Matthew chapter 1" emitted chapter-only Matthew 1:1
        // @ 92%, then "Matthew chapter 1 verse 1" was suppressed as a repeat of
        // the same book/chapter/verse key — so preview/auto-live never ran
        // (both require !is_chapter_only).
        let mut recent = RecentDirectEmissions::default();
        let start = std::time::Instant::now();

        let mut chapter_only = vec![direct_result("Matthew 1:1", 40, 1, 1, true)];
        recent.suppress_repeats(&mut chapter_only, DIRECT_REPEAT_SUPPRESSION, start);
        assert_eq!(chapter_only.len(), 1);

        let mut full = vec![direct_result("Matthew 1:1", 40, 1, 1, false)];
        recent.suppress_repeats(
            &mut full,
            DIRECT_REPEAT_SUPPRESSION,
            start + Duration::from_millis(200),
        );
        assert_eq!(
            full.len(),
            1,
            "full Matthew 1:1 must upgrade past chapter-only placeholder"
        );
    }

    #[test]
    fn reference_re_emits_once_the_window_has_passed() {
        // A verse genuinely read again later must surface again.
        let mut recent = RecentDirectEmissions::default();
        let start = std::time::Instant::now();

        let mut first = vec![direct_result("Psalms 23:1", 19, 23, 1, false)];
        recent.suppress_repeats(&mut first, DIRECT_REPEAT_SUPPRESSION, start);

        let mut later = vec![direct_result("Psalms 23:1", 19, 23, 1, false)];
        recent.suppress_repeats(
            &mut later,
            DIRECT_REPEAT_SUPPRESSION,
            start + DIRECT_REPEAT_SUPPRESSION + Duration::from_millis(1),
        );

        assert_eq!(later.len(), 1, "a genuine re-reading must surface again");
    }

    #[test]
    fn egw_quote_window_keeps_a_quote_the_bible_window_truncates() {
        // The 2026-08-04 Great Controversy quote as it arrived across STT
        // finals. The 12-word Bible window keeps only the tail, so the shared
        // run with the paragraph topped out at 5 (cued-hint band, 75-80%).
        // The wider EGW window keeps the whole sentence, which the harness
        // scores at run=9 -> 92% auto-queue.
        let joined = [
            "And then there's Ellen White's quote that says",
            "Fearful is the issue to which the world is to be brought",
            "the powers of earth uniting",
            "to war against the commandment of God",
        ]
        .join(" ");

        let bible_window = clamp_to_recent_words(&joined, LIVE_DETECTION_WINDOW_WORDS);
        let egw_window = clamp_to_recent_words(&joined, LIVE_EGW_QUOTE_WINDOW_WORDS);

        assert!(
            !bible_window.contains("Fearful is the issue"),
            "bible window should still be tight: {bible_window}"
        );
        assert!(
            egw_window.contains("Fearful is the issue"),
            "egw window must retain the head of the quote: {egw_window}"
        );
        assert!(
            egw_window.contains("commandment of God"),
            "egw window must retain the tail of the quote: {egw_window}"
        );
    }

    #[test]
    fn clamp_to_recent_words_returns_all_when_under_limit() {
        assert_eq!(
            clamp_to_recent_words("john three sixteen", 12),
            "john three sixteen"
        );
    }

    #[test]
    fn clamp_to_recent_words_normalizes_empty_and_extra_whitespace() {
        assert_eq!(clamp_to_recent_words("", 12), "");
        assert_eq!(clamp_to_recent_words("   spaced   out  ", 12), "spaced out");
    }

    #[test]
    fn trim_to_sentence_start_drops_leading_partial_sentence() {
        assert_eq!(
            detection_logic::trim_to_sentence_start(
                "One, two, testing. The Lord is my shepherd; I shall not want",
                6
            ),
            "The Lord is my shepherd; I shall not want"
        );
    }

    #[test]
    fn trim_to_sentence_start_drops_multiple_stale_sentences() {
        assert_eq!(
            detection_logic::trim_to_sentence_start(
                "pastures. He restores my soul. He leads me beside the still waters today",
                6
            ),
            "He leads me beside the still waters today"
        );
    }

    #[test]
    fn trim_to_sentence_start_keeps_mixed_window_when_tail_is_too_short() {
        assert_eq!(
            detection_logic::trim_to_sentence_start(
                "not want He maketh me lie down green pastures. The Lord is",
                6
            ),
            "not want He maketh me lie down green pastures. The Lord is"
        );
    }

    #[test]
    fn trim_to_sentence_start_ignores_semicolons_and_no_punctuation() {
        assert_eq!(
            detection_logic::trim_to_sentence_start("The Lord is my shepherd; I shall not want", 6),
            "The Lord is my shepherd; I shall not want"
        );
    }

    #[test]
    fn strip_reference_scaffolding_removes_afrikaans_reference_words() {
        assert_eq!(
            strip_reference_scaffolding("Deuteronomium 16 vers 18 Regters en opsigters"),
            "Deuteronomium Regters en opsigters"
        );
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

    #[test]
    fn bible_detection_defaults_on_and_can_be_toggled_independently() {
        let app_state = crate::state::AppState::new();
        assert!(app_state
            .bible_detection_enabled
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!app_state
            .detection_paused
            .load(std::sync::atomic::Ordering::Relaxed));

        app_state
            .bible_detection_enabled
            .store(false, std::sync::atomic::Ordering::SeqCst);

        assert!(!app_state
            .bible_detection_enabled
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!app_state
            .detection_paused
            .load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_finalize_live_semantic_results_dedupes_and_boosts_overlap() {
        let results = vec![
            make_detection_result("John 3:16", 43, 3, 16, 0.86),
            make_detection_result("John 3:16", 43, 3, 16, 0.74),
            make_detection_result("Romans 8:28", 45, 8, 28, 0.72),
        ];

        let finalized = finalize_live_semantic_results(results, LIVE_SEMANTIC_MIN_CONFIDENCE);

        assert_eq!(finalized.len(), 2);
        assert_eq!(finalized[0].verse_ref, "John 3:16");
        assert!(
            finalized[0].confidence > 0.86,
            "overlap should boost the deduped result"
        );
    }

    #[test]
    fn finalize_live_semantic_results_drops_sub_floor_noise() {
        // Live FTS/semantic search emits ~63-68% keyword matches during prose.
        // They must be dropped at the source so they never reach the UI or IPC.
        let results = vec![
            make_detection_result("John 3:16", 43, 3, 16, 0.86),
            make_detection_result("Job 23:2", 18, 23, 2, 0.68),
            make_detection_result("Mark 15:4", 41, 15, 4, 0.64),
        ];

        let finalized = finalize_live_semantic_results(results, LIVE_SEMANTIC_MIN_CONFIDENCE);

        assert_eq!(finalized.len(), 1);
        assert_eq!(finalized[0].verse_ref, "John 3:16");
    }

    #[test]
    fn test_finalize_live_semantic_results_caps_after_dedupe() {
        let results = vec![
            make_detection_result("John 3:16", 43, 3, 16, 0.90),
            make_detection_result("John 3:16", 43, 3, 16, 0.75),
            make_detection_result("Romans 8:28", 45, 8, 28, 0.82),
            make_detection_result("Genesis 1:1", 1, 1, 1, 0.81),
            make_detection_result("Psalm 23:1", 19, 23, 1, 0.80),
            make_detection_result("Isaiah 53:5", 23, 53, 5, 0.79),
            make_detection_result("Matthew 5:3", 40, 5, 3, 0.78),
        ];

        let finalized = finalize_live_semantic_results(results, LIVE_SEMANTIC_MIN_CONFIDENCE);

        assert_eq!(finalized.len(), LIVE_SEMANTIC_CAP);
        assert!(finalized.iter().any(|r| r.verse_ref == "Romans 8:28"));
        assert!(finalized.iter().any(|r| r.verse_ref == "Genesis 1:1"));
    }

    #[test]
    fn reading_scope_filter_suppresses_out_of_chapter_semantic_bible_results() {
        let results = vec![
            make_detection_result("Isaiah 53:7", 23, 53, 7, 1.00),
            make_detection_result("Revelation 13:8", 66, 13, 8, 0.91),
            make_detection_result("Revelation 20:12", 66, 20, 12, 0.89),
        ];

        let filtered =
            detection_logic::filter_semantic_results_to_reading_scope(results, Some((66, 13)));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].verse_ref, "Revelation 13:8");
    }

    #[test]
    fn stale_reading_scope_releases_on_strong_out_of_book_semantic_hit() {
        // Reading mode anchored on John 5, but no verse has matched for 20s+
        // and the speaker is now paraphrasing Psalm 23 — release the scope.
        let results = vec![make_detection_result("Psalm 23:1", 19, 23, 1, 0.93)];

        assert!(detection_logic::should_release_stale_reading_scope(
            &results, 43, 5, 20, 0.75
        ));
    }

    #[test]
    fn stale_reading_scope_releases_on_strong_same_book_out_of_chapter_hit() {
        // 2026-07-07 incident: scope anchored on Jeremiah 1 while the speaker
        // quotes Jeremiah 29:11. Same book, different chapter — the strong hit
        // must release the stale scope instead of being suppressed for the
        // full reading-mode timeout.
        let results = vec![make_detection_result("Jeremiah 29:11", 24, 29, 11, 0.92)];

        assert!(detection_logic::should_release_stale_reading_scope(
            &results, 24, 1, 20, 0.75
        ));
        assert_eq!(
            detection_logic::strong_out_of_scope_bible_book(&results, 24, 1),
            Some((24, 29))
        );
    }

    #[test]
    fn stale_reading_scope_releases_on_operator_threshold_hit() {
        // 2026-07-07 second incident: "Matthew 1:2" spoken as a bare citation
        // anchored reading mode, the speaker never read Matthew 1, and every
        // Jeremiah quote-overlap hit (~0.79) sat below the old hardcoded 0.85
        // release bar — suppressed for the full 3-minute timeout. Once the
        // scope is stale, any hit worth SHOWING (>= the operator's semantic
        // threshold) must release it.
        let results = vec![make_detection_result("Jeremiah 29:11", 24, 29, 11, 0.79)];

        assert!(detection_logic::should_release_stale_reading_scope(
            &results, 40, 1, 25, 0.75
        ));

        // A hit below the operator threshold would never be emitted, so it
        // must not release the scope either.
        let weak = vec![make_detection_result("Job 23:2", 18, 23, 2, 0.70)];
        assert!(!detection_logic::should_release_stale_reading_scope(
            &weak, 40, 1, 25, 0.75
        ));
    }

    #[test]
    fn repeated_out_of_scope_release_uses_short_live_pause_boundary() {
        let results = vec![make_detection_result("Genesis 17:1", 1, 17, 1, 0.71)];

        assert_eq!(
            detection_logic::live_pause_out_of_scope_bible_book(&results, 59, 2, 5, 0.60),
            None
        );
        assert_eq!(
            detection_logic::live_pause_out_of_scope_bible_book(&results, 59, 2, 6, 0.60),
            Some((1, 17))
        );

        let weak = vec![make_detection_result("Job 23:2", 18, 23, 2, 0.59)];
        assert_eq!(
            detection_logic::live_pause_out_of_scope_bible_book(&weak, 59, 2, 6, 0.60),
            None
        );
    }

    #[test]
    fn active_reading_scope_is_not_released_while_verses_still_match() {
        // Parallel-passage echo while genuinely reading the chapter: reading
        // mode advanced recently, so out-of-book hits stay suppressed.
        let results = vec![make_detection_result("Mark 2:9", 41, 2, 9, 0.95)];

        assert!(!detection_logic::should_release_stale_reading_scope(
            &results, 43, 5, 5, 0.75
        ));
    }

    #[test]
    fn strong_out_of_scope_bible_book_ignores_weak_and_in_scope_hits() {
        let results = vec![
            make_detection_result("Job 23:2", 18, 23, 2, 0.72),
            make_detection_result("John 5:8", 43, 5, 8, 0.97),
        ];
        assert_eq!(
            detection_logic::strong_out_of_scope_bible_book(&results, 43, 5),
            None
        );

        let results = vec![make_detection_result("Psalm 23:2", 19, 23, 2, 0.92)];
        assert_eq!(
            detection_logic::strong_out_of_scope_bible_book(&results, 43, 5),
            Some((19, 23))
        );
    }

    #[test]
    fn stale_reading_scope_holds_without_a_strong_out_of_scope_hit() {
        // Weak out-of-book noise and in-scope hits never release the scope.
        let results = vec![
            make_detection_result("Job 23:2", 18, 23, 2, 0.72),
            make_detection_result("John 5:8", 43, 5, 8, 0.97),
        ];

        assert!(!detection_logic::should_release_stale_reading_scope(
            &results, 43, 5, 60, 0.75
        ));
    }

    #[test]
    fn reading_scope_filter_is_noop_without_active_scope() {
        let results = vec![
            make_detection_result("Isaiah 53:7", 23, 53, 7, 1.00),
            make_detection_result("Revelation 13:8", 66, 13, 8, 0.91),
        ];

        let filtered = detection_logic::filter_semantic_results_to_reading_scope(results, None);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|r| r.verse_ref == "Isaiah 53:7"));
        assert!(filtered.iter().any(|r| r.verse_ref == "Revelation 13:8"));
    }

    #[test]
    fn semantic_job_slot_replace_reports_whether_existing_job_was_replaced() {
        let slot = Arc::new(Mutex::new(None));

        let old = SemanticJob {
            seq: 1,
            text: "old".to_string(),
            egw_text: "old".to_string(),
            stt_confidence: 0.5,
        };
        let new = SemanticJob {
            seq: 2,
            text: "new".to_string(),
            egw_text: "new".to_string(),
            stt_confidence: 0.8,
        };
        assert!(!replace_semantic_job(&slot, old, "test"));
        assert!(replace_semantic_job(&slot, new.clone(), "test"));

        assert_eq!(take_semantic_job(&slot, "test"), Some(new));
        assert_eq!(take_semantic_job(&slot, "test"), None);
    }

    #[test]
    fn semantic_job_slot_recovers_from_poisoned_lock() {
        let slot = Arc::new(Mutex::new(None));

        let poisoned_slot = slot.clone();
        let _ = std::panic::catch_unwind(move || {
            let mut guard = poisoned_slot.lock().unwrap();
            guard.replace(SemanticJob {
                seq: 1,
                text: "poisoned".to_string(),
                egw_text: "poisoned".to_string(),
                stt_confidence: 0.4,
            });
            panic!("poison semantic slot");
        });

        assert!(replace_semantic_job(
            &slot,
            SemanticJob {
                seq: 2,
                text: "recovered".to_string(),
                egw_text: "recovered".to_string(),
                stt_confidence: 0.9,
            },
            "test"
        ));
        assert_eq!(
            take_semantic_job(&slot, "test"),
            Some(SemanticJob {
                seq: 2,
                text: "recovered".to_string(),
                egw_text: "recovered".to_string(),
                stt_confidence: 0.9,
            })
        );
    }

    #[test]
    fn deepgram_semantic_buffer_waits_until_speech_final() {
        let mut buffer = DeepgramSemanticBuffer::default();

        assert_eq!(buffer.push_final(1, "John 3".to_string(), false), None);
        assert_eq!(
            buffer.push_final(2, "sixteen".to_string(), true),
            Some((2, "John 3 sixteen".to_string()))
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn deepgram_semantic_buffer_flushes_duplicate_speech_final_boundary() {
        let mut buffer = DeepgramSemanticBuffer::default();

        assert_eq!(buffer.push_final(1, "Psalm 23".to_string(), false), None);
        assert_eq!(buffer.flush_with_seq(2), Some((2, "Psalm 23".to_string())));
        assert!(buffer.is_empty());
    }

    #[test]
    fn deepgram_semantic_buffer_utterance_end_uses_last_final_seq() {
        let mut buffer = DeepgramSemanticBuffer::default();

        assert_eq!(
            buffer.push_final(7, "The Lord is my shepherd".to_string(), false),
            None
        );
        assert_eq!(
            buffer.flush(),
            Some((7, "The Lord is my shepherd".to_string()))
        );
        assert!(buffer.is_empty());
    }
}

#[cfg(test)]
mod auto_queue_digit_growth_tests {
    use super::rebalance_auto_queue_for_digit_growth;
    use crate::commands::detection::DetectionResult;

    fn bible_hit(verse: i32, confidence: f64, auto_queued: bool) -> DetectionResult {
        DetectionResult {
            content_type: "bible".to_string(),
            verse_ref: format!("John 3:{verse}"),
            verse_text: String::new(),
            book_name: "John".to_string(),
            book_number: 43,
            chapter: 3,
            verse,
            confidence,
            rank_score: confidence,
            source: "direct".to_string(),
            auto_queued,
            transcript_snippet: String::new(),
            is_chapter_only: false,
            egw_paragraph: None,
            match_char_start: None,
        }
    }

    #[test]
    fn single_digit_citation_loses_auto_queue() {
        // "John 3 verse 1..." while "sixteen" is still arriving.
        let mut results = vec![bible_hit(3, 1.0, true)];
        rebalance_auto_queue_for_digit_growth(&mut results, 0.98, false);
        assert!(
            !results[0].auto_queued,
            "a provisional single-digit citation must not auto-fire"
        );
    }

    #[test]
    fn final_single_digit_citation_keeps_auto_queue() {
        let mut results = vec![bible_hit(8, 1.0, false)];
        rebalance_auto_queue_for_digit_growth(&mut results, 0.98, true);
        assert!(results[0].auto_queued);
    }

    #[test]
    fn final_single_digit_citation_does_not_create_a_second_auto_queue() {
        let mut results = vec![bible_hit(8, 1.0, true), bible_hit(16, 1.0, true)];
        rebalance_auto_queue_for_digit_growth(&mut results, 0.98, true);
        assert!(!results[0].auto_queued);
        assert!(results[1].auto_queued);
    }

    #[test]
    fn auto_queue_moves_to_the_digit_stable_citation() {
        // Live 2026-08-04: 3:1 consumed the merger's single auto-queue slot and
        // left the verse actually being read at auto_q=false.
        let mut results = vec![bible_hit(1, 1.0, true), bible_hit(16, 1.0, false)];
        rebalance_auto_queue_for_digit_growth(&mut results, 0.98, false);

        assert!(!results[0].auto_queued, "John 3:1 must lose auto-queue");
        assert!(results[1].auto_queued, "John 3:16 must inherit auto-queue");
    }

    #[test]
    fn re_award_respects_the_operator_threshold() {
        // 3:16 sits below the configured auto-queue threshold, so the merger
        // would never have auto-queued it — the strip must not hand it the flag.
        let mut results = vec![bible_hit(1, 1.0, true), bible_hit(16, 0.80, false)];
        rebalance_auto_queue_for_digit_growth(&mut results, 0.98, false);

        assert!(!results[0].auto_queued);
        assert!(
            !results[1].auto_queued,
            "a sub-threshold hit must not inherit auto-queue"
        );
    }

    #[test]
    fn manual_mode_awards_nothing() {
        // Manual mode sets the threshold to infinity, so nothing arrives
        // auto-queued and there is nothing to re-award.
        let mut results = vec![bible_hit(1, 1.0, false), bible_hit(16, 1.0, false)];
        rebalance_auto_queue_for_digit_growth(&mut results, f64::INFINITY, false);

        assert!(results.iter().all(|result| !result.auto_queued));
    }

    #[test]
    fn an_untouched_auto_queue_elsewhere_blocks_re_award() {
        // 3:33 already holds the slot; stripping 3:1 must not add a second.
        let mut results = vec![bible_hit(1, 1.0, true), bible_hit(33, 1.0, true)];
        rebalance_auto_queue_for_digit_growth(&mut results, 0.98, false);

        assert!(!results[0].auto_queued);
        assert!(results[1].auto_queued);
        assert_eq!(
            results.iter().filter(|result| result.auto_queued).count(),
            1
        );
    }
}

#[cfg(test)]
mod egw_cue_refresh_tests {
    use super::refresh_egw_cue_for_surviving_quote;
    use crate::commands::detection::DetectionResult;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn egw_quote() -> DetectionResult {
        DetectionResult {
            content_type: "egw".to_string(),
            verse_ref: "Patriarchs and Prophets p.322 par.1".to_string(),
            verse_text: String::new(),
            book_name: "Patriarchs and Prophets".to_string(),
            book_number: 1,
            chapter: 322,
            verse: 1,
            confidence: 0.92,
            rank_score: 0.92,
            source: "semantic".to_string(),
            auto_queued: false,
            transcript_snippet: String::new(),
            is_chapter_only: false,
            egw_paragraph: None,
            match_char_start: None,
        }
    }

    #[test]
    fn a_surviving_quote_extends_the_cue() {
        let cue = AtomicU64::new(1_000);
        refresh_egw_cue_for_surviving_quote(&cue, 9_000, true, &[egw_quote()]);
        assert_eq!(cue.load(Ordering::Relaxed), 9_000);
    }

    #[test]
    fn a_discarded_quote_does_not_extend_the_cue() {
        // dampen_egw_for_low_stt_confidence / retain_best_egw_quote ran and left
        // nothing; Bible semantic detection must be allowed to re-arm on time.
        let cue = AtomicU64::new(1_000);
        refresh_egw_cue_for_surviving_quote(&cue, 9_000, true, &[]);
        assert_eq!(cue.load(Ordering::Relaxed), 1_000);
    }

    #[test]
    fn no_cue_means_no_refresh() {
        let cue = AtomicU64::new(0);
        refresh_egw_cue_for_surviving_quote(&cue, 9_000, false, &[egw_quote()]);
        assert_eq!(cue.load(Ordering::Relaxed), 0);
    }
}
