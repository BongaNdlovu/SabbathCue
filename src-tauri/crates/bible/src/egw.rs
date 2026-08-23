use crate::db::BibleDb;
use crate::error::BibleError;
use crate::models::{EgwBook, EgwChapterInfo, EgwPageInfo, EgwParagraph};

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

impl BibleDb {
    /// List all EGW books, ordered by book number.
    ///
    pub fn list_egw_books(&self) -> Result<Vec<EgwBook>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, book_number, title, abbreviation, chapter_count \
             FROM egw_books b \
             WHERE EXISTS ( \
               SELECT 1 FROM egw_paragraphs p \
               WHERE p.book_number = b.book_number \
                 AND p.page > 0 \
                 AND p.page_paragraph > 0 \
             ) \
             ORDER BY book_number",
        )?;
        let rows = stmt.query_map([], |row: &rusqlite::Row| {
            Ok(EgwBook {
                id: row.get(0)?,
                book_number: row.get(1)?,
                title: row.get(2)?,
                abbreviation: row.get(3)?,
                chapter_count: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// List chapters (with titles and paragraph counts) for one EGW book.
    /// Legacy callers only receive paragraphs that can resolve to printed pages.
    pub fn list_egw_chapters(&self, book_number: i32) -> Result<Vec<EgwChapterInfo>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT chapter, chapter_title, COUNT(*) AS paragraph_count \
             FROM egw_paragraphs \
             WHERE book_number = ?1 AND page > 0 AND page_paragraph > 0 \
             GROUP BY chapter, chapter_title \
             ORDER BY chapter",
        )?;
        let rows = stmt.query_map(rusqlite::params![book_number], |row: &rusqlite::Row| {
            Ok(EgwChapterInfo {
                chapter: row.get(0)?,
                title: row.get(1)?,
                paragraph_count: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// List printed pages (with paragraph counts) for one EGW book.
    pub fn list_egw_pages(&self, book_number: i32) -> Result<Vec<EgwPageInfo>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT page, COUNT(*) AS paragraph_count \
             FROM egw_paragraphs \
             WHERE book_number = ?1 AND page > 0 AND page_paragraph > 0 \
             GROUP BY page \
             ORDER BY page",
        )?;
        let rows = stmt.query_map(rusqlite::params![book_number], |row: &rusqlite::Row| {
            Ok(EgwPageInfo {
                page: row.get(0)?,
                paragraph_count: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Get every paragraph in a chapter, ordered by paragraph number.
    /// Legacy callers only receive paragraphs that can resolve to printed pages.
    pub fn get_egw_chapter(
        &self,
        book_number: i32,
        chapter: i32,
    ) -> Result<Vec<EgwParagraph>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text \
             FROM egw_paragraphs \
             WHERE book_number = ?1 AND chapter = ?2 AND page > 0 AND page_paragraph > 0 \
             ORDER BY paragraph",
        )?;
        let rows = stmt.query_map(rusqlite::params![book_number, chapter], map_egw_paragraph)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Get every paragraph on a printed page, ordered by page paragraph number.
    pub fn get_egw_page(
        &self,
        book_number: i32,
        page: i32,
    ) -> Result<Vec<EgwParagraph>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text \
             FROM egw_paragraphs \
             WHERE book_number = ?1 AND page = ?2 AND page > 0 AND page_paragraph > 0 \
             ORDER BY page_paragraph",
        )?;
        let rows = stmt.query_map(rusqlite::params![book_number, page], map_egw_paragraph)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Get a single paragraph by (book, chapter, paragraph).
    /// Legacy callers only receive paragraphs that can resolve to printed pages.
    pub fn get_egw_paragraph(
        &self,
        book_number: i32,
        chapter: i32,
        paragraph: i32,
    ) -> Result<Option<EgwParagraph>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text \
             FROM egw_paragraphs \
             WHERE book_number = ?1 AND chapter = ?2 AND paragraph = ?3 AND page > 0 AND page_paragraph > 0",
        )?;
        let mut rows = stmt.query_map(
            rusqlite::params![book_number, chapter, paragraph],
            map_egw_paragraph,
        )?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Get a single paragraph by (book, printed page, page paragraph).
    pub fn get_egw_paragraph_by_page(
        &self,
        book_number: i32,
        page: i32,
        page_paragraph: i32,
    ) -> Result<Option<EgwParagraph>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text \
             FROM egw_paragraphs \
             WHERE book_number = ?1 AND page = ?2 AND page_paragraph = ?3 AND page > 0 AND page_paragraph > 0",
        )?;
        let mut rows = stmt.query_map(
            rusqlite::params![book_number, page, page_paragraph],
            map_egw_paragraph,
        )?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// All EGW paragraphs as `(id, text)` pairs, for embedding-index builds.
    /// Returns an empty list when no EGW content has been imported.
    pub fn list_egw_paragraph_texts(&self) -> Result<Vec<(i64, String)>, BibleError> {
        let conn = self.conn()?;
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='egw_paragraphs'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !table_exists {
            return Ok(vec![]);
        }
        let mut stmt = conn.prepare(
            "SELECT id, text FROM egw_paragraphs \
             WHERE page > 0 AND page_paragraph > 0 \
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Cheap content fingerprint for the EGW corpus: (row count, sum of ids,
    /// stable text hash) over page-resolved paragraphs.
    pub fn egw_content_fingerprint(&self) -> Result<(i64, i64, u64), BibleError> {
        let conn = self.conn()?;
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='egw_paragraphs'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !table_exists {
            return Ok((0, 0, 0));
        }
        let mut stmt = conn.prepare(
            "SELECT id, text FROM egw_paragraphs \
             WHERE page > 0 AND page_paragraph > 0 \
             ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        let mut count = 0_i64;
        let mut id_sum = 0_i64;
        let mut text_hash = FNV_OFFSET_BASIS;

        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let text: String = row.get(1)?;
            count += 1;
            id_sum += id;
            text_hash = fnv1a_update(text_hash, &id.to_le_bytes());
            text_hash = fnv1a_update(text_hash, b"\0");
            text_hash = fnv1a_update(text_hash, text.as_bytes());
            text_hash = fnv1a_update(text_hash, b"\xff");
        }

        if count == 0 {
            text_hash = 0;
        }

        Ok((count, id_sum, text_hash))
    }

    /// Get a single paragraph by its row id.
    pub fn get_egw_paragraph_by_id(&self, id: i64) -> Result<Option<EgwParagraph>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text \
             FROM egw_paragraphs WHERE id = ?1 AND page > 0 AND page_paragraph > 0",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id], map_egw_paragraph)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Full-text keyword search of EGW paragraphs via FTS5 (all terms, any order).
    pub fn search_egw(&self, query: &str, limit: usize) -> Result<Vec<EgwParagraph>, BibleError> {
        let conn = self.conn()?;

        // If no EGW content has been imported, the FTS table won't exist yet.
        let fts_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='egw_paragraphs_fts'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !fts_exists {
            return Ok(vec![]);
        }

        // Keyword search: each significant token must be present (FTS5 implicit
        // AND), in any order — not an exact phrase.
        let tokens: Vec<String> = crate::search::query_terms(query)
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{w}\""))
            .collect();
        if tokens.is_empty() {
            return Ok(vec![]);
        }
        let fts_query = tokens.join(" ");

        #[expect(
            clippy::cast_possible_wrap,
            reason = "limit is a small page-size value that fits in i64"
        )]
        let limit_i64 = limit as i64;

        let mut stmt = conn.prepare(
            "SELECT p.id, p.book_number, p.book_title, p.chapter, p.chapter_title, p.paragraph, p.page, p.page_paragraph, p.text \
             FROM egw_paragraphs_fts fts \
             JOIN egw_paragraphs p ON p.id = fts.rowid \
             WHERE fts.text MATCH ?1 AND p.page > 0 AND p.page_paragraph > 0 \
             ORDER BY bm25(egw_paragraphs_fts) \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![fts_query, limit_i64], map_egw_paragraph)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// BM25-ranked keyword search over EGW paragraphs for live transcript matching.
    ///
    /// Mirrors `search_verses_bm25`: phrase, AND, then OR tiers, reusing the
    /// same query builders. Unlike `search_egw` (implicit AND, for the search
    /// box), the OR tier lets a partial transcript window match a paragraph.
    pub fn search_egw_bm25(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<EgwParagraph>, BibleError> {
        let conn = self.conn()?;

        let fts_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='egw_paragraphs_fts'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !fts_exists {
            return Ok(vec![]);
        }

        #[expect(
            clippy::cast_possible_wrap,
            reason = "limit is a small page-size value that fits in i64"
        )]
        let limit_i64 = limit as i64;

        let mut results: Vec<EgwParagraph> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for fts_query in [
            crate::search::build_phrase_query(query),
            crate::search::build_and_query(query),
            crate::search::build_or_query(query),
        ] {
            if fts_query.is_empty() || results.len() >= limit {
                continue;
            }
            let mut stmt = conn.prepare(
                "SELECT p.id, p.book_number, p.book_title, p.chapter, p.chapter_title, p.paragraph, p.page, p.page_paragraph, p.text \
                 FROM egw_paragraphs_fts fts \
                 JOIN egw_paragraphs p ON p.id = fts.rowid \
                 WHERE fts.text MATCH ?1 AND p.page > 0 AND p.page_paragraph > 0 \
                 ORDER BY bm25(egw_paragraphs_fts) \
                 LIMIT ?2",
            )?;
            let rows =
                stmt.query_map(rusqlite::params![fts_query, limit_i64], map_egw_paragraph)?;
            for row in rows {
                let paragraph = row?;
                if seen.insert(paragraph.id) {
                    results.push(paragraph);
                }
            }
        }

        results.truncate(limit);
        Ok(results)
    }
}

fn map_egw_paragraph(row: &rusqlite::Row) -> rusqlite::Result<EgwParagraph> {
    Ok(EgwParagraph {
        id: row.get(0)?,
        book_number: row.get(1)?,
        book_title: row.get(2)?,
        chapter: row.get(3)?,
        chapter_title: row.get(4)?,
        paragraph: row.get(5)?,
        page: row.get(6)?,
        page_paragraph: row.get(7)?,
        text: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn test_db() -> BibleDb {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE egw_books (id INTEGER PRIMARY KEY AUTOINCREMENT, book_number INTEGER NOT NULL UNIQUE, title TEXT NOT NULL, abbreviation TEXT NOT NULL, chapter_count INTEGER NOT NULL DEFAULT 0);\
             CREATE TABLE egw_paragraphs (id INTEGER PRIMARY KEY AUTOINCREMENT, book_id INTEGER NOT NULL, book_number INTEGER NOT NULL, book_title TEXT NOT NULL, chapter INTEGER NOT NULL, chapter_title TEXT NOT NULL, paragraph INTEGER NOT NULL, page INTEGER NOT NULL, page_paragraph INTEGER NOT NULL, text TEXT NOT NULL);\
             INSERT INTO egw_books (book_number, title, abbreviation, chapter_count) VALUES (1, 'Patriarchs and Prophets', 'PP', 1);\
             INSERT INTO egw_books (book_number, title, abbreviation, chapter_count) VALUES (2, 'Hidden Legacy Book', 'HLB', 1);\
             INSERT INTO egw_paragraphs (book_id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text) VALUES (1, 1, 'Patriarchs and Prophets', 1, 'Why Was Sin Permitted?', 1, 29, 1, 'God is love.');\
             INSERT INTO egw_paragraphs (book_id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text) VALUES (1, 1, 'Patriarchs and Prophets', 1, 'Why Was Sin Permitted?', 2, 29, 2, 'The history of the great conflict.');\
             INSERT INTO egw_paragraphs (book_id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text) VALUES (2, 2, 'Hidden Legacy Book', 1, 'Legacy', 1, 0, 0, 'This unresolved EGW paragraph should stay hidden.');             INSERT INTO egw_paragraphs (book_id, book_number, book_title, chapter, chapter_title, paragraph, page, page_paragraph, text) VALUES (2, 2, 'Hidden Legacy Book', 1, 'Legacy', 2, 30, 0, 'Page-resolved but paragraph-unresolved row must also stay hidden.');\
             CREATE VIRTUAL TABLE egw_paragraphs_fts USING fts5(text, content='egw_paragraphs', content_rowid='id', tokenize='unicode61');\
             INSERT INTO egw_paragraphs_fts(rowid, text) SELECT id, text FROM egw_paragraphs;",
        )
        .unwrap();
        BibleDb {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn lists_books() {
        let db = test_db();
        let books = db.list_egw_books().unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Patriarchs and Prophets");
    }

    #[test]
    fn hides_books_without_page_resolved_paragraphs() {
        let db = test_db();
        let books = db.list_egw_books().unwrap();
        assert!(books.iter().all(|book| book.title != "Hidden Legacy Book"));
        assert!(db.search_egw("unresolved hidden", 10).unwrap().is_empty());
        assert!(db.get_egw_paragraph_by_id(3).unwrap().is_none());
    }

    #[test]
    fn gets_chapter_paragraphs() {
        let db = test_db();
        let paras = db.get_egw_chapter(1, 1).unwrap();
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].paragraph, 1);
    }

    #[test]
    fn searches_paragraphs() {
        let db = test_db();
        let hits = db.search_egw("conflict", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].paragraph, 2);
    }

    #[test]
    fn gets_page_paragraphs() {
        let db = test_db();
        let paras = db.get_egw_page(1, 29).unwrap();
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[1].page_paragraph, 2);
    }

    #[test]
    fn page_lookup_hides_zero_page_paragraph_rows() {
        // A row with page > 0 but page_paragraph = 0 cannot resolve to a
        // printed paragraph; every sibling query hides such rows and this
        // accessor must too (it previously returned them).
        let db = test_db();
        assert!(db.get_egw_paragraph_by_page(2, 30, 0).unwrap().is_none());
    }

    #[test]
    fn searches_paragraphs_with_apostrophes_without_fts_syntax_error() {
        let db = test_db();
        let hits = db.search_egw_bm25("history don't conflict", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn keyword_search_is_order_independent() {
        let db = test_db();
        // "great conflict" appears as "the great conflict"; reversed order must still match.
        let hits = db.search_egw("conflict great", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].paragraph, 2);
    }

    #[test]
    fn bm25_search_matches_partial_transcript_window() {
        let db = test_db();
        let hits = db
            .search_egw_bm25("tonight we consider the great conflict story", 5)
            .unwrap();
        assert!(hits.iter().any(|p| p.paragraph == 2));
    }

    #[test]
    fn lists_chapters_with_counts() {
        let db = test_db();
        let chapters = db.list_egw_chapters(1).unwrap();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].chapter, 1);
        assert_eq!(chapters[0].paragraph_count, 2);
    }

    #[test]
    fn lists_pages_with_counts() {
        let db = test_db();
        let pages = db.list_egw_pages(1).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page, 29);
        assert_eq!(pages[0].paragraph_count, 2);
    }

    #[test]
    fn gets_single_paragraph_or_none() {
        let db = test_db();
        let p = db.get_egw_paragraph(1, 1, 2).unwrap();
        assert_eq!(p.unwrap().text, "The history of the great conflict.");
        assert!(db.get_egw_paragraph(1, 1, 99).unwrap().is_none());
    }

    #[test]
    fn gets_single_page_paragraph_or_none() {
        let db = test_db();
        let p = db.get_egw_paragraph_by_page(1, 29, 2).unwrap();
        assert_eq!(p.unwrap().text, "The history of the great conflict.");
        assert!(db.get_egw_paragraph_by_page(1, 29, 99).unwrap().is_none());
    }

    #[test]
    fn lists_all_paragraph_texts_for_embedding() {
        let db = test_db();
        let texts = db.list_egw_paragraph_texts().unwrap();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0], (1, "God is love.".to_string()));
    }

    #[test]
    fn gets_paragraph_by_row_id() {
        let db = test_db();
        let p = db.get_egw_paragraph_by_id(2).unwrap().unwrap();
        assert_eq!(p.text, "The history of the great conflict.");
        assert!(db.get_egw_paragraph_by_id(99).unwrap().is_none());
    }

    #[test]
    fn empty_query_returns_no_results() {
        let db = test_db();
        assert!(db.search_egw("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn fingerprint_counts_only_page_resolved_paragraphs() {
        let db = test_db();
        // test_db has 2 page-resolved paragraphs (ids 1, 2) and 1 hidden (id 3).
        let (count, id_sum, text_hash) = db.egw_content_fingerprint().unwrap();
        assert_eq!((count, id_sum), (2, 3));
        assert_ne!(text_hash, 0);
    }

    #[test]
    fn fingerprint_changes_when_paragraph_text_changes() {
        let db = test_db();
        let before = db.egw_content_fingerprint().unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "UPDATE egw_paragraphs SET text = ?1 WHERE id = 2",
                ["The repaired history of the great conflict."],
            )
            .unwrap();
        }
        let after = db.egw_content_fingerprint().unwrap();

        assert_eq!(before.0, after.0);
        assert_eq!(before.1, after.1);
        assert_ne!(before.2, after.2);
    }
}
