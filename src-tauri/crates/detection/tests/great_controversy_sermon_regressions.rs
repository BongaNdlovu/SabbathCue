//! Regressions distilled from the supplied Great Controversy live sermon.

use rhema_detection::DirectDetector;

const LIVE_THRESHOLD: f64 = 0.90;

#[test]
fn dotted_first_ordinal_reference_reaches_live_threshold() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect("1st Peter 3.15");
    let reference = detections
        .iter()
        .find(|detection| {
            detection.verse_ref.book_name == "1 Peter"
                && detection.verse_ref.chapter == 3
                && detection.verse_ref.verse_start == 15
        })
        .expect("1st Peter 3.15 must parse as an explicit reference");

    assert!(reference.confidence >= LIVE_THRESHOLD);
}

#[test]
fn dotted_second_ordinal_reference_reaches_live_threshold() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect("2nd Corinthians 10.5");
    let reference = detections
        .iter()
        .find(|detection| {
            detection.verse_ref.book_name == "2 Corinthians"
                && detection.verse_ref.chapter == 10
                && detection.verse_ref.verse_start == 5
        })
        .expect("2nd Corinthians 10.5 must parse as an explicit reference");

    assert!(reference.confidence >= LIVE_THRESHOLD);
}

#[test]
fn prose_number_after_dotted_reference_is_not_ambiguous() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect(
        "Which now and then is reflected in 2nd Corinthians 10.5 one of my favorite verses.",
    );
    let reference = detections
        .iter()
        .find(|detection| {
            detection.verse_ref.book_name == "2 Corinthians"
                && detection.verse_ref.chapter == 10
                && detection.verse_ref.verse_start == 5
        })
        .expect("the dotted reference must remain explicit before ordinary prose");

    assert!(reference.confidence >= LIVE_THRESHOLD);
}

#[test]
fn bare_number_range_joined_by_and_preserves_verse_end() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect("John 12, 31 and 32");
    let reference = detections
        .iter()
        .find(|detection| {
            detection.verse_ref.book_name == "John"
                && detection.verse_ref.chapter == 12
                && detection.verse_ref.verse_start == 31
        })
        .expect("John 12:31-32 must parse as an explicit reference");

    assert_eq!(reference.verse_ref.verse_end, Some(32));
}

#[test]
fn unconnected_third_number_surfaces_held_alternatives() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect("Matthew 25 45 41 was prepared for the devil and his angels.");
    let verses = detections
        .iter()
        .map(|detection| {
            (
                detection.verse_ref.verse_start,
                detection.confidence < LIVE_THRESHOLD,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(verses, vec![(45, true), (41, true)]);
}
