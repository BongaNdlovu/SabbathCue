//! Property-based tests for Bible-reference parsing and translation
//! command false positives.
//!
//! These exercise the production parser and detector, not a shadow grammar.

use proptest::prelude::*;
use rhema_detection::direct::automaton::BookMatcher;
use rhema_detection::direct::books::BOOKS;
use rhema_detection::direct::parser::parse_reference;
use rhema_detection::DirectDetector;

fn book_indices() -> impl Strategy<Value = usize> {
    0..BOOKS.len()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 96,
        ..ProptestConfig::default()
    })]

    #[test]
    fn colon_form_parses_to_the_spoken_chapter_and_verse(
        book_idx in book_indices(),
        chapter in 1i32..=150,
        verse in 1i32..=176,
    ) {
        let book = &BOOKS[book_idx];
        let text = format!("{} {chapter}:{verse}", book.name);
        let matcher = BookMatcher::new();
        let matches = matcher.find_books(&text);
        prop_assert!(
            !matches.is_empty(),
            "canonical name must match: {text}"
        );
        let intended = matches
            .iter()
            .find(|found| found.book_number == book.number)
            .unwrap_or(&matches[0]);
        let parsed = parse_reference(&text, intended)
            .expect("complete colon reference must parse");
        prop_assert_eq!(parsed.book_number, book.number);
        prop_assert_eq!(parsed.chapter, chapter);
        prop_assert_eq!(parsed.verse_start, verse);
        prop_assert!(parsed.verse_end.is_none());
    }

    #[test]
    fn spoken_chapter_verse_form_parses_the_same_coordinates(
        book_idx in book_indices(),
        chapter in 1i32..=150,
        verse in 1i32..=176,
    ) {
        let book = &BOOKS[book_idx];
        let text = format!("{} chapter {chapter} verse {verse}", book.name);
        let matcher = BookMatcher::new();
        let matches = matcher.find_books(&text);
        prop_assume!(!matches.is_empty());
        let intended = matches
            .iter()
            .find(|found| found.book_number == book.number)
            .unwrap_or(&matches[0]);
        let parsed = parse_reference(&text, intended)
            .expect("spoken chapter/verse must parse");
        prop_assert_eq!(parsed.book_number, book.number);
        prop_assert_eq!(parsed.chapter, chapter);
        prop_assert_eq!(parsed.verse_start, verse);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 80,
        ..ProptestConfig::default()
    })]

    #[test]
    fn sermon_prose_with_version_words_is_not_a_translation_command(
        topic in "[A-Za-z]{4,12}",
        people in "[A-Za-z]{4,10}",
    ) {
        let detector = DirectDetector::new();
        let prose = [
            format!("the net result of {topic} is a blessing"),
            format!("we have faith in the message of {topic}"),
            format!("good news for {people} this sabbath"),
            format!("king james was a monarch who commissioned {topic}"),
            format!("english is the language of this {topic}"),
            format!("share the good news with {people} today"),
        ];
        for sentence in prose {
            prop_assert!(
                detector.detect_translation_command(&sentence).is_none(),
                "narration must not switch translations: {sentence}"
            );
        }
    }
}

#[test]
fn documented_translation_commands_still_switch() {
    let detector = DirectDetector::new();
    for (phrase, abbrev) in [
        ("give me niv", "NIV"),
        ("switch to esv", "ESV"),
        ("read in kjv", "KJV"),
        ("new international version", "NIV"),
        ("king james version", "KJV"),
    ] {
        assert_eq!(
            detector.detect_translation_command(phrase).as_deref(),
            Some(abbrev),
            "{phrase}"
        );
    }
}
