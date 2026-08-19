#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]

use std::sync::atomic::Ordering;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use rhema_detection::{DetectionPipeline, ReadingMode};

use super::validation::{
    bounded_optional_limit, bounded_text, MAX_QUERY_BYTES, MAX_TRANSCRIPT_BYTES,
};
use crate::state::AppState;

mod egw;
mod result;
mod semantic_search;
mod settings;

pub(crate) use egw::{
    apply_egw_auto_queue, dampen_egw_for_low_stt_confidence, detect_egw_quotes,
    drop_egw_quotes_echoing_scripture, detect_egw_references, egw_cue_is_currently_live,
    note_and_check_egw_cue, retain_best_egw_quote,
};
pub use result::{apply_presentation_grant, to_result, DetectionResult};
use semantic_search::{run_semantic_search, SemanticSearchResult};
use settings::{apply_detection_settings_to_merger, DEFAULT_SEMANTIC_VISIBILITY_THRESHOLD};

/// Confidence assigned to the best FTS5 BM25 match (rank 0) in context search.
pub(crate) const FTS5_RANK0_CONFIDENCE: f64 = 0.68;

/// Confidence decrease per FTS5 rank position.
pub(crate) const FTS5_CONFIDENCE_DECAY: f64 = 0.04;

/// FTS5 results below this confidence are not included.
pub(crate) const FTS5_MIN_CONFIDENCE: f64 = 0.50;

/// Direct detection results at or above this confidence are visible to operators.
/// Auto-live/auto-queue uses the UI threshold separately.
pub(crate) const OPERATOR_DETECTION_THRESHOLD: f64 = 0.70;

fn apply_bible_mode_to_reading_mode(enabled: bool, reading_mode: &mut ReadingMode) {
    if !enabled {
        reading_mode.deactivate();
    }
}

/// Run the detection pipeline on a piece of transcript text
#[tauri::command]
pub fn detect_verses(
    state: State<'_, Mutex<AppState>>,
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
    text: String,
) -> Result<Vec<DetectionResult>, String> {
    bounded_text(&text, "text", MAX_TRANSCRIPT_BYTES)?;
    let semantic_detection_enabled = {
        let app_state = state.lock().map_err(|e| e.to_string())?;
        app_state.semantic_detection_enabled.load(Ordering::Relaxed)
    };
    let merged = {
        let mut pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
        if semantic_detection_enabled {
            pipeline.process(&text)
        } else {
            pipeline.process_direct(&text)
        }
    };
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let mut results: Vec<DetectionResult> =
        merged.iter().map(|m| to_result(&app_state, m)).collect();
    results.extend(detect_egw_references(&app_state, &text));
    Ok(results)
}

/// Check if semantic search is available
#[tauri::command]
pub fn detection_status(
    state: State<'_, Mutex<AppState>>,
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
) -> Result<DetectionStatusResult, String> {
    let (bible_detection_enabled, semantic_detection_enabled) = {
        let app_state = state.lock().map_err(|e| e.to_string())?;
        (
            app_state.bible_detection_enabled.load(Ordering::Relaxed),
            app_state.semantic_detection_enabled.load(Ordering::Relaxed),
        )
    };
    let pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
    Ok(DetectionStatusResult {
        has_direct: true,
        has_semantic: pipeline.has_semantic(),
        paraphrase_enabled: pipeline.use_synonyms(),
        bible_detection_enabled,
        semantic_detection_enabled,
    })
}

/// Toggle paraphrase detection (synonym expansion) on/off
#[tauri::command]
pub fn toggle_paraphrase_detection(
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
    enabled: bool,
) -> Result<bool, String> {
    let mut pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
    pipeline.set_use_synonyms(enabled);
    log::info!("[DET] Paraphrase detection (synonyms) set to: {enabled}");
    Ok(enabled)
}

#[derive(Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Tauri status DTO mirrors frontend boolean readiness flags"
)]
pub struct DetectionStatusResult {
    pub has_direct: bool,
    pub has_semantic: bool,
    pub paraphrase_enabled: bool,
    pub bible_detection_enabled: bool,
    pub semantic_detection_enabled: bool,
}

