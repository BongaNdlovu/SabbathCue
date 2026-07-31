use std::collections::HashSet;
use std::time::Instant;

use serde::Serialize;

use crate::state::AppState;

use super::result::resolve_semantic_verse_id;
use super::{FTS5_CONFIDENCE_DECAY, FTS5_MIN_CONFIDENCE, FTS5_RANK0_CONFIDENCE};

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

pub(super) fn run_semantic_search(
    app_state: &AppState,
    query: &str,
    limit: usize,
    vector_results: Vec<(i64, f64)>,
    semantic_ready: bool,
) -> Vec<SemanticSearchResult> {
    let t0 = Instant::now();
    let vector_hit_count = vector_results.len();

    let mut results: Vec<SemanticSearchResult> = vector_results
        .into_iter()
        .filter_map(|(verse_id, similarity)| {
            resolve_semantic_verse_id(app_state, verse_id).map(|v| SemanticSearchResult {
                verse_ref: format!("{} {}:{}", v.book_name, v.chapter, v.verse),
                verse_text: v.text,
                book_name: v.book_name,
                book_number: v.book_number,
                chapter: v.chapter,
                verse: v.verse,
                similarity,
            })
        })
        .collect();

    let mut fts_count = 0;
    if let Some(ref db) = app_state.bible_db {
        let fts_results = db
            .search_verses_bm25_scoped(query, limit, None)
            .unwrap_or_else(|e| {
                log::warn!("[semantic_search] FTS5/BM25 query failed: {e}");
                Vec::new()
            });
        fts_count = fts_results.len();
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

    results.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    log::info!(
        "[DET-SEMANTIC-SEARCH] words={} vector_hits={} fts_hits={} semantic_ready={} active_translation_id={} results={} elapsed={:?}",
        query.split_whitespace().count(),
        vector_hit_count,
        fts_count,
        semantic_ready,
        app_state.active_translation_id,
        results.len(),
        t0.elapsed()
    );

    results
}
