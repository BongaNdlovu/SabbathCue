//! Recall regressions: does the correct verse enter the candidate pool at all?
//! Distilled from live sermon logs where DeepSeek ranked correctly but the
//! right verse was never retrieved. Unlike the pipeline tests, these go
//! through real SQLite FTS5.

use rhema_bible::BibleDb;

fn recall_db() -> BibleDb {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE translations (id INTEGER PRIMARY KEY, abbreviation TEXT, title TEXT, language TEXT, is_copyrighted INTEGER, is_downloaded INTEGER);
         CREATE TABLE verses (id INTEGER PRIMARY KEY, translation_id INTEGER, book_number INTEGER, book_name TEXT, book_abbreviation TEXT, chapter INTEGER, verse INTEGER, text TEXT);
         CREATE VIRTUAL TABLE verses_fts USING fts5(text, content='verses', content_rowid='id', tokenize='unicode61');
         INSERT INTO translations VALUES (1, 'KJV', 'King James', 'en', 0, 1);",
    )
    .unwrap();

    let verses: [(i64, i32, &str, i32, i32, &str); 6] = [
        (1, 17, "Esther", 4, 14, "For if thou altogether holdest thy peace at this time, then shall there enlargement and deliverance arise to the Jews from another place; but thou and thy father's house shall be destroyed: and who knoweth whether thou art come to the kingdom for such a time as this?"),
        (2, 30, "Amos", 5, 13, "Therefore the prudent shall keep silence in that time; for it is an evil time."),
        (3, 39, "Malachi", 1, 1, "The burden of the word of the LORD to Israel by Malachi."),
        (4, 41, "Mark", 4, 39, "And he arose, and rebuked the wind, and said unto the sea, Peace, be still. And the wind ceased, and there was a great calm."),
        (5, 53, "2 Thessalonians", 2, 3, "Let no man deceive you by any means: for that day shall not come, except there come a falling away first, and that man of sin be revealed, the son of perdition;"),
        (6, 44, "Acts", 16, 25, "And at midnight Paul and Silas prayed, and sang praises unto God: and the prisoners heard them."),
    ];
    for (id, book_number, book_name, chapter, verse, text) in verses {
        conn.execute(
            "INSERT INTO verses VALUES (?1, 1, ?2, ?3, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, book_number, book_name, chapter, verse, text],
        )
        .unwrap();
    }
    conn.execute_batch("INSERT INTO verses_fts(verses_fts) VALUES('rebuild');")
        .unwrap();
    BibleDb::from_connection(conn)
}

fn assert_recalls(db: &BibleDb, query: &str, book: &str, chapter: i32, verse: i32) {
    let results = db.search_verses_bm25(query, 10).unwrap();
    assert!(
        results
            .iter()
            .any(|r| r.book_name == book && r.chapter == chapter && r.verse == verse),
        "expected {book} {chapter}:{verse} in pool for {query:?}, got {:?}",
        results
            .iter()
            .map(|r| format!("{} {}:{}", r.book_name, r.chapter, r.verse))
            .collect::<Vec<_>>()
    );
}

#[test]
fn verbatim_quote_that_fills_the_window_is_recalled() {
    let db = recall_db();
    assert_recalls(
        &db,
        "And at midnight Paul and Silas prayed and sang praises unto God",
        "Acts",
        16,
        25,
    );
}

#[test]
fn short_quoted_fragment_inside_prose_is_recalled() {
    let db = recall_db();
    // Live log 2026-07-31 19:49: this window returned Amos 5:13, never Esther.
    assert_recalls(
        &db,
        "Malachi is speaking to Esther and he's saying maybe it was for such a time as this",
        "Esther",
        4,
        14,
    );
}

#[test]
fn book_hint_scopes_the_pool_to_the_named_book() {
    let db = recall_db();
    let results = db
        .search_verses_bm25_scoped("for such a time as this", 10, Some(17))
        .unwrap();
    assert!(
        results.iter().all(|r| r.book_number == 17),
        "book hint must exclude other books, got {:?}",
        results
            .iter()
            .map(|r| r.book_name.clone())
            .collect::<Vec<_>>()
    );
    assert!(results.iter().any(|r| r.chapter == 4 && r.verse == 14));
}

#[test]
fn absent_book_hint_leaves_the_pool_unscoped() {
    let db = recall_db();
    let hinted = db
        .search_verses_bm25_scoped("peace be still", 10, None)
        .unwrap();
    let plain = db.search_verses_bm25("peace be still", 10).unwrap();
    assert_eq!(hinted.len(), plain.len());
}
