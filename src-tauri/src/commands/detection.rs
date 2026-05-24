#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]

use std::collections::HashSet;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use rhema_detection::{DetectionPipeline, MergedDetection, ReadingMode};

use crate::state::AppState;

/// Confidence assigned to the best FTS5 BM25 match (rank 0) in context search.
pub(crate) const FTS5_RANK0_CONFIDENCE: f64 = 0.75;

/// Confidence decrease per FTS5 rank position.
pub(crate) const FTS5_CONFIDENCE_DECAY: f64 = 0.04;

/// FTS5 results below this confidence are not included.
pub(crate) const FTS5_MIN_CONFIDENCE: f64 = 0.42;

/// Detection results at or above this confidence are visible to operators.
/// Auto-live/auto-queue uses the UI threshold separately.
pub(crate) const OPERATOR_DETECTION_THRESHOLD: f64 = 0.42;

/// Serializable detection result for the frontend
#[derive(Clone, Serialize)]
pub struct DetectionResult {
    pub verse_ref: String,
    pub verse_text: String,
    pub book_name: String,
    pub book_number: i32,
    pub chapter: i32,
    pub verse: i32,
    pub confidence: f64,
    pub source: String,
    pub auto_queued: bool,
    pub transcript_snippet: String,
    /// True when detected from a chapter-only reference (verse defaults to 1, may be refined).
    pub is_chapter_only: bool,
}

fn source_to_string(source: &rhema_detection::DetectionSource) -> String {
    match source {
        rhema_detection::DetectionSource::DirectReference => "direct".to_string(),
        rhema_detection::DetectionSource::Semantic { .. } => "semantic".to_string(),
    }
}

/// Resolve a detection to a full verse result using the database.
///
/// Resolution order:
/// 1. By `verse_id` (semantic detections with DB primary key)
/// 2. By `book_number/chapter/verse_start` with active translation (direct + FTS5 detections)
/// 3. Fallback to unresolved `VerseRef` fields (no DB available)
pub fn to_result(state: &AppState, merged: &MergedDetection) -> DetectionResult {
    let vr = &merged.detection.verse_ref;
    let vid = merged.detection.verse_id;

    let resolved = state.bible_db.as_ref().and_then(|db| {
        // Try verse_id first (vector-based semantic detections)
        if let Some(id) = vid {
            if let Ok(Some(v)) = db.get_verse_by_id(id) {
                return Some(v);
            }
        }
        // Fall back to book/chapter/verse lookup (direct + FTS5 detections)
        if vr.book_number > 0 && vr.chapter > 0 && vr.verse_start > 0 {
            if let Ok(Some(v)) = db.get_verse(
                state.active_translation_id,
                vr.book_number,
                vr.chapter,
                vr.verse_start,
            ) {
                return Some(v);
            }
        }
        None
    });

    let (reference, verse_text, book_name, book_number, chapter, verse) = if let Some(v) = resolved
    {
        let r = format!("{} {}:{}", v.book_name, v.chapter, v.verse);
        (r, v.text, v.book_name, v.book_number, v.chapter, v.verse)
    } else {
        let r = format!("{} {}:{}", vr.book_name, vr.chapter, vr.verse_start);
        (
            r,
            String::new(),
            vr.book_name.clone(),
            vr.book_number,
            vr.chapter,
            vr.verse_start,
        )
    };

    DetectionResult {
        verse_ref: reference,
        verse_text,
        book_name,
        book_number,
        chapter,
        verse,
        confidence: merged.detection.confidence,
        source: source_to_string(&merged.detection.source),
        auto_queued: merged.auto_queued,
        transcript_snippet: merged.detection.transcript_snippet.clone(),
        is_chapter_only: merged.detection.is_chapter_only,
    }
}

/// Run the detection pipeline on a piece of transcript text
#[tauri::command]
pub fn detect_verses(
    state: State<'_, Mutex<AppState>>,
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
    text: String,
) -> Result<Vec<DetectionResult>, String> {
    let merged = {
        let mut pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
        pipeline.process(&text)
    };
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let results: Vec<DetectionResult> = merged.iter().map(|m| to_result(&app_state, m)).collect();
    Ok(results)
}

