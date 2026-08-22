use rhema_detection::DirectDetector;

#[test]
fn singular_plural_book_variants_still_detect_through_direct_detector() {
    let mut detector = DirectDetector::new();

    // Chapter-only citations ("Hebrew 11" with no verse) are held for
    // refinement instead of emitted, so complete each citation with a verse.
    let hebrews = detector.detect("read Hebrew 11 verse 4");
    assert!(
        hebrews.iter().any(|detection| {
            detection.verse_ref.book_name == "Hebrews"
                && detection.verse_ref.chapter == 11
                && detection.verse_ref.verse_start == 4
        }),
        "singular Hebrew should recover Hebrews"
    );

    let romans = detector.detect("turn to Roman 8 verse 28");
    assert!(
        romans.iter().any(|detection| {
            detection.verse_ref.book_name == "Romans"
                && detection.verse_ref.chapter == 8
                && detection.verse_ref.verse_start == 28
        }),
        "singular Roman should recover Romans"
    );
}

#[test]
fn prose_number_still_does_not_fabricate_numbers_reference() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect("who's the number 1 in the room");

    assert!(
        detections
            .iter()
            .all(|detection| detection.verse_ref.book_name != "Numbers"),
        "ordinary number prose must not fuzzy-match Numbers"
    );
}

#[test]
fn daniel_seven_ten_number_prose_does_not_fabricate_numbers_reference() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect(
        "a fiery stream issued and came forth from before him a thousand thousands ministered to him ten thousand times ten thousand stood before him",
    );

    assert!(
        detections
            .iter()
            .all(|detection| detection.verse_ref.book_name != "Numbers"),
        "Daniel 7:10 number prose must not fabricate Numbers"
    );
}
