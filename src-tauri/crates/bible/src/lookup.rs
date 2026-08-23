use crate::db::BibleDb;
use crate::error::BibleError;
use crate::models::{Book, SearchVerse, Translation, Verse};

impl BibleDb {
    /// Look up a verse by its database primary key (verses.id).
    ///
    pub fn get_verse_by_id(&self, id: i64) -> Result<Option<Verse>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, translation_id, book_number, book_name, book_abbreviation, chapter, verse, text \
             FROM verses WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id], |row: &rusqlite::Row| {
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
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_verse_by_id_in_translation(
        &self,
        id: i64,
        translation_id: i64,
    ) -> Result<Option<Verse>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT target.id, target.translation_id, target.book_number, target.book_name, target.book_abbreviation, target.chapter, target.verse, target.text \
             FROM verses source \
             JOIN verses target \
               ON target.translation_id = ?2 \
              AND target.book_number = source.book_number \
              AND target.chapter = source.chapter \
              AND target.verse = source.verse \
             WHERE source.id = ?1",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id, translation_id], |row| {
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
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_verse(
        &self,
        translation_id: i64,
        book_number: i32,
        chapter: i32,
        verse: i32,
    ) -> Result<Option<Verse>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, translation_id, book_number, book_name, book_abbreviation, chapter, verse, text \
             FROM verses \
             WHERE translation_id = ?1 AND book_number = ?2 AND chapter = ?3 AND verse = ?4",
        )?;
        let mut rows = stmt.query_map(
            rusqlite::params![translation_id, book_number, chapter, verse],
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
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_chapter(
        &self,
        translation_id: i64,
        book_number: i32,
        chapter: i32,
    ) -> Result<Vec<Verse>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, translation_id, book_number, book_name, book_abbreviation, chapter, verse, text \
             FROM verses \
             WHERE translation_id = ?1 AND book_number = ?2 AND chapter = ?3 \
             ORDER BY verse",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![translation_id, book_number, chapter],
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

    pub fn get_verse_range(
        &self,
        translation_id: i64,
        book_number: i32,
        chapter: i32,
        verse_start: i32,
        verse_end: i32,
    ) -> Result<Vec<Verse>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, translation_id, book_number, book_name, book_abbreviation, chapter, verse, text \
             FROM verses \
             WHERE translation_id = ?1 AND book_number = ?2 AND chapter = ?3 \
               AND verse >= ?4 AND verse <= ?5 \
             ORDER BY verse",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![translation_id, book_number, chapter, verse_start, verse_end],
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

    /// Load all verses for one translation for client-side context search indexing.
    pub fn load_translation_verses_for_search(
        &self,
        translation_id: i64,
    ) -> Result<Vec<SearchVerse>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT book_number, book_name, chapter, verse, text \
             FROM verses \
             WHERE translation_id = ?1 \
             ORDER BY book_number, chapter, verse",
        )?;
        let rows = stmt.query_map([translation_id], |row: &rusqlite::Row| {
            Ok(SearchVerse {
                book_number: row.get(0)?,
                book_name: row.get(1)?,
                chapter: row.get(2)?,
                verse: row.get(3)?,
                text: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Whether the translation with this ID exists and is unlocked (not
    /// copyrighted and fully downloaded). Missing IDs count as locked.
    ///
    /// This is the single choke point for the copyright policy: the live BM25
    /// path already enforces `is_copyrighted = 0 AND is_downloaded = 1` in
    /// SQL (see `run_fts_query`), so direct lookups must not bypass it.
    pub fn translation_unlocked(&self, translation_id: i64) -> Result<bool, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT is_copyrighted, is_downloaded FROM translations WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![translation_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        Ok(matches!(
            rows.next(),
            Some(Ok((copyrighted, downloaded))) if copyrighted == 0 && downloaded == 1
        ))
    }

    pub fn list_translations(&self) -> Result<Vec<Translation>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, abbreviation, title, language, is_copyrighted, is_downloaded \
             FROM translations",
        )?;
        let rows = stmt.query_map([], |row: &rusqlite::Row| {
            Ok(Translation {
                id: row.get(0)?,
                abbreviation: row.get(1)?,
                title: row.get(2)?,
                language: row.get(3)?,
                is_copyrighted: row.get(4)?,
                is_downloaded: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_books(&self, translation_id: i64) -> Result<Vec<Book>, BibleError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, translation_id, book_number, name, abbreviation, testament \
             FROM books \
             WHERE translation_id = ?1 \
             ORDER BY book_number",
        )?;
        let rows = stmt.query_map(rusqlite::params![translation_id], |row: &rusqlite::Row| {
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
mod lookup_tests {
    use super::BibleDb;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn fixture_db() -> BibleDb {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE translations (id INTEGER PRIMARY KEY, abbreviation TEXT, title TEXT, language TEXT, is_copyrighted INTEGER, is_downloaded INTEGER);
             CREATE TABLE books (id INTEGER PRIMARY KEY, translation_id INTEGER, book_number INTEGER, name TEXT, abbreviation TEXT, testament TEXT);
             CREATE TABLE verses (id INTEGER PRIMARY KEY, translation_id INTEGER, book_number INTEGER, book_name TEXT, book_abbreviation TEXT, chapter INTEGER, verse INTEGER, text TEXT);
             INSERT INTO translations VALUES
               (1, 'KJV', 'King James', 'en', 0, 1),
               (2, 'NKJV', 'New King James', 'en', 1, 1);
             INSERT INTO books VALUES (1, 1, 43, 'John', 'Jn', 'NT');
             INSERT INTO verses VALUES
               (1, 1, 43, 'John', 'Jn', 3, 16, 'For God so loved the world.'),
               (2, 1, 43, 'John', 'Jn', 3, 17, 'For God sent not his Son.'),
               (3, 2, 43, 'John', 'Jn', 3, 16, 'Other translation verse.');",
        )
        .unwrap();
        BibleDb {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn get_verse_returns_matching_row() {
        let db = fixture_db();
        let verse = db.get_verse(1, 43, 3, 16).unwrap().expect("verse");
        assert_eq!(verse.text, "For God so loved the world.");
    }

    #[test]
    fn get_verse_by_id_in_translation_returns_matching_reference() {
        let db = fixture_db();
        let verse = db
            .get_verse_by_id_in_translation(1, 2)
            .unwrap()
            .expect("verse");
        assert_eq!(verse.translation_id, 2);
        assert_eq!(verse.text, "Other translation verse.");
    }

    #[test]
    fn get_verse_by_id_in_translation_returns_none_when_translation_missing_reference() {
        let db = fixture_db();
        assert!(db.get_verse_by_id_in_translation(2, 2).unwrap().is_none());
    }

    #[test]
    fn get_verse_returns_none_for_missing_verse() {
        let db = fixture_db();
        assert!(db.get_verse(1, 43, 3, 99).unwrap().is_none());
    }

    #[test]
    fn get_chapter_orders_verses() {
        let db = fixture_db();
        let chapter = db.get_chapter(1, 43, 3).unwrap();
        assert_eq!(chapter.len(), 2);
        assert_eq!(chapter[0].verse, 16);
        assert_eq!(chapter[1].verse, 17);
    }

    #[test]
    fn get_verse_range_is_inclusive() {
        let db = fixture_db();
        let range = db.get_verse_range(1, 43, 3, 16, 17).unwrap();
        assert_eq!(range.len(), 2);
        assert_eq!(range[0].verse, 16);
        assert_eq!(range[1].verse, 17);
    }

    #[test]
    fn load_translation_verses_for_search_filters_by_translation() {
        let db = fixture_db();
        let rows = db.load_translation_verses_for_search(1).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.book_name == "John"));
    }

    #[test]
    fn list_books_returns_ordered_rows() {
        let db = fixture_db();
        let books = db.list_books(1).unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].book_number, 43);
    }
}