/// Check if semantic search is available
#[tauri::command]
pub fn detection_status(
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
) -> Result<DetectionStatusResult, String> {
    let pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
    Ok(DetectionStatusResult {
        has_direct: true,
        has_semantic: pipeline.has_semantic(),
        paraphrase_enabled: pipeline.use_synonyms(),
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
pub struct DetectionStatusResult {
    pub has_direct: bool,
    pub has_semantic: bool,
    pub paraphrase_enabled: bool,
}

#[derive(Serialize)]
pub struct SemanticSearchResult {
    pub verse_ref: String,
    pub verse_text: String,
    pub book_name: String,
    pub book_number: i32,
    pub chapter: i32,
    pub verse: i32,
    pub similarity: f64,
}

#[tauri::command]
pub fn semantic_search(
    state: State<'_, Mutex<AppState>>,
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SemanticSearchResult>, String> {
    let k = limit.unwrap_or(10);

    // Lock pipeline for vector search (may be slow if ONNX runs). If the
    // optional semantic assets are absent, continue with the FTS5 fallback
    // below so context search still works in free/lightweight installs.
    let vector_results = {
        let mut pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
        if pipeline.has_semantic() {
            pipeline.semantic_search(&query, k)
        } else {
            Vec::new()
        }
    }; // Pipeline lock dropped

    // Lock AppState for DB lookups only (fast)
    let app_state = state.lock().map_err(|e| e.to_string())?;

    let mut results: Vec<SemanticSearchResult> = vector_results
        .into_iter()
        .filter_map(|(verse_id, similarity)| {
            if let Some(ref db) = app_state.bible_db {
                if let Ok(Some(v)) = db.get_verse_by_id(verse_id) {
                    return Some(SemanticSearchResult {
                        verse_ref: format!("{} {}:{}", v.book_name, v.chapter, v.verse),
                        verse_text: v.text,
                        book_name: v.book_name,
                        book_number: v.book_number,
                        chapter: v.chapter,
                        verse: v.verse,
                        similarity,
                    });
                }
            }
            None
        })
        .collect();

    // FTS5 BM25 across all English translations — resolve to active translation
    if let Some(ref db) = app_state.bible_db {
        let fts_results = db.search_verses_bm25(&query, k).unwrap_or_default();
        let seen: HashSet<(i32, i32, i32)> = results
            .iter()
            .map(|r| (r.book_number, r.chapter, r.verse))
            .collect();

        for (rank, fts) in fts_results.iter().enumerate() {
            if !seen.contains(&(fts.book_number, fts.chapter, fts.verse)) {
                #[expect(clippy::cast_precision_loss, reason = "rank is small")]
                let similarity = FTS5_RANK0_CONFIDENCE - (rank as f64 * FTS5_CONFIDENCE_DECAY);
                if similarity < FTS5_MIN_CONFIDENCE {
                    break;
                }
                // Resolve to active translation text
                if let Ok(Some(v)) = db.get_verse(
                    app_state.active_translation_id,
                    fts.book_number,
                    fts.chapter,
                    fts.verse,
                ) {
                    results.push(SemanticSearchResult {
                        verse_ref: format!("{} {}:{}", v.book_name, v.chapter, v.verse),
                        verse_text: v.text,
                        book_name: v.book_name,
                        book_number: v.book_number,
                        chapter: v.chapter,
                        verse: v.verse,
                        similarity,
                    });
                }
            }
        }
    }

    // Ensure highest similarity is always first
    results.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(results)
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

#[tauri::command]
pub fn update_detection_settings(
    merger_state: State<'_, Mutex<rhema_detection::DetectionMerger>>,
    pipeline_state: State<'_, Mutex<DetectionPipeline>>,
    auto_mode: bool,
    confidence_threshold: f64,
    cooldown_ms: u64,
) -> Result<(), String> {
    let threshold = confidence_threshold.clamp(0.0, 1.0);
    let auto_threshold = if auto_mode { threshold } else { 2.0 };

    {
        let mut merger = merger_state.lock().map_err(|e| e.to_string())?;
        apply_detection_settings_to_merger(&mut merger, auto_threshold, cooldown_ms);
    }

    {
        let mut pipeline = pipeline_state.lock().map_err(|e| e.to_string())?;
        apply_detection_settings_to_merger(pipeline.merger_mut(), auto_threshold, cooldown_ms);
    }

    log::info!(
        "[DET] Settings updated: auto_mode={auto_mode}, operator_threshold={OPERATOR_DETECTION_THRESHOLD:.2}, auto_threshold={auto_threshold:.2}, cooldown_ms={}",
        cooldown_ms.clamp(250, 60_000)
    );

    Ok(())
}

fn apply_detection_settings_to_merger(
    merger: &mut rhema_detection::DetectionMerger,
    auto_threshold: f64,
    cooldown_ms: u64,
) {
    merger.set_confidence_threshold(OPERATOR_DETECTION_THRESHOLD);
    merger.set_auto_queue_threshold(auto_threshold);
    merger.set_cooldown_ms(cooldown_ms.clamp(250, 60_000));
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
    use rhema_detection::{Detection, DetectionMerger, DetectionSource, VerseRef};

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
        }
    }

    #[test]
    fn detection_settings_keep_semantic_results_visible_below_auto_threshold() {
        let mut merger = DetectionMerger::new();

        apply_detection_settings_to_merger(&mut merger, 0.80, 2500);

        let results = merger.merge(vec![], vec![semantic_detection(0.50)]);

        assert!(
            (merger.confidence_threshold() - OPERATOR_DETECTION_THRESHOLD).abs() < f64::EPSILON
        );
        assert!((merger.auto_queue_threshold() - 0.80).abs() < f64::EPSILON);
        assert_eq!(results.len(), 1);
        assert!(!results[0].auto_queued);
    }

    #[test]
    fn manual_mode_disables_auto_queue_without_hiding_semantic_results() {
        let mut merger = DetectionMerger::new();

        apply_detection_settings_to_merger(&mut merger, 2.0, 2500);

        let results = merger.merge(vec![], vec![semantic_detection(0.72)]);

        assert_eq!(results.len(), 1);
        assert!(!results[0].auto_queued);
    }
}