#[tauri::command]
pub fn semantic_search(
    state: State<'_, Mutex<AppState>>,
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SemanticSearchResult>, String> {
    bounded_text(&query, "query", MAX_QUERY_BYTES)?;
    let k = bounded_optional_limit(limit, 10)?;
    let (vector_results, semantic_ready) = {
        let mut pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
        let semantic_ready = pipeline.has_semantic();
        if semantic_ready {
            (pipeline.semantic_search(&query, k), semantic_ready)
        } else {
            (Vec::new(), semantic_ready)
        }
    };
    let app_state = state.lock().map_err(|e| e.to_string())?;
    Ok(run_semantic_search(
        &app_state,
        &query,
        k,
        vector_results,
        semantic_ready,
    ))
}

/// Get reading mode status
#[tauri::command]
pub fn reading_mode_status(
    state: State<'_, Mutex<ReadingMode>>,
) -> Result<ReadingModeStatus, String> {
    let rm = state.lock().map_err(|e| e.to_string())?;
    Ok(ReadingModeStatus {
        active: rm.is_active(),
        current_verse: rm.current_verse(),
    })
}

#[derive(Serialize)]
pub struct ReadingModeStatus {
    pub active: bool,
    pub current_verse: Option<i32>,
}

/// Stop reading mode
#[tauri::command]
pub fn stop_reading_mode(state: State<'_, Mutex<ReadingMode>>) -> Result<(), String> {
    let mut rm = state.lock().map_err(|e| e.to_string())?;
    rm.deactivate();
    Ok(())
}

fn set_reading_mode_reference_inner(
    app_state: &AppState,
    reading_mode: &mut ReadingMode,
    book_number: i32,
    book_name: &str,
    chapter: i32,
    verse: i32,
) -> Result<(), String> {
    let db = app_state
        .bible_db
        .as_ref()
        .ok_or_else(|| "Bible database not initialized".to_string())?;
    // Auto-live commits are dispatched from the frontend and land after the STT
    // path has already advanced reading mode. Within the chapter we are already
    // reading, a commit for a verse at or behind the cursor is therefore stale
    // and must not re-anchor: the 2026-08-04 "John 3 verse 16" partial resolved
    // to 3:1 first, and its late commit dragged the projected verse back to 1.
    // Mirrors `should_restart_reading` for this frontend-driven path.
    if reading_mode.is_active()
        && reading_mode.current_book() == book_number
        && reading_mode.current_chapter() == chapter
        && reading_mode
            .current_verse()
            .is_some_and(|current| verse <= current)
    {
        return Ok(());
    }
    let chapter_verses = db
        .get_chapter(app_state.active_translation_id, book_number, chapter)
        .map_err(|error| error.to_string())?;
    if chapter_verses.is_empty() {
        return Err(format!("No verses found for {book_name} {chapter}"));
    }
    reading_mode.start(
        book_number,
        book_name,
        chapter,
        verse,
        chapter_verses
            .into_iter()
            .map(|item| (item.verse, item.text))
            .collect(),
    );
    Ok(())
}

/// Keep reading-mode navigation aligned when a semantic quote, rather than an
/// explicitly spoken citation, becomes the active live verse.
#[tauri::command]
pub fn set_reading_mode_reference(
    state: State<'_, Mutex<AppState>>,
    reading_mode_state: State<'_, Mutex<ReadingMode>>,
    book_number: i32,
    book_name: String,
    chapter: i32,
    verse: i32,
) -> Result<(), String> {
    let app_state = state.lock().map_err(|error| error.to_string())?;
    let mut reading_mode = reading_mode_state
        .lock()
        .map_err(|error| error.to_string())?;
    set_reading_mode_reference_inner(
        &app_state,
        &mut reading_mode,
        book_number,
        &book_name,
        chapter,
        verse,
    )
}

#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "Tauri command parameters are the serialized frontend settings payload"
)]
pub fn update_detection_settings(
    state: State<'_, Mutex<AppState>>,
    reading_mode_state: State<'_, Mutex<ReadingMode>>,
    merger_state: State<'_, Mutex<rhema_detection::DetectionMerger>>,
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
    auto_mode: bool,
    bible_detection_enabled: Option<bool>,
    semantic_detection_enabled: Option<bool>,
    confidence_threshold: f64,
    semantic_confidence_threshold: Option<f64>,
    cooldown_ms: u64,
) -> Result<(), String> {
    if !confidence_threshold.is_finite() {
        return Err("confidence_threshold must be finite".into());
    }
    if let Some(threshold) = semantic_confidence_threshold {
        if !threshold.is_finite() {
            return Err("semantic_confidence_threshold must be finite".into());
        }
    }
    let threshold = confidence_threshold.clamp(0.0, 1.0);
    let semantic_threshold = semantic_confidence_threshold
        .unwrap_or(DEFAULT_SEMANTIC_VISIBILITY_THRESHOLD)
        .clamp(0.0, 1.0);
    let bible_enabled = bible_detection_enabled.unwrap_or(true);
    let semantic_enabled = semantic_detection_enabled.unwrap_or(true);
    let auto_threshold = auto_mode.then_some(threshold);

    {
        let app_state = state.lock().map_err(|e| e.to_string())?;
        app_state
            .bible_detection_enabled
            .store(bible_enabled, Ordering::SeqCst);
        app_state
            .semantic_detection_enabled
            .store(semantic_enabled, Ordering::SeqCst);
        app_state
            .auto_mode
            .store(auto_mode, Ordering::SeqCst);
    }

    if !bible_enabled {
        let mut reading_mode = reading_mode_state
            .lock()
            .map_err(|error| error.to_string())?;
        apply_bible_mode_to_reading_mode(bible_enabled, &mut reading_mode);
    }

    {
        let mut merger = merger_state.lock().map_err(|e| e.to_string())?;
        apply_detection_settings_to_merger(
            &mut merger,
            auto_threshold,
            semantic_enabled,
            semantic_threshold,
            cooldown_ms,
        );
    }

    {
        let mut pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
        apply_detection_settings_to_merger(
            pipeline.merger_mut(),
            auto_threshold,
            semantic_enabled,
            semantic_threshold,
            cooldown_ms,
        );
    }

    log::info!(
        "[DET] Settings updated: auto_mode={auto_mode}, operator_threshold={OPERATOR_DETECTION_THRESHOLD:.2}, bible_enabled={bible_enabled}, semantic_enabled={semantic_enabled}, semantic_threshold={semantic_threshold:.2}, auto_threshold={}, cooldown_ms={}",
        auto_threshold.map_or_else(|| "disabled".to_string(), |value| format!("{value:.2}")),
        cooldown_ms.clamp(250, 60_000)
    );

    Ok(())
}

