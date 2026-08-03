use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use rusqlite::Connection;

use crate::db::BibleDb;
use crate::error::BibleError;
use crate::models::{Book, Verse};

/// A verse with its BM25 relevance rank from FTS5 full-text search.
/// Deduplicated across translations — one entry per unique verse reference.
pub struct Bm25Result {
    /// BM25 rank (negative; more negative = more relevant).
    pub rank: f64,
    pub book_number: i32,
    pub book_name: String,
    pub chapter: i32,
    pub verse: i32,
    pub is_broad_match: bool,
    /// True when this hit came from the quoted-phrase tier (contiguous span).
    pub is_phrase_match: bool,
    /// The matched verse's text, for downstream quote-overlap verification.
    pub text: String,
}

// ── Stop words ──────────────────────────────────────────────────────

/// Common English stop words that match nearly every Bible verse.
/// Filtering these keeps AND queries fast (~5-20ms instead of 200-1300ms).
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
    "from", "is", "it", "not", "be", "are", "was", "were", "been", "has", "have", "had", "do",
    "does", "did", "will", "would", "shall", "should", "may", "might", "can", "could", "that",
    "this", "these", "those", "he", "she", "we", "they", "you", "i", "me", "him", "her", "us",
    "them", "my", "his", "its", "our", "your", "their", "so", "if", "as", "no", "up", "all", "am",
    "about", "into", "when", "what", "which", "who", "whom", "how", "than", "then", "now", "just",
    "also", "very", "like", "even", "out", "there", "here", "die", "n", "en", "of", "maar", "in",
    "op", "aan", "vir", "van", "met", "deur", "uit", "tot", "oor", "onder", "by", "na", "is",
    "was", "wees", "het", "sal", "sou", "kan", "kon", "moet", "mag", "wil", "worden", "dit", "dat",
    "hierdie", "daardie", "hy", "sy", "ons", "julle", "hulle", "jy", "jou", "my", "hom", "haar",
    "hul", "syne", "se", "geen", "nie", "ook", "so", "dan", "toe", "nou", "daar", "hier", "as",
    "wat", "wie", "waar", "hoe", "wanneer", "al", "alles", "elke", "almal",
];

static STOP_WORD_SET: OnceLock<HashSet<&str>> = OnceLock::new();

fn is_stop_word(word: &str) -> bool {
    STOP_WORD_SET
        .get_or_init(|| STOP_WORDS.iter().copied().collect())
        .contains(word.to_lowercase().as_str())
}

/// Reference-mechanics tokens from spoken citations ("Verse 27", "chapter 2")
/// that (almost) never occur in verse text: digits and the verse/chapter
/// keywords across the supported STT languages. Left in the query they poison
/// AND tiers outright (no verse text contains "27") and waste OR-tier term
/// slots that the quoted verse content needs.
fn is_reference_noise_token(word: &str) -> bool {
    if word.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    matches!(
        word.to_lowercase().as_str(),
        "verse"
            | "verses"
            | "vers"
            | "verset"
            | "versets"
            | "versiculo"
            | "versículo"
            | "chapter"
            | "chapters"
            | "hoofstuk"
            | "capitulo"
            | "capítulo"
            | "chapitre"
    )
}

// ── FTS5 query builders ─────────────────────────────────────────────

/// Split input into FTS-safe alphanumeric terms.
pub(crate) fn query_terms(input: &str) -> impl Iterator<Item = &str> {
    input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
}

/// Exact phrase match — wraps entire input in double quotes.
/// `"Follow peace with all men"` matches only verses containing that exact sequence.
pub(crate) fn build_phrase_query(input: &str) -> String {
    let cleaned = query_terms(input).collect::<Vec<_>>().join(" ");
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    format!("\"{trimmed}\"")
}

/// Minimum terms in a phrase span. Below four, common word runs
/// ("and he said unto") match hundreds of verses and pollute the pool.
const MIN_PHRASE_SPAN_TERMS: usize = 4;
/// After stripping a leading stop word from an end-anchored span, allow this
/// short distinctive length ("the everlasting gospel") — still too short for
/// the primary ladder, so it is only emitted as a stop-stripped variant.
const MIN_STRIPPED_PHRASE_TERMS: usize = 3;
/// Longest span worth issuing. A verse is rarely more than this many words,
/// so longer spans cannot match and would only consume the round-trip budget.
const MAX_PHRASE_SPAN_TERMS: usize = 12;
/// Upper bound on end-anchored SQL round trips added by the phrase tier
/// (includes stop-stripped short variants appended after the primary ladder).
const MAX_PHRASE_SPANS: usize = 12;
/// Length used for interior (non-tail) spans. One length only: sliding every
/// length over every offset is O(n^2) SQL round trips.
const INTERIOR_SPAN_TERMS: usize = 6;
/// Upper bound on interior spans, on top of `MAX_PHRASE_SPANS`.
const MAX_INTERIOR_SPANS: usize = 6;

