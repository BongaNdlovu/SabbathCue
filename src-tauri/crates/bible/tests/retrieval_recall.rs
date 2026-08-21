//! Recall regressions: does the correct verse enter the candidate pool at all?
//! Distilled from live sermon logs where `DeepSeek` ranked correctly but the
//! right verse was never retrieved. Unlike the pipeline tests, these go
//! through real `SQLite` FTS5.

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

    let verses: [(i64, i32, &str, i32, i32, &str); 18] = [
        (1, 17, "Esther", 4, 14, "For if thou altogether holdest thy peace at this time, then shall there enlargement and deliverance arise to the Jews from another place; but thou and thy father's house shall be destroyed: and who knoweth whether thou art come to the kingdom for such a time as this?"),
        (2, 30, "Amos", 5, 13, "Therefore the prudent shall keep silence in that time; for it is an evil time."),
        (3, 39, "Malachi", 1, 1, "The burden of the word of the LORD to Israel by Malachi."),
        (4, 41, "Mark", 4, 39, "And he arose, and rebuked the wind, and said unto the sea, Peace, be still. And the wind ceased, and there was a great calm."),
        (5, 53, "2 Thessalonians", 2, 3, "Let no man deceive you by any means: for that day shall not come, except there come a falling away first, and that man of sin be revealed, the son of perdition;"),
        (6, 44, "Acts", 16, 25, "And at midnight Paul and Silas prayed, and sang praises unto God: and the prisoners heard them."),
        (7, 49, "Ephesians", 6, 11, "Put on the whole armour of God, that ye may be able to stand against the wiles of the devil."),
        (8, 66, "Revelation", 14, 6, "And I saw another angel fly in the midst of heaven, having the everlasting gospel to preach unto them that dwell on the earth, and to every nation, and kindred, and tongue, and people,"),
        (9, 51, "Colossians", 1, 27, "To whom God would make known what is the riches of the glory of this mystery among the Gentiles; which is Christ in you, the hope of glory:"),
        (10, 66, "Revelation", 13, 8, "And all that dwell upon the earth shall worship him, whose names are not written in the book of life of the Lamb slain from the foundation of the world."),
        (11, 40, "Matthew", 3, 13, "Then cometh Jesus from Galilee to Jordan unto John, to be baptized of him."),
        (12, 43, "John", 3, 3, "Jesus answered and said unto him, Verily, verily, I say unto thee, Except a man be born again, he cannot see the kingdom of God."),
        (13, 43, "John", 11, 43, "And when he thus had spoken, he cried with a loud voice, Lazarus, come forth."),
        (14, 1, "Genesis", 37, 24, "And they took him, and cast him into a pit: and the pit was empty, there was no water in it."),
        (15, 66, "Revelation", 13, 16, "And he causeth all, both small and great, rich and poor, free and bond, to receive a mark in their right hand, or in their foreheads:"),
        (16, 66, "Revelation", 13, 17, "And that no man might buy or sell, save he that had the mark, or the name of the beast, or the number of his name."),
        (17, 40, "Matthew", 14, 25, "And in the fourth watch of the night Jesus went unto them, walking on the sea."),
        (18, 43, "John", 6, 19, "So when they had rowed about five and twenty or thirty furlongs, they see Jesus walking on the sea, and drawing nigh unto the ship: and they were afraid."),
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
fn modern_storm_and_boat_request_recalls_the_kjv_calm() {
    let db = recall_db();
    assert_recalls(
        &db,
        "Please show the verse that talks about Jesus coming the storm in the boat",
        "Mark",
        4,
        39,
    );
}

#[test]
fn prison_scene_request_recalls_paul_and_silas_praying() {
    let db = recall_db();
    assert_recalls(&db, "Paul and Silas in prison", "Acts", 16, 25);
    assert_recalls(
        &db,
        "And then let's go to the verse that talks about Paul and Silas singing in a prison.",
        "Acts",
        16,
        25,
    );
}

#[test]
fn joseph_pit_request_recalls_genesis_37_24() {
    let db = recall_db();
    assert_recalls(&db, "Joseph thrown into a well", "Genesis", 37, 24);
    assert_recalls(&db, "Joseph thrown into a pit", "Genesis", 37, 24);
}

#[test]
fn mark_of_the_beast_request_recalls_revelation_13() {
    let db = recall_db();
    assert_recalls(&db, "mark of the beast", "Revelation", 13, 16);
}

#[test]
fn jesus_walking_on_water_recalls_the_gospel_scene() {
    let db = recall_db();
    let results = db
        .search_verses_bm25("Jesus walking on water", 10)
        .unwrap();
    assert!(
        results.iter().any(|r| {
            (r.book_name == "Matthew" && r.chapter == 14 && r.verse == 25)
                || (r.book_name == "John" && r.chapter == 6 && r.verse == 19)
        }),
        "expected a walking-on-water Gospel verse, got {:?}",
        results
            .iter()
            .map(|r| format!("{} {}:{}", r.book_name, r.chapter, r.verse))
            .collect::<Vec<_>>()
    );
}

#[test]
fn lazarus_command_request_recalls_the_resurrection_scene() {
    let db = recall_db();
    assert_recalls(
        &db,
        "Then Jesus with a loud voice said Lazarus come out",
        "John",
        11,
        43,
    );
}

#[test]
fn baptism_request_recalls_jesus_baptized_by_john() {
    let db = recall_db();
    assert_recalls(
        &db,
        "the verse where John the Baptist baptizes Jesus",
        "Matthew",
        3,
        13,
    );
}

#[test]
fn born_again_request_recalls_nicodemus_anchor() {
    let db = recall_db();
    assert_recalls(
        &db,
        "where Jesus and Nicodemus talk about being born again",
        "John",
        3,
        3,
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

#[test]
fn quoted_fragment_survives_a_long_prose_window() {
    let db = recall_db();
    assert_recalls(
        &db,
        "unless we are in our secret closet at home praying, you will not stand the wiles of the devil",
        "Ephesians",
        6,
        11,
    );
}

#[test]
fn mid_window_everlasting_gospel_is_recalled() {
    let db = recall_db();
    assert_recalls(
        &db,
        "And so we have the final messages, the three angels messages going out to the whole world. It is the everlasting gospel.",
        "Revelation",
        14,
        6,
    );
}

#[test]
fn hope_of_glory_fragment_is_recalled() {
    let db = recall_db();
    assert_recalls(
        &db,
        "We need to spend time pointing men and women to Jesus Christ, the hope of glory.",
        "Colossians",
        1,
        27,
    );
}

#[test]
fn lamb_slain_from_foundation_is_recalled() {
    let db = recall_db();
    assert_recalls(
        &db,
        "I want that blood. The lamb slain from the foundation of the world.",
        "Revelation",
        13,
        8,
    );
}
