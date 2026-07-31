use serde::Serialize;

use rhema_bible::{EgwParagraph, Verse};
use rhema_detection::MergedDetection;

use crate::state::AppState;

/// Serializable detection result for the frontend
#[derive(Clone, Serialize)]
pub struct DetectionResult {
    pub content_type: String,
    pub verse_ref: String,
    pub verse_text: String,
    pub book_name: String,
    pub book_number: i32,
    pub chapter: i32,
    pub verse: i32,
    pub confidence: f64,
    pub rank_score: f64,
    pub source: String,
    pub auto_queued: bool,
    pub transcript_snippet: String,
    /// True when detected from a chapter-only reference (verse defaults to 1, may be refined).
    pub is_chapter_only: bool,
    pub egw_paragraph: Option<EgwParagraph>,
    /// UTF-8 byte offset into `verse_text` where the spoken quote begins (EGW).
    /// `None` for Bible detections and for EGW hits without a measured run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_char_start: Option<usize>,
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
/// 1. Semantic `verse_id` mapped to the active translation by reference.
/// 2. By `book_number/chapter/verse_start` with active translation.
/// 3. Semantic `verse_id` source row fallback if the active translation is missing the verse.
/// 4. Fallback to unresolved `VerseRef` fields (no DB available).
pub fn to_result(state: &AppState, merged: &MergedDetection) -> DetectionResult {
    let vr = &merged.detection.verse_ref;
    let vid = merged.detection.verse_id;

    let resolved = state.bible_db.as_ref().and_then(|db| {
        let source_verse = vid.and_then(|id| resolve_semantic_verse_id(state, id));
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
        if source_verse.is_some() {
            return source_verse;
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
        content_type: "bible".to_string(),
        verse_ref: reference,
        verse_text,
        book_name,
        book_number,
        chapter,
        verse,
        confidence: merged.detection.confidence,
        rank_score: merged.detection.rank_score(),
        source: source_to_string(&merged.detection.source),
        auto_queued: merged.auto_queued,
        transcript_snippet: merged.detection.transcript_snippet.clone(),
        is_chapter_only: merged.detection.is_chapter_only,
        egw_paragraph: None,
        match_char_start: None,
    }
}

pub(super) fn egw_to_result(
    paragraph: EgwParagraph,
    confidence: f64,
    transcript_snippet: &str,
) -> DetectionResult {
    let reference = format!(
        "{} p.{} par.{}",
        paragraph.book_title, paragraph.page, paragraph.page_paragraph
    );

    DetectionResult {
        content_type: "egw".to_string(),
        verse_ref: reference,
        verse_text: paragraph.text.clone(),
        book_name: paragraph.book_title.clone(),
        book_number: paragraph.book_number,
        chapter: paragraph.page,
        verse: paragraph.page_paragraph,
        confidence,
        rank_score: confidence,
        source: "direct".to_string(),
        auto_queued: false,
        transcript_snippet: transcript_snippet.to_string(),
        is_chapter_only: false,
        egw_paragraph: Some(paragraph),
        match_char_start: None,
    }
}

pub(super) fn resolve_semantic_verse_id(state: &AppState, verse_id: i64) -> Option<Verse> {
    let db = state.bible_db.as_ref()?;
    match db.get_verse_by_id_in_translation(verse_id, state.active_translation_id) {
        Ok(Some(active_verse)) => {
            if active_verse.id != verse_id {
                log::debug!(
                    "[DET] Resolved semantic verse_id={} to active_translation_id={} as {} {}:{}",
                    verse_id,
                    state.active_translation_id,
                    active_verse.book_name,
                    active_verse.chapter,
                    active_verse.verse
                );
            }
            return Some(active_verse);
        }
        Ok(None) => {}
        Err(error) => {
            log::warn!(
                "[DET] Failed to resolve semantic verse_id={} in active_translation_id={}: {error}",
                verse_id,
                state.active_translation_id
            );
        }
    }

    match db.get_verse_by_id(verse_id) {
        Ok(source_verse) => source_verse,
        Err(error) => {
            log::warn!("[DET] Failed to resolve semantic source verse_id={verse_id}: {error}");
            None
        }
    }
}