fn push_unique_span(spans: &mut Vec<String>, terms: &[&str]) {
    if terms.len() < MIN_STRIPPED_PHRASE_TERMS {
        return;
    }
    let span = format!("\"{}\"", terms.join(" "));
    if !spans.contains(&span) {
        spans.push(span);
    }
}

/// Build phrase spans and report how many leading entries are end-anchored
/// (the rest, if any, are interior).
///
/// End-anchored spans come first (quotation finishes at the window tail).
/// Stop-stripped short variants of the tail cover three-word quotes that
/// follow a light verb ("it is the everlasting gospel"). If those cannot
/// match, bounded interior 6-grams walk from the tail so mid-window quotes
/// remain reachable without O(n^2) SQL. Total length of this list is at most
/// `MAX_PHRASE_SPANS + MAX_INTERIOR_SPANS`.
pub(crate) fn build_phrase_spans_with_end_count(input: &str) -> (Vec<String>, usize) {
    let terms: Vec<&str> = query_terms(input).collect();
    if terms.len() < MIN_STRIPPED_PHRASE_TERMS {
        return (Vec::new(), 0);
    }
    let mut spans = Vec::new();
    if terms.len() >= MIN_PHRASE_SPAN_TERMS {
        let longest = terms.len().min(MAX_PHRASE_SPAN_TERMS);
        for len in (MIN_PHRASE_SPAN_TERMS..=longest).rev() {
            if spans.len() >= MAX_PHRASE_SPANS {
                break;
            }
            push_unique_span(&mut spans, &terms[terms.len() - len..]);
        }
    }

    // Stop-stripped tail variants: "is the everlasting gospel" → also try
    // "the everlasting gospel". Budget-limited; only leading stops dropped.
    if terms.len() >= MIN_PHRASE_SPAN_TERMS && spans.len() < MAX_PHRASE_SPANS {
        let tail = &terms[terms.len() - MIN_PHRASE_SPAN_TERMS..];
        if is_stop_word(tail[0]) {
            push_unique_span(&mut spans, &tail[1..]);
        }
    }

    let end_n = spans.len();

    // Interior spans only after end-anchored set is built. The SQL runner
    // tries end-anchored first and only continues to interior when those
    // return no rows (see search_verses_bm25_scoped).
    if terms.len() > INTERIOR_SPAN_TERMS {
        for start in (0..=terms.len() - INTERIOR_SPAN_TERMS).rev() {
            if spans.len() >= end_n + MAX_INTERIOR_SPANS {
                break;
            }
            push_unique_span(&mut spans, &terms[start..start + INTERIOR_SPAN_TERMS]);
        }
    }
    (spans, end_n)
}

/// Quoted phrase queries for the input, longest first (end-anchored then interior).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_phrase_spans(input: &str) -> Vec<String> {
    build_phrase_spans_with_end_count(input).0
}

/// AND query with stop words removed — all significant words must be present.
/// `"be doers of the word"` → `doers word` (finds James 1:22).
/// Capped at 12 terms to prevent expensive queries on long text.
pub(crate) fn build_and_query(input: &str) -> String {
    let mut seen = HashSet::new();
    let tokens: Vec<String> = query_terms(input)
        .filter(|w| w.len() >= 2 && !is_stop_word(w) && !is_reference_noise_token(w))
        .filter(|w| seen.insert(w.to_lowercase()))
        .take(12)
        .map(ToOwned::to_owned)
        .collect();
    if tokens.is_empty() {
        return String::new();
    }
    tokens.join(" ")
}

