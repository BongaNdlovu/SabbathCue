#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]

use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

use super::validation::{bounded_limit, bounded_text, MAX_QUERY_BYTES};
use crate::state::AppState;
use rhema_bible::{BibleDb, Book, CrossReference, Translation, Verse};

const BIBLE_DB_NOT_LOADED: &str = "Bible database not loaded";

fn require_bible_db(db: Option<&BibleDb>) -> Result<&BibleDb, String> {
    db.ok_or_else(|| BIBLE_DB_NOT_LOADED.to_string())
}

fn translation_locked(translation: &Translation) -> bool {
    translation.is_copyrighted || !translation.is_downloaded
}

/// Refuse to serve verse text for locked translations. The live BM25 path
/// already enforces this in SQL (`run_fts_query`); without the same gate on
/// the direct lookup commands, any IPC caller could read copyrighted or
/// not-yet-downloaded translation text by ID.
fn ensure_translation_unlocked(db: &BibleDb, translation_id: i64) -> Result<(), String> {
    match db.translation_unlocked(translation_id) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "Translation ID {translation_id} is locked until licensing is ready"
        )),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn list_translations(state: State<'_, Mutex<AppState>>) -> Result<Vec<Translation>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = require_bible_db(app_state.bible_db.as_ref())?;
    db.list_translations().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_books(
    state: State<'_, Mutex<AppState>>,
    translation_id: i64,
) -> Result<Vec<Book>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = require_bible_db(app_state.bible_db.as_ref())?;
    db.list_books(translation_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_chapter(
    state: State<'_, Mutex<AppState>>,
    translation_id: i64,
    book_number: i32,
    chapter: i32,
) -> Result<Vec<Verse>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = require_bible_db(app_state.bible_db.as_ref())?;
    ensure_translation_unlocked(db, translation_id)?;
    db.get_chapter(translation_id, book_number, chapter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_verse(
    state: State<'_, Mutex<AppState>>,
    translation_id: i64,
    book_number: i32,
    chapter: i32,
    verse: i32,
) -> Result<Option<Verse>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = require_bible_db(app_state.bible_db.as_ref())?;
    ensure_translation_unlocked(db, translation_id)?;
    db.get_verse(translation_id, book_number, chapter, verse)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_verses(
    state: State<'_, Mutex<AppState>>,
    query: String,
    translation_id: i64,
    limit: usize,
) -> Result<Vec<Verse>, String> {
    bounded_text(&query, "query", MAX_QUERY_BYTES)?;
    let limit = bounded_limit(limit)?;
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = require_bible_db(app_state.bible_db.as_ref())?;
    ensure_translation_unlocked(db, translation_id)?;
    db.search_verses(&query, translation_id, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_cross_references(
    state: State<'_, Mutex<AppState>>,
    book_number: i32,
    chapter: i32,
    verse: i32,
) -> Result<Vec<CrossReference>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = require_bible_db(app_state.bible_db.as_ref())?;
    db.get_cross_references(book_number, chapter, verse)
        .map_err(|e| e.to_string())
}

/// Get the active translation ID
#[tauri::command]
pub fn get_active_translation(state: State<'_, Mutex<AppState>>) -> Result<i64, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    Ok(app_state.active_translation_id)
}

/// Set the active translation by ID
#[tauri::command]
pub fn set_active_translation(
    state: State<'_, Mutex<AppState>>,
    translation_id: i64,
) -> Result<i64, String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    // Verify the translation exists
    if let Some(ref db) = app_state.bible_db {
        let translations = db.list_translations().map_err(|e| e.to_string())?;
        let Some(translation) = translations.iter().find(|t| t.id == translation_id) else {
            return Err(format!("Translation ID {translation_id} not found"));
        };
        if translation_locked(translation) {
            return Err(format!(
                "{} is coming soon and cannot be selected yet",
                translation.abbreviation
            ));
        }
    }
    app_state.active_translation_id = translation_id;
    log::info!("[BIBLE] Active translation set to ID {translation_id}");
    Ok(translation_id)
}

#[derive(Serialize)]
pub struct VerseSearchRow {
    pub book_number: i32,
    pub book_name: String,
    pub chapter: i32,
    pub verse: i32,
    pub text: String,
}

#[tauri::command]
pub fn get_translation_verses_for_search(
    state: State<'_, Mutex<AppState>>,
    translation_id: i64,
) -> Result<Vec<VerseSearchRow>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = require_bible_db(app_state.bible_db.as_ref())?;
    ensure_translation_unlocked(db, translation_id)?;

    db.load_translation_verses_for_search(translation_id)
        .map(|rows| {
            rows.into_iter()
                .map(|v| VerseSearchRow {
                    book_number: v.book_number,
                    book_name: v.book_name,
                    chapter: v.chapter,
                    verse: v.verse,
                    text: v.text,
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{require_bible_db, translation_locked, BIBLE_DB_NOT_LOADED};
    use crate::commands::validation::{bounded_limit, bounded_text, MAX_QUERY_BYTES};
    use rhema_bible::Translation;

    fn translation(is_copyrighted: bool, is_downloaded: bool) -> Translation {
        Translation {
            id: 1,
            abbreviation: "TST".to_string(),
            title: "Test".to_string(),
            language: "en".to_string(),
            is_copyrighted,
            is_downloaded,
        }
    }

    #[test]
    fn require_bible_db_reports_stable_error() {
        assert_eq!(require_bible_db(None).unwrap_err(), BIBLE_DB_NOT_LOADED);
    }

    #[test]
    fn search_validation_runs_before_db_access() {
        let long_query = "x".repeat(MAX_QUERY_BYTES + 1);
        let err = bounded_text(&long_query, "query", MAX_QUERY_BYTES).unwrap_err();
        assert!(err.contains("query"));
        assert!(bounded_limit(0).is_err());
    }

    #[test]
    fn copyrighted_or_missing_translations_are_locked() {
        assert!(!translation_locked(&translation(false, true)));
        assert!(translation_locked(&translation(true, true)));
        assert!(translation_locked(&translation(false, false)));
    }
}