#[derive(Serialize)]
pub struct DetectionControlStatus {
    pub detection_paused: bool,
}

#[tauri::command]
pub fn set_detection_paused(
    state: State<'_, Mutex<AppState>>,
    paused: bool,
) -> Result<bool, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    app_state
        .detection_paused
        .store(paused, std::sync::atomic::Ordering::SeqCst);
    log::info!("[DET] Detection paused set to: {paused}");
    Ok(paused)
}

#[tauri::command]
pub fn detection_control_status(
    state: State<'_, Mutex<AppState>>,
) -> Result<DetectionControlStatus, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let detection_paused = app_state
        .detection_paused
        .load(std::sync::atomic::Ordering::Relaxed);
    Ok(DetectionControlStatus { detection_paused })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhema_detection::{Detection, DetectionMerger, DetectionSource, MergedDetection, VerseRef};

    struct DetectionFixture {
        state: AppState,
        _dir: tempfile::TempDir,
    }

    fn semantic_detection(confidence: f64) -> Detection {
        Detection {
            verse_ref: VerseRef {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 3,
                verse_start: 16,
                verse_end: None,
            },
            verse_id: None,
            confidence,
            source: DetectionSource::Semantic {
                similarity: confidence,
            },
            transcript_snippet: "testimony about grace and rescue".to_string(),
            detected_at: 0,
            is_chapter_only: false,
            ..Detection::default()
        }
    }

    fn semantic_merged_with_verse_id(verse_id: i64) -> MergedDetection {
        MergedDetection {
            detection: Detection {
                verse_ref: VerseRef {
                    book_number: 0,
                    book_name: String::new(),
                    chapter: 0,
                    verse_start: 0,
                    verse_end: None,
                },
                verse_id: Some(verse_id),
                confidence: 0.72,
                source: DetectionSource::Semantic { similarity: 0.72 },
                transcript_snippet: "God so loved the world".to_string(),
                detected_at: 0,
                is_chapter_only: false,
                ..Detection::default()
            },
            auto_queued: false,
        }
    }

    fn direct_merged_with_reference(book_number: i32, chapter: i32, verse: i32) -> MergedDetection {
        MergedDetection {
            detection: Detection {
                verse_ref: VerseRef {
                    book_number,
                    book_name: "John".to_string(),
                    chapter,
                    verse_start: verse,
                    verse_end: None,
                },
                verse_id: None,
                confidence: 0.98,
                source: DetectionSource::DirectReference,
                transcript_snippet: format!("John {chapter}:{verse}"),
                detected_at: 0,
                is_chapter_only: false,
                ..Detection::default()
            },
            auto_queued: true,
        }
    }

    fn fixture_state(active_translation_id: i64) -> DetectionFixture {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("rhema.db");
        let conn = rusqlite::Connection::open(&db_path).expect("open fixture db");
        conn.execute_batch(
            "CREATE TABLE translations (id INTEGER PRIMARY KEY, abbreviation TEXT, title TEXT, language TEXT, is_copyrighted INTEGER, is_downloaded INTEGER);
             CREATE TABLE books (id INTEGER PRIMARY KEY, translation_id INTEGER, book_number INTEGER, name TEXT, abbreviation TEXT, testament TEXT);
             CREATE TABLE verses (id INTEGER PRIMARY KEY, translation_id INTEGER, book_number INTEGER, book_name TEXT, book_abbreviation TEXT, chapter INTEGER, verse INTEGER, text TEXT);
             CREATE TABLE egw_books (id INTEGER PRIMARY KEY AUTOINCREMENT, book_number INTEGER NOT NULL UNIQUE, title TEXT NOT NULL, abbreviation TEXT NOT NULL, chapter_count INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE egw_paragraphs (id INTEGER PRIMARY KEY AUTOINCREMENT, book_id INTEGER NOT NULL, book_number INTEGER NOT NULL, book_title TEXT NOT NULL, chapter INTEGER NOT NULL, chapter_title TEXT NOT NULL, paragraph INTEGER NOT NULL, page INTEGER NOT NULL, page_paragraph INTEGER NOT NULL, text TEXT NOT NULL);
             INSERT INTO translations VALUES
               (1, 'KJV', 'King James', 'en', 0, 1),
               (2, 'NKJV', 'New King James', 'en', 1, 1),
               (3, 'NLT', 'New Living Translation', 'en', 1, 1),
               (4, 'SpaRV', 'Reina-Valera', 'es', 0, 1),
               (5, 'FreJND', 'J.N. Darby French', 'fr', 0, 1);
             INSERT INTO books VALUES
               (1, 1, 43, 'John', 'Jn', 'NT'),
               (2, 2, 43, 'John', 'Jn', 'NT'),
               (3, 3, 43, 'John', 'Jn', 'NT'),
               (4, 4, 43, 'Juan', 'Jn', 'NT'),
               (5, 5, 43, 'Jean', 'Jn', 'NT');
             INSERT INTO verses VALUES
               (99, 1, 43, 'John', 'Jn', 3, 1, 'KJV John 3:1 text.'),
               (100, 1, 43, 'John', 'Jn', 3, 16, 'KJV John 3:16 text.'),
               (101, 1, 43, 'John', 'Jn', 3, 17, 'KJV John 3:17 text.'),
               (200, 2, 43, 'John', 'Jn', 3, 16, 'NKJV John 3:16 text.'),
               (300, 3, 43, 'John', 'Jn', 3, 16, 'NLT John 3:16 text.'),
               (400, 4, 43, 'Juan', 'Jn', 3, 16, 'SpaRV Juan 3:16 text.'),
               (500, 5, 43, 'Jean', 'Jn', 3, 16, 'FreJND Jean 3:16 text.');
             INSERT INTO egw_books (book_number, title, abbreviation, chapter_count) VALUES
               (1, 'Patriarchs and Prophets', 'PP', 2),
               (2, 'The Desire of Ages', 'DA', 1);
             -- Rows 1-2: short paragraphs for explicit page/paragraph reference tests.
             -- Rows 3-5: longer SYNTHETIC paragraphs in EGW's register. Row 3 is the
             -- quote-detection positive control; rows 4-5 are adversarial negatives,
             -- written to sit close to the closing-sermon fixture's themes so that
             -- topical similarity alone would surface them. Not real EGW text.
             INSERT INTO egw_paragraphs (book_id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text) VALUES
               (1, 1, 'Patriarchs and Prophets', 1, 'Why Was Sin Permitted?', 2, 29, 2, 'The history of the great conflict.'),
               (2, 2, 'The Desire of Ages', 14, 'We Have Found the Messias', 3, 73, 3, 'Jesus had bidden Peter and his companions follow Him.'),
               (2, 2, 'The Desire of Ages', 15, 'The Shepherd And His Flock', 4, 480, 1, 'The shepherd does not remain in the fold waiting for the wandering sheep to return of itself, but he goes forth into the wilderness and searches until the lost is found, and he returns rejoicing with the burden upon his shoulders.'),
               (2, 2, 'The Desire of Ages', 15, 'The Shepherd And His Flock', 5, 481, 2, 'There is joy among the angels of heaven whenever a single wandering soul turns from the paths of sin, and the multitudes of the redeemed are never so precious to the Saviour as the one who was lost.'),
               (1, 1, 'Patriarchs and Prophets', 2, 'The Harvest', 6, 122, 4, 'Every act of service rendered in humility is recorded, and the report of the labourer is weighed not by the numbers gathered but by the faithfulness of the heart that laboured.');
             CREATE VIRTUAL TABLE egw_paragraphs_fts USING fts5(text, content='egw_paragraphs', content_rowid='id', tokenize='unicode61');
             INSERT INTO egw_paragraphs_fts(rowid, text) SELECT id, text FROM egw_paragraphs;",
        )
        .expect("fixture schema");
        drop(conn);

        let mut state = AppState::new();
        state.active_translation_id = active_translation_id;
        state.bible_db = Some(rhema_bible::BibleDb::open(&db_path).expect("open bible db"));

        DetectionFixture { state, _dir: dir }
    }

    #[test]
    fn detection_settings_keep_semantic_results_visible_below_auto_threshold() {
        let mut merger = DetectionMerger::new();

        apply_detection_settings_to_merger(
            &mut merger,
            Some(0.80),
            true,
            DEFAULT_SEMANTIC_VISIBILITY_THRESHOLD,
            2500,
        );

        let results = merger.merge(vec![], vec![semantic_detection(0.79)]);

        assert!(
            (merger.confidence_threshold() - OPERATOR_DETECTION_THRESHOLD).abs() < f64::EPSILON
        );
        assert!(
            (merger.semantic_confidence_threshold() - DEFAULT_SEMANTIC_VISIBILITY_THRESHOLD).abs()
                < f64::EPSILON
        );
        assert!((merger.auto_queue_threshold() - 0.80).abs() < f64::EPSILON);
        assert_eq!(results.len(), 1);
        assert!(!results[0].auto_queued);
    }

    #[test]
    fn manual_mode_disables_auto_queue_without_hiding_semantic_results() {
        let mut merger = DetectionMerger::new();

        apply_detection_settings_to_merger(
            &mut merger,
            None,
            true,
            DEFAULT_SEMANTIC_VISIBILITY_THRESHOLD,
            2500,
        );

        let results = merger.merge(vec![], vec![semantic_detection(0.79)]);

        assert_eq!(results.len(), 1);
        assert!(!results[0].auto_queued);
        assert!(merger.auto_queue_threshold().is_infinite());
    }

    #[test]
    fn detection_settings_apply_semantic_visibility_threshold() {
        let mut merger = DetectionMerger::new();

        apply_detection_settings_to_merger(&mut merger, Some(0.85), true, 0.70, 2500);

        assert!(
            (merger.confidence_threshold() - OPERATOR_DETECTION_THRESHOLD).abs() < f64::EPSILON
        );
        assert!((merger.semantic_confidence_threshold() - 0.70).abs() < f64::EPSILON);
        assert!(merger
            .merge(vec![], vec![semantic_detection(0.69)])
            .is_empty());
        assert_eq!(
            merger.merge(vec![], vec![semantic_detection(0.70)]).len(),
            1
        );
    }

    #[test]
    fn detection_settings_can_disable_semantic_results() {
        let mut merger = DetectionMerger::new();

        apply_detection_settings_to_merger(&mut merger, Some(0.85), false, 0.70, 2500);

        assert!(merger.semantic_confidence_threshold().is_infinite());
        assert!(merger
            .merge(vec![], vec![semantic_detection(1.0)])
            .is_empty());
    }

    #[test]
    fn to_result_resolves_semantic_vector_id_to_active_translation() {
        let fixture = fixture_state(3);
        let merged = semantic_merged_with_verse_id(100);

        let result = to_result(&fixture.state, &merged);

        assert_eq!(result.verse_ref, "John 3:16");
        assert_eq!(result.verse_text, "NLT John 3:16 text.");
        assert_eq!(result.source, "semantic");
    }

    #[test]
    fn to_result_resolves_semantic_vector_id_to_spanish_and_french_active_translations() {
        for (translation_id, expected_ref, expected_text) in [
            (4, "Juan 3:16", "SpaRV Juan 3:16 text."),
            (5, "Jean 3:16", "FreJND Jean 3:16 text."),
        ] {
            let fixture = fixture_state(translation_id);
            let merged = semantic_merged_with_verse_id(100);

            let result = to_result(&fixture.state, &merged);

            assert_eq!(result.verse_ref, expected_ref);
            assert_eq!(result.verse_text, expected_text);
            assert_eq!(result.source, "semantic");
        }
    }

    #[test]
    fn to_result_resolves_direct_reference_to_active_translation() {
        let fixture = fixture_state(2);
        let merged = direct_merged_with_reference(43, 3, 16);

        let result = to_result(&fixture.state, &merged);

        assert_eq!(result.verse_ref, "John 3:16");
        assert_eq!(result.verse_text, "NKJV John 3:16 text.");
        assert_eq!(result.source, "direct");
        assert!(result.auto_queued);
    }

    #[test]
    fn semantic_live_reference_seeds_next_verse_navigation() {
        let fixture = fixture_state(1);
        let mut reading_mode = ReadingMode::new();

        set_reading_mode_reference_inner(&fixture.state, &mut reading_mode, 43, "John", 3, 16)
            .expect("seed reading mode");

        let advance = reading_mode
            .check_transcript("Let's go to the next verse.")
            .expect("next verse command");
        assert_eq!(advance.book_name, "John");
        assert_eq!(advance.chapter, 3);
        assert_eq!(advance.verse, 17);
    }

    #[test]
    fn late_live_realignment_does_not_drag_cursor_backward() {
        // Auto-live commits lag the STT path. On 2026-08-04 the speaker said
        // "John 3 verse 16"; the partial resolved to John 3:1 first, so a late
        // commit for 3:1 landed after reading mode had already advanced to 3:16
        // and dragged the projected verse back (Started 3:1 -> 3:16 -> 3:1).
        let fixture = fixture_state(1);
        let mut reading_mode = ReadingMode::new();

        set_reading_mode_reference_inner(&fixture.state, &mut reading_mode, 43, "John", 3, 16)
            .expect("seed reading mode at 3:16");
        assert_eq!(reading_mode.current_verse(), Some(16));

        set_reading_mode_reference_inner(&fixture.state, &mut reading_mode, 43, "John", 3, 1)
            .expect("late auto-live commit for 3:1");

        assert_eq!(
            reading_mode.current_verse(),
            Some(16),
            "a late live commit for an earlier verse must not re-anchor backward"
        );
    }

    #[test]
    fn live_realignment_still_reanchors_forward() {
        let fixture = fixture_state(1);
        let mut reading_mode = ReadingMode::new();

        set_reading_mode_reference_inner(&fixture.state, &mut reading_mode, 43, "John", 3, 1)
            .expect("seed reading mode at 3:1");
        assert_eq!(reading_mode.current_verse(), Some(1));

        set_reading_mode_reference_inner(&fixture.state, &mut reading_mode, 43, "John", 3, 16)
            .expect("forward commit for 3:16");

        assert_eq!(
            reading_mode.current_verse(),
            Some(16),
            "forward re-anchoring within the chapter must still work"
        );
    }

    #[test]
    fn disabling_bible_mode_deactivates_reading_mode() {
        let mut reading_mode = ReadingMode::new();
        reading_mode.start(
            43,
            "John",
            3,
            16,
            vec![
                (16, "For God so loved the world".to_string()),
                (17, "For God sent not his Son".to_string()),
            ],
        );
        assert!(reading_mode.is_active());

        apply_bible_mode_to_reading_mode(false, &mut reading_mode);

        assert!(!reading_mode.is_active());
        assert!(!reading_mode.has_verses());
    }

    #[test]
    fn to_result_resolves_direct_reference_to_spanish_and_french_active_translations() {
        for (translation_id, expected_ref, expected_text) in [
            (4, "Juan 3:16", "SpaRV Juan 3:16 text."),
            (5, "Jean 3:16", "FreJND Jean 3:16 text."),
        ] {
            let fixture = fixture_state(translation_id);
            let merged = direct_merged_with_reference(43, 3, 16);

            let result = to_result(&fixture.state, &merged);

            assert_eq!(result.verse_ref, expected_ref);
            assert_eq!(result.verse_text, expected_text);
            assert_eq!(result.source, "direct");
        }
    }

    #[test]
    fn to_result_falls_back_to_source_translation_when_active_reference_missing() {
        let fixture = fixture_state(2);
        let merged = semantic_merged_with_verse_id(101);

        let result = to_result(&fixture.state, &merged);

        assert_eq!(result.verse_ref, "John 3:17");
        assert_eq!(result.verse_text, "KJV John 3:17 text.");
    }

    #[test]
    fn detect_egw_references_finds_title_page_paragraph() {
        let fixture = fixture_state(1);

        let results = detect_egw_references(
            &fixture.state,
            "please read Patriarchs and Prophets page twenty nine paragraph two",
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content_type, "egw");
        assert_eq!(results[0].verse_ref, "Patriarchs and Prophets p.29 par.2");
        assert_eq!(
            results[0].egw_paragraph.as_ref().map(|p| p.text.as_str()),
            Some("The history of the great conflict.")
        );
    }

    #[test]
    fn egw_direct_results_auto_queue_when_auto_mode_threshold_allows() {
        let fixture = fixture_state(1);
        let mut results = detect_egw_references(
            &fixture.state,
            "please read Patriarchs and Prophets page twenty nine paragraph two",
        );
        let mut merger = DetectionMerger::new();
        apply_detection_settings_to_merger(
            &mut merger,
            Some(0.80),
            true,
            DEFAULT_SEMANTIC_VISIBILITY_THRESHOLD,
            2500,
        );

        apply_egw_auto_queue(&mut results, &mut merger);

        assert_eq!(results.len(), 1);
        assert!(results[0].auto_queued);
    }

    #[test]
    fn egw_direct_results_do_not_auto_queue_in_manual_mode() {
        let fixture = fixture_state(1);
        let mut results = detect_egw_references(
            &fixture.state,
            "Patriarchs and Prophets page twenty nine paragraph two",
        );
        let mut merger = DetectionMerger::new();
        apply_detection_settings_to_merger(
            &mut merger,
            None,
            true,
            DEFAULT_SEMANTIC_VISIBILITY_THRESHOLD,
            2500,
        );

        apply_egw_auto_queue(&mut results, &mut merger);

        assert_eq!(results.len(), 1);
        assert!(!results[0].auto_queued);
    }

    #[test]
    fn detect_egw_references_resolves_reference_embedded_in_rolling_window() {
        // The live semantic gate runs this on the rolling transcript window so
        // Ellen White references that endpointing fragmented across finals are
        // still caught (surrounded by other speech).
        let fixture = fixture_state(1);

        let results = detect_egw_references(
            &fixture.state,
            "testing one two the desire of ages page seventy three paragraph three and then we continue",
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref, "The Desire of Ages p.73 par.3");
        assert_eq!(results[0].source, "direct");
    }

    #[test]
    fn detect_egw_references_handles_noisy_spoken_human_intro() {
        let fixture = fixture_state(1);

        let results = detect_egw_references(
            &fixture.state,
            "um can we go to patriarchs and prophets page twenty nine paragraph two please",
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content_type, "egw");
        assert_eq!(results[0].verse_ref, "Patriarchs and Prophets p.29 par.2");
        assert_eq!(results[0].source, "direct");
    }

    #[test]
    fn detect_egw_references_ignores_shorthand_references() {
        let fixture = fixture_state(1);

        let results = detect_egw_references(&fixture.state, "DA 73.3");

        assert!(results.is_empty());

        let results =
            detect_egw_references(&fixture.state, "DA page seventy three paragraph three");

        assert!(results.is_empty());
    }

    #[test]
    fn detect_egw_quotes_fires_on_verbatim_reading_with_attribution() {
        let fixture = fixture_state(1);
        // 12 consecutive content words lifted from DA p.480 par.1.
        let spoken = "the shepherd does not remain in the fold waiting for the wandering sheep to return of itself but he goes forth into the wilderness";

        let results = detect_egw_quotes(&fixture.state, spoken, true);

        assert_eq!(results.len(), 1, "expected exactly one EGW quote");
        assert_eq!(results[0].content_type, "egw");
        assert_eq!(results[0].source, "semantic");
        assert_eq!(results[0].chapter, 480);
        assert_eq!(results[0].verse, 1);
        assert!(results[0].auto_queued, "verbatim + cue should auto-queue");
    }

    #[test]
    fn detect_egw_quotes_fires_without_cue_but_does_not_auto_queue() {
        let fixture = fixture_state(1);
        let spoken = "the shepherd does not remain in the fold waiting for the wandering sheep to return of itself but he goes forth into the wilderness";

        let results = detect_egw_quotes(&fixture.state, spoken, false);

        assert_eq!(results.len(), 1);
        assert!(
            !results[0].auto_queued,
            "no attribution means no auto-queue"
        );
        assert!(results[0].confidence >= 0.88);
    }

    #[test]
    fn detect_egw_quotes_rejects_thematic_overlap() {
        let fixture = fixture_state(1);
        // Same subject as DA p.481 par.2, none of its phrasing.
        let spoken = "when even one person is saved there is rejoicing in heaven tonight";

        assert!(
            detect_egw_quotes(&fixture.state, spoken, false).is_empty(),
            "topical similarity alone must not fire"
        );
        assert!(
            detect_egw_quotes(&fixture.state, spoken, true).is_empty(),
            "a cue must not rescue a candidate with no shared phrasing"
        );
    }

    #[test]
    fn detect_egw_quotes_ignores_windows_below_the_word_floor() {
        let fixture = fixture_state(1);
        assert!(detect_egw_quotes(&fixture.state, "the shepherd goes", true).is_empty());
    }

    #[test]
    fn real_sermon_produces_no_egw_quote_detections() {
        let fixture = fixture_state(1);
        let raw = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../data/detection-fixtures/sermon-closing-2026-07.json"),
        )
        .expect("read sermon fixture");
        let cases: serde_json::Value = serde_json::from_str(&raw).expect("parse sermon fixture");
        let entries = cases.as_array().expect("fixture is an array");
        assert!(entries.len() > 50, "fixture should be a full sermon");

        let db = fixture.state.bible_db.as_ref().expect("fixture db");
        let mut nominated = 0;
        let mut false_positives = Vec::new();
        for entry in entries {
            let Some(text) = entry.get("text").and_then(serde_json::Value::as_str) else {
                continue;
            };
            nominated += db.search_egw_bm25(text, 1).expect("bm25 search").len();
            // Worst case: pretend attribution is live for every window.
            for cue_active in [false, true] {
                for result in detect_egw_quotes(&fixture.state, text, cue_active) {
                    false_positives.push(format!(
                        "cue={cue_active} {} ({:.0}%) <- {text}",
                        result.verse_ref,
                        result.confidence * 100.0
                    ));
                }
            }
        }

        // Non-vacuity: BM25 must actually reach into the EGW paragraphs on this
        // sermon, or the assertion below would pass without verifying anything.
        // Measured at 20 nominations across 56 eligible windows — that is the
        // flood the old flat-confidence path emitted.
        assert!(
            nominated >= 10,
            "BM25 nominated only {nominated} candidates; the negative control is vacuous"
        );

        assert!(
            false_positives.is_empty(),
            "EGW quote detection fired on a sermon containing no EGW quotation:\n{}",
            false_positives.join("\n")
        );
    }

    #[test]
    fn detect_egw_quotes_narrows_to_one_paragraph_for_live_emission() {
        let fixture = fixture_state(1);
        // Wording drawn from two different fixture paragraphs at once.
        let spoken = "the shepherd does not remain in the fold waiting for the wandering sheep and there is joy among the angels of heaven whenever a single wandering soul turns";

        // BM25 nominates several candidates so the spoken paragraph is not
        // missed; the live path narrows them to one operator-facing winner.
        let mut results = detect_egw_quotes(&fixture.state, spoken, true);
        retain_best_egw_quote(&mut results);

        assert!(results.len() <= 1);
    }

    #[test]
    fn detect_egw_references_requires_existing_paragraph() {
        let fixture = fixture_state(1);

        let results = detect_egw_references(
            &fixture.state,
            "Patriarchs and Prophets page twenty nine paragraph ninety nine",
        );

        assert!(results.is_empty());
    }
}