/// OR query with stop words removed — any significant word matches.
/// `"It's a new creature Old things passed away"` → `"creature" OR "things" OR "passed" OR "away"`.
/// KJV name aliases are appended to modern spoken names. Capped at 12 terms
/// to prevent expensive queries while leaving room for a few expansions.
pub(crate) fn build_or_query(input: &str) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates = Vec::new();
    for (position, word) in query_terms(input).enumerate() {
        if word.len() < 3 || is_stop_word(word) || is_reference_noise_token(word) {
            continue;
        }
        if seen.insert(word.to_ascii_lowercase()) {
            candidates.push((position, word.to_string()));
        }
        for variant in crate::kjv_names::kjv_variants(word) {
            if seen.insert((*variant).to_string()) {
                candidates.push((position, (*variant).to_string()));
            }
        }
        for variant in spoken_kjv_variants(word) {
            if seen.insert((*variant).to_string()) {
                candidates.push((position, (*variant).to_string()));
            }
        }
    }
    // Longer words carry more retrieval information in fallback OR searches.
    // Select them before applying the fixed query budget, then restore speech
    // order to keep generated queries deterministic and readable.
    candidates.sort_by(|(left_position, left), (right_position, right)| {
        right
            .len()
            .cmp(&left.len())
            .then_with(|| left_position.cmp(right_position))
    });
    candidates.truncate(12);
    candidates.sort_by_key(|(position, _)| *position);
    if candidates.is_empty() {
        return String::new();
    }
    candidates
        .into_iter()
        .map(|(_, token)| format!("\"{token}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn spoken_kjv_variants(word: &str) -> &'static [&'static str] {
    match word.to_ascii_lowercase().as_str() {
        "boat" | "boats" => &["ship"],
        "prison" => &["prisoner", "prisoners"],
        "storm" | "storms" => &["wind", "sea", "calm", "tempest"],
        "baptist" | "baptizing" | "baptizes" | "baptize" | "baptism" => {
            &["baptized", "baptize", "baptizing", "baptism"]
        }
        "nicodemus" => &["born", "again"],
        _ => &[],
    }
}

// ── SQL runner ──────────────────────────────────────────────────────

/// Execute a BM25-ranked FTS5 query across unlocked translations.
#[expect(
    clippy::cast_possible_wrap,
    reason = "limit is a small page-size value that fits in i64"
)]
fn run_fts_query(
    conn: &Connection,
    fts_query: &str,
    limit: usize,
    is_broad_match: bool,
    is_phrase_match: bool,
    book_hint: Option<i32>,
) -> Result<Vec<Bm25Result>, BibleError> {
    if fts_query.is_empty() {
        return Ok(vec![]);
    }
    // `?3 IS NULL` makes the filter inert when no book was named, so hinted
    // and unhinted queries share one prepared statement and one plan.
    let mut stmt = conn.prepare(
        "SELECT bm25(verses_fts) as rank, v.book_number, v.book_name, v.chapter, v.verse, v.text \
         FROM verses_fts fts \
         JOIN verses v ON v.rowid = fts.rowid \
         JOIN translations t ON t.id = v.translation_id \
         WHERE fts.text MATCH ?1 AND t.is_copyrighted = 0 AND t.is_downloaded = 1 \
           AND (?3 IS NULL OR v.book_number = ?3) \
         ORDER BY rank \
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![fts_query, limit as i64, book_hint],
        |row: &rusqlite::Row| {
            Ok(Bm25Result {
                rank: row.get(0)?,
                book_number: row.get(1)?,
                book_name: row.get(2)?,
                chapter: row.get(3)?,
                verse: row.get(4)?,
                is_broad_match,
                is_phrase_match,
                text: row.get(5)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(BibleError::from)
}

/// Deduplicate results by (`book_number`, chapter, verse), keeping the strongest
/// (most negative) BM25 score for each verse.
///
/// A verse can surface from several FTS tiers (phrase, AND, OR) with different
/// scores; keeping the strongest makes the score a reliable relevance signal for
/// downstream gating. First-seen order is preserved.
fn dedup_results(results: Vec<Bm25Result>, limit: usize) -> Vec<Bm25Result> {
    let mut order: Vec<(i32, i32, i32)> = Vec::new();
    let mut best: HashMap<(i32, i32, i32), Bm25Result> = HashMap::new();
    for result in results {
        let key = (result.book_number, result.chapter, result.verse);
        match best.get_mut(&key) {
            Some(existing) if result.rank < existing.rank => {
                let is_broad_match = existing.is_broad_match && result.is_broad_match;
                let is_phrase_match = existing.is_phrase_match || result.is_phrase_match;
                *existing = Bm25Result {
                    is_broad_match,
                    is_phrase_match,
                    ..result
                };
            }
            Some(existing) => {
                existing.is_broad_match &= result.is_broad_match;
                existing.is_phrase_match |= result.is_phrase_match;
            }
            None => {
                order.push(key);
                best.insert(key, result);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|key| best.remove(&key))
        .take(limit)
        .collect()
}

fn dedup_count(results: &[Bm25Result]) -> usize {
    let mut seen = HashSet::new();
    results
        .iter()
        .filter(|r| seen.insert((r.book_number, r.chapter, r.verse)))
        .count()
}

fn build_short_clause_and_queries(input: &str) -> Vec<String> {
    let mut clauses: Vec<(usize, String)> = input
        .split(['.', '!', '?'])
        .filter_map(|clause| {
            let query = build_and_query(clause);
            let terms = query.split_whitespace().count();
            (3..=6).contains(&terms).then_some((terms, query))
        })
        .collect();
    clauses.sort_by_key(|(terms, _)| *terms);
    clauses.truncate(4);
    clauses.into_iter().map(|(_, query)| query).collect()
}

/// Short concept anchors for modern event descriptions whose wording differs
/// from the KJV text. These are issued before the rolling phrase spans so the
/// anchor verse enters the candidate pool even when surrounding names or
/// commentary dilute BM25 ranking.
fn build_topic_phrase_queries(input: &str) -> Vec<String> {
    let lower = input.to_ascii_lowercase();
    let mut queries = Vec::new();
    if lower.contains("born again") {
        queries.push("\"born again\"".to_string());
    }
    if lower.contains("baptiz") {
        if lower.contains("jesus") {
            queries.push("baptized Jesus".to_string());
        }
        queries.push("\"baptized\"".to_string());
    }
    queries
}

// ── BibleDb methods ─────────────────────────────────────────────────

impl BibleDb {
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned (i.e., a thread panicked
    /// while holding the database lock).
    pub fn search_verses(
        &self,
        query: &str,
        translation_id: i64,
        limit: usize,
    ) -> Result<Vec<Verse>, BibleError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| BibleError::Internal(e.to_string()))?;
        let sanitized = build_and_query(query);
        if sanitized.is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = conn.prepare(
            "SELECT v.id, v.translation_id, v.book_number, v.book_name, v.book_abbreviation, v.chapter, v.verse, v.text \
             FROM verses_fts fts \
             JOIN verses v ON v.rowid = fts.rowid \
             WHERE fts.text MATCH ?1 AND v.translation_id = ?2 \
             LIMIT ?3",
        )?;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "limit is a small page-size value that fits in i64"
        )]
        let limit_i64 = limit as i64;
        let rows = stmt.query_map(
            rusqlite::params![sanitized, translation_id, limit_i64],
            |row: &rusqlite::Row| {
                Ok(Verse {
                    id: row.get(0)?,
                    translation_id: row.get(1)?,
                    book_number: row.get(2)?,
                    book_name: row.get(3)?,
                    book_abbreviation: row.get(4)?,
                    chapter: row.get(5)?,
                    verse: row.get(6)?,
                    text: row.get(7)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Unscoped search. Retained so existing callers and tests are unaffected.
    pub fn search_verses_bm25(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Bm25Result>, BibleError> {
        self.search_verses_bm25_scoped(query, limit, None)
    }

    /// Search verses using FTS5 with BM25 ranking across unlocked translations.
    ///
    /// Three-tier strategy with stop-word filtering for speed:
    /// 1. **Phrase** — end-anchored exact substring spans, longest first (~5ms)
    /// 2. **AND** — all significant words present, stop words removed (~5-20ms)
    /// 3. **OR** — any significant word matches, capped at 10 terms (~10-30ms)
    ///
    /// When `book_hint` is `Some`, every tier is restricted to that book number
    /// so a spoken book name scopes retrieval instead of becoming a text term.
    ///
    /// Results are deduplicated by verse reference across translations.
    pub fn search_verses_bm25_scoped(
        &self,
        query: &str,
        limit: usize,
        book_hint: Option<i32>,
    ) -> Result<Vec<Bm25Result>, BibleError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| BibleError::Internal(e.to_string()))?;
        let fetch_limit = limit * 4;

        // Query text is spoken content and must not reach release logs (see
        // `transcript_logging_decision`); log only tier term counts. The query
        // itself is available in debug builds via the gated app-layer
        // `[DET-SEMANTIC] Running on:` line.

        // Tier 1: Exact phrase match.
        // Phase A: end-anchored spans longest first — stop at first hit.
        // Phase B: only if phase A returned nothing, try interior spans
        // (mid-window quotes). Never skip interiors just because a long
        // end-anchored span was *attempted*; only skip when a hit exists.
        let term_count = query_terms(query).count();
        log::debug!("[FTS5-BM25] phrase tier: {term_count} terms");
        let (spans, end_n) = build_phrase_spans_with_end_count(query);
        let mut all_results = Vec::new();
        for topic_query in build_topic_phrase_queries(query) {
            all_results.extend(run_fts_query(
                &conn,
                &topic_query,
                fetch_limit,
                false,
                true,
                book_hint,
            )?);
        }
        if all_results.is_empty() {
            for span in spans.iter().take(end_n) {
                all_results = run_fts_query(&conn, span, fetch_limit, false, true, book_hint)?;
                if !all_results.is_empty() {
                    break;
                }
            }
        }
        if all_results.is_empty() {
            for span in spans.iter().skip(end_n) {
                all_results = run_fts_query(&conn, span, fetch_limit, false, true, book_hint)?;
                if !all_results.is_empty() {
                    break;
                }
            }
        }

        // Tier 2: AND with stop words filtered (~5-20ms)
        if dedup_count(&all_results) < limit {
            for clause_query in build_short_clause_and_queries(query) {
                all_results.extend(run_fts_query(
                    &conn,
                    &clause_query,
                    fetch_limit,
                    false,
                    false,
                    book_hint,
                )?);
            }
            let and_q = build_and_query(query);
            if !and_q.is_empty() {
                log::debug!(
                    "[FTS5-BM25] AND tier: {} terms",
                    and_q.split_whitespace().count()
                );
                all_results.extend(run_fts_query(
                    &conn,
                    &and_q,
                    fetch_limit,
                    false,
                    false,
                    book_hint,
                )?);
            }
        }

        // Tier 3: OR with stop words filtered, capped at 10 terms (~10-30ms)
        if dedup_count(&all_results) < limit {
            let or_q = build_or_query(query);
            if !or_q.is_empty() {
                log::debug!(
                    "[FTS5-BM25] OR tier: {} terms",
                    or_q.matches(" OR ").count() + 1
                );
                all_results.extend(run_fts_query(
                    &conn,
                    &or_q,
                    fetch_limit,
                    true,
                    false,
                    book_hint,
                )?);
            }
        }

        let results = dedup_results(all_results, limit);
        log::info!("[FTS5-BM25] Found {} unique verses", results.len());
        Ok(results)
    }

    pub fn search_books(&self, query: &str) -> Result<Vec<Book>, BibleError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| BibleError::Internal(e.to_string()))?;
        let pattern = format!("{query}%");
        let mut stmt = conn.prepare(
            "SELECT id, translation_id, book_number, name, abbreviation, testament \
             FROM books \
             WHERE name LIKE ?1 OR abbreviation LIKE ?1 \
             ORDER BY book_number",
        )?;
        let rows = stmt.query_map(rusqlite::params![pattern], |row: &rusqlite::Row| {
            Ok(Book {
                id: row.get(0)?,
                translation_id: row.get(1)?,
                book_number: row.get(2)?,
                name: row.get(3)?,
                abbreviation: row.get(4)?,
                testament: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn fixture_db() -> BibleDb {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE translations (id INTEGER PRIMARY KEY, abbreviation TEXT, title TEXT, language TEXT, is_copyrighted INTEGER, is_downloaded INTEGER);
             CREATE TABLE verses (id INTEGER PRIMARY KEY, translation_id INTEGER, book_number INTEGER, book_name TEXT, book_abbreviation TEXT, chapter INTEGER, verse INTEGER, text TEXT);
             CREATE VIRTUAL TABLE verses_fts USING fts5(text, content='verses', content_rowid='id', tokenize='unicode61');
             INSERT INTO translations VALUES
               (1, 'KJV', 'King James', 'en', 0, 1),
               (2, 'Afr1953', 'Afrikaans 1933/1953 Bybel', 'af', 1, 1);
             INSERT INTO verses VALUES
               (1, 1, 5, 'Deuteronomy', 'Deut', 16, 18, 'Judges and officers shalt thou make thee in all thy gates.'),
               (2, 2, 5, 'Deuteronomium', 'Deut', 16, 18, 'Regters en opsigters moet jy vir jou aanstel in al jou poorte.'),
               (3, 1, 40, 'Matthew', 'Matt', 24, 37, 'But as the days of Noe were, so shall also the coming of the Son of man be.');
             INSERT INTO verses_fts(rowid, text) SELECT id, text FROM verses;",
        )
        .unwrap();
        BibleDb {
            conn: Mutex::new(conn),
        }
    }

    fn bm25_with_broad_match(
        rank: f64,
        book_number: i32,
        chapter: i32,
        verse: i32,
        is_broad_match: bool,
    ) -> Bm25Result {
        Bm25Result {
            rank,
            book_number,
            book_name: format!("Book{book_number}"),
            chapter,
            verse,
            is_broad_match,
            is_phrase_match: false,
            text: String::new(),
        }
    }

    fn bm25(rank: f64, book_number: i32, chapter: i32, verse: i32) -> Bm25Result {
        bm25_with_broad_match(rank, book_number, chapter, verse, false)
    }

    #[test]
    fn broad_query_keeps_distinctive_late_quote_terms() {
        let query = build_or_query(
            "Many will not be lost because of great and terrible sins but because of small +             compromises and indecision. We hear the story of Belshazzar and Nebuchadnezzar +             and lift our eyes to heaven and say Lord help me. I believe but help my unbelief. +             Deliver me from myself and from the power of the enemy.",
        );

        assert!(
            query.contains("\"unbelief\""),
            "the bounded fallback query must retain distinctive late quote terms: {query}"
        );
    }

    #[test]
    fn short_clause_queries_isolate_an_embedded_modern_quote() {
        let queries = build_short_clause_and_queries(
            "We lift our eyes to heaven and say Lord help me. I believe but help my unbelief. Deliver me from the enemy.",
        );

        assert!(
            queries.iter().any(|query| query == "believe help unbelief"),
            "short embedded quotation must receive its own strict query: {queries:?}"
        );
    }

    #[test]
    fn phrase_spans_are_end_anchored_longest_first() {
        let (spans, end_n) = build_phrase_spans_with_end_count("he was saying such a time as this");
        assert_eq!(spans[0], "\"he was saying such a time as this\"");
        assert_eq!(spans[1], "\"was saying such a time as this\"");
        assert!(spans.contains(&"\"such a time as this\"".to_string()));
        // Primary ladder stays at the four-term floor; exactly one derived
        // stop-stripped 3-gram may follow it.
        let short_end_spans: Vec<&str> = spans
            .iter()
            .take(end_n)
            .filter(|span| span.split_whitespace().count() < MIN_PHRASE_SPAN_TERMS)
            .map(String::as_str)
            .collect();
        assert_eq!(short_end_spans, vec!["\"time as this\""]);
    }

    #[test]
    fn phrase_spans_are_bounded_and_skip_short_input() {
        assert!(build_phrase_spans("only two").is_empty());
        assert!(
            build_phrase_spans(&"word ".repeat(60)).len() <= MAX_PHRASE_SPANS + MAX_INTERIOR_SPANS
        );
    }

    #[test]
    fn three_word_input_without_a_stripped_stop_does_not_enter_phrase_tier() {
        assert!(
            build_phrase_spans("ordinary sermon language").is_empty(),
            "three-word phrases are too collision-prone unless derived by stripping a leading stop word"
        );
    }

    #[test]
    fn phrase_spans_reach_verse_length_on_long_windows() {
        let spans = build_phrase_spans(
            "unless we are in our secret closet at home praying you will not stand the wiles of the devil",
        );
        assert!(
            spans.iter().any(|s| s == "\"wiles of the devil\""),
            "long window must still try a verse-sized span, got {spans:?}"
        );
    }

    #[test]
    fn phrase_spans_stay_bounded_and_ordered_longest_first() {
        let (spans, end_n) = build_phrase_spans_with_end_count(&"word ".repeat(60));
        assert!(
            spans.len() <= MAX_PHRASE_SPANS + MAX_INTERIOR_SPANS,
            "got {} spans",
            spans.len()
        );
        let lengths: Vec<usize> = spans
            .iter()
            .take(end_n)
            .map(|s| s.split_whitespace().count())
            .filter(|n| *n >= MIN_PHRASE_SPAN_TERMS)
            .collect();
        assert!(
            lengths.windows(2).all(|w| w[0] > w[1]),
            "primary end-anchored spans must be strictly longest-first, got {lengths:?}"
        );
        assert_eq!(*lengths.last().unwrap(), MIN_PHRASE_SPAN_TERMS);
    }

    #[test]
    fn phrase_spans_include_interior_windows() {
        let spans =
            build_phrase_spans("the three angels messages it is the everlasting gospel to preach");
        assert!(
            spans
                .iter()
                .any(|s| s == "\"the everlasting gospel to preach\""
                    || s == "\"everlasting gospel to preach\""
                    || s == "\"the everlasting gospel\""),
            "an interior or stripped quoted phrase must be reachable, got {spans:?}"
        );
    }

    #[test]
    fn interior_phrase_spans_require_six_words() {
        let (spans, end_n) = build_phrase_spans_with_end_count(
            "You know, the song writer says, Nothing between me and my savior.",
        );
        let interior = &spans[end_n..];

        assert!(
            interior
                .iter()
                .all(|span| span.split_whitespace().count() >= 6),
            "collision-prone short interior spans must not receive phrase evidence: {interior:?}"
        );
        assert!(
            !interior.iter().any(|span| span == "\"between me and my\""),
            "the hymn/Bible collision must not enter the phrase tier: {interior:?}"
        );
    }

    #[test]
    fn six_word_interior_span_keeps_mid_window_recall() {
        let (spans, end_n) = build_phrase_spans_with_end_count(
            "the three angels messages it is the everlasting gospel to preach unto them that dwell on the earth",
        );
        let interior = &spans[end_n..];

        assert!(
            interior
                .iter()
                .any(|span| span == "\"everlasting gospel to preach unto them\""),
            "the distinctive six-word Revelation span must remain reachable: {interior:?}"
        );
    }

    #[test]
    fn phrase_spans_reach_stripped_three_word_tail() {
        let spans = build_phrase_spans(
            "And so we have the final messages the three angels messages going out to the whole world. It is the everlasting gospel.",
        );
        assert!(
            spans.iter().any(|s| s == "\"the everlasting gospel\""),
            "stop-stripped three-word quote must be tried, got {spans:?}"
        );
    }

    #[test]
    fn dedup_keeps_strongest_bm25_per_verse() {
        // The same verse surfaces from multiple FTS tiers with different scores:
        // a weak phrase-tier hit first, then a strong AND-tier hit. Dedup must keep
        // the strongest (most negative) score so downstream relevance gating is accurate.
        let results = vec![
            bm25(-11.68, 43, 3, 16), // phrase tier (weak), seen first
            bm25(-24.99, 43, 3, 16), // AND tier (strong), seen later
            bm25(-8.0, 45, 5, 8),
        ];

        let deduped = dedup_results(results, 10);

        assert_eq!(deduped.len(), 2);
        let john = deduped
            .iter()
            .find(|r| r.book_number == 43)
            .expect("John 3:16 retained");
        assert!(
            (john.rank - (-24.99)).abs() < f64::EPSILON,
            "expected strongest score -24.99, got {}",
            john.rank
        );
    }

    #[test]
    fn dedup_preserves_first_seen_order_and_limit() {
        let results = vec![
            bm25(-5.0, 1, 1, 1),
            bm25(-9.0, 2, 2, 2),
            bm25(-30.0, 1, 1, 1), // stronger dup of first verse — must not reorder
            bm25(-3.0, 3, 3, 3),
        ];

        let deduped = dedup_results(results, 2);

        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].book_number, 1);
        assert!((deduped[0].rank - (-30.0)).abs() < f64::EPSILON);
        assert_eq!(deduped[1].book_number, 2);
    }

    #[test]
    fn dedup_preserves_strict_tier_when_broad_duplicate_has_stronger_rank() {
        let results = vec![
            bm25_with_broad_match(-12.0, 43, 3, 16, false),
            bm25_with_broad_match(-25.0, 43, 3, 16, true),
        ];

        let deduped = dedup_results(results, 10);

        assert_eq!(deduped.len(), 1);
        assert!((deduped[0].rank - (-25.0)).abs() < f64::EPSILON);
        assert!(!deduped[0].is_broad_match);
    }

    #[test]
    fn dedup_preserves_strict_tier_when_broad_duplicate_has_weaker_rank() {
        let results = vec![
            bm25_with_broad_match(-25.0, 43, 3, 16, true),
            bm25_with_broad_match(-12.0, 43, 3, 16, false),
        ];

        let deduped = dedup_results(results, 10);

        assert_eq!(deduped.len(), 1);
        assert!((deduped[0].rank - (-25.0)).abs() < f64::EPSILON);
        assert!(!deduped[0].is_broad_match);
    }

    #[test]
    fn bm25_ignores_locked_translation_text() {
        let db = fixture_db();

        let results = db
            .search_verses_bm25("Regters en opsigters moet jy vir jou aanstel", 10)
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn phrase_query_wraps_input() {
        assert_eq!(
            build_phrase_query("Follow peace with all men"),
            "\"Follow peace with all men\""
        );
    }

    #[test]
    fn phrase_query_strips_special_chars() {
        assert_eq!(
            build_phrase_query("God's love* NEAR/2"),
            "\"God s love NEAR 2\""
        );
    }

    #[test]
    fn phrase_query_empty() {
        assert_eq!(build_phrase_query(""), String::new());
    }

    #[test]
    fn and_query_filters_stop_words() {
        assert_eq!(build_and_query("be doers of the word"), "doers word");
    }

    #[test]
    fn and_query_filters_all_stop_words() {
        assert_eq!(build_and_query("I am a the"), String::new());
    }

    #[test]
    fn and_query_keeps_significant_words() {
        assert_eq!(
            build_and_query("for God so loved the world"),
            "God loved world"
        );
    }

    #[test]
    fn and_query_caps_at_12_terms() {
        let long_input = "God love peace faith hope joy spirit truth grace mercy light salvation prayer worship glory kingdom";
        let result = build_and_query(long_input);
        let term_count = result.split_whitespace().count();
        assert!(term_count <= 12);
    }

    #[test]
    fn or_query_filters_stop_words() {
        assert_eq!(
            build_or_query("It's a new creature Old things are passed away"),
            "\"new\" OR \"creature\" OR \"Old\" OR \"things\" OR \"passed\" OR \"away\""
        );
    }

    #[test]
    fn query_builders_strip_apostrophes_for_fts5_safety() {
        assert_eq!(build_and_query("praise ye don't"), "praise ye don");
        assert_eq!(
            build_or_query("praise the lord don't"),
            "\"praise\" OR \"lord\" OR \"don\""
        );
    }

    #[test]
    fn and_query_drops_reference_noise_tokens() {
        // Spoken citations wrap quotes in reference mechanics ("Verse 27.
        // Remember we read it? Verse 27. Therefore, O king…"). Digits never
        // appear in verse text and "verse"/"chapter" almost never do, so they
        // must not poison the AND query or eat the term cap.
        let query = build_and_query(
            "Verse 27 remember we read it verse 27 therefore O king let my counsel be acceptable unto thee break off your sins",
        );
        assert!(!query.contains("27"), "digits must be filtered: {query}");
        assert!(
            !query
                .to_lowercase()
                .split_whitespace()
                .any(|t| t == "verse"),
            "'verse' keyword must be filtered: {query}"
        );
        assert!(
            query.contains("counsel"),
            "content words must survive: {query}"
        );
        assert!(
            query.contains("acceptable"),
            "content words must survive: {query}"
        );
    }

    #[test]
    fn or_query_drops_reference_noise_tokens_and_duplicates() {
        let query = build_or_query(
            "Chapter 2 verse 37 the Bible says you O king are the king of kings for the God of heaven has given you a kingdom power strength and glory",
        );
        assert!(
            !query.contains("\"37\""),
            "digits must be filtered: {query}"
        );
        assert!(
            !query.contains("\"verse\""),
            "'verse' must be filtered: {query}"
        );
        assert!(
            !query.contains("\"chapter\""),
            "'chapter' must be filtered: {query}"
        );
        assert_eq!(
            query.matches("\"king\"").count(),
            1,
            "duplicate terms must not eat the term cap: {query}"
        );
        // With noise and duplicates gone, the 10-term cap reaches deep into
        // the quote instead of stalling on "chapter 2 verse 37 … verse verse".
        assert!(
            query.contains("\"kingdom\""),
            "content words must survive: {query}"
        );
        assert!(
            query.contains("\"strength\""),
            "content words must survive: {query}"
        );
    }

    #[test]
    fn bm25_results_carry_verse_text_for_quote_verification() {
        let db = fixture_db();

        let results = db
            .search_verses_bm25("Judges and officers shalt thou make thee", 10)
            .unwrap();

        assert!(!results.is_empty());
        assert_eq!(
            results[0].text,
            "Judges and officers shalt thou make thee in all thy gates."
        );
    }

    #[test]
    fn bm25_name_alias_recovers_kjv_noe_verse_for_spoken_noah() {
        let db = fixture_db();

        let results = db.search_verses_bm25("the days of Noah", 10).unwrap();

        assert!(
            results.iter().any(|result| {
                result.book_number == 40 && result.chapter == 24 && result.verse == 37
            }),
            "modern Noah query should retrieve KJV Matthew 24:37 via Noe alias"
        );
    }

    #[test]
    fn or_query_caps_at_12_terms() {
        let long_input =
            "God love peace faith hope joy spirit truth grace mercy light salvation prayer";
        let result = build_or_query(long_input);
        let term_count = result.matches(" OR ").count() + 1;
        assert!(term_count <= 12);
    }

    #[test]
    fn or_query_expands_modern_kjv_name_aliases() {
        let query = build_or_query("the days of Noah");
        assert!(
            query.contains("\"Noah\""),
            "modern name must remain: {query}"
        );
        assert!(
            query.contains("\"noe\""),
            "KJV alias must be added: {query}"
        );
    }

    #[test]
    fn or_query_does_not_expand_unlisted_words() {
        let query = build_or_query("the shepherd");
        assert!(!query.contains("\"noe\""));
    }

    #[test]
    fn or_query_expands_baptism_event_language() {
        let query = build_or_query("the verse where John the Baptist baptizes Jesus");
        assert!(
            query.contains("\"baptized\""),
            "baptism variants missing: {query}"
        );
        assert!(
            query.contains("\"baptism\""),
            "baptism concept missing: {query}"
        );
    }

    #[test]
    fn or_query_expands_nicodemus_born_again_language() {
        let query = build_or_query("where Jesus and Nicodemus talk about being born again");
        assert!(query.contains("\"born\""), "born concept missing: {query}");
        assert!(
            query.contains("\"again\""),
            "again concept missing: {query}"
        );
    }

    #[test]
    fn topic_phrase_queries_keep_modern_event_anchors() {
        assert_eq!(
            build_topic_phrase_queries("where Jesus and Nicodemus talk about being born again"),
            vec!["\"born again\""]
        );
        assert_eq!(
            build_topic_phrase_queries("John the Baptist baptizing Jesus"),
            vec!["baptized Jesus", "\"baptized\""]
        );
    }

    #[test]
    fn or_query_empty_on_all_stop_words() {
        assert_eq!(build_or_query("I am a the is"), String::new());
    }
}
