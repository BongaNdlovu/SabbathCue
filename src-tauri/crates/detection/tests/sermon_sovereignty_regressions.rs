//! Regressions distilled from a real live-sermon transcript (Daniel /
//! "God's sovereignty" sermon). The preacher cites references the way real
//! preachers do: "Daniel chapter one", long commentary, then "read verse 2",
//! later a bare "Chapter 2, verse 37" with no book name at all.

use rhema_detection::DirectDetector;

/// Live auto-fire threshold used by the app.
const LIVE_THRESHOLD: f64 = 0.90;

#[test]
fn explicit_spoken_chapter_only_is_held_and_completes_live() {
    // Since the 2026-08-21 citation-autolive contract, incomplete citations
    // emit nothing: "Daniel chapter one" is held for refinement, not carded.
    let mut detector = DirectDetector::new();

    let detections =
        detector.detect("In Daniel chapter one, by the way, now I want us to look at this text.");

    assert!(
        detections.iter().all(|d| d.verse_ref.book_name != "Daniel"),
        "spoken 'Daniel chapter one' must not emit a detection card: {detections:?}"
    );

    // The held citation is still a direct citation once the verse arrives,
    // and only then may it reach the live threshold.
    let completed = detector.detect("read verse 2");
    let daniel = completed
        .iter()
        .find(|d| d.verse_ref.book_name == "Daniel")
        .expect("held 'Daniel chapter one' must complete from 'read verse 2'");
    assert_eq!(daniel.verse_ref.chapter, 1);
    assert_eq!(daniel.verse_ref.verse_start, 2);
    assert!(
        daniel.confidence >= LIVE_THRESHOLD,
        "an explicitly spoken chapter reference completed by its verse must go live \
         (got {:.2})",
        daniel.confidence
    );
}

#[test]
fn daniel_reading_flow_verse_continuation_still_completes() {
    // The fast path: "Daniel chapter one" ... commentary ... "read verse 2"
    // within the incomplete-reference window.
    let mut detector = DirectDetector::new();

    detector.detect("In Daniel chapter one, by the way, now I want us to look at this text.");
    detector.detect(
        "In the third year of the reign of Joachim, king of Judah came Nebuchadnezzar, \
         king of Babylon, unto Jerusalem, and besieged it.",
    );
    detector.detect(
        "This is around 605, 606 BC, and so the record says, and they came and besieged it.",
    );
    let detections = detector.detect(
        "Read verse 2. So you've seen human responsibility, what men could do, their choices. \
         Listen to verse 2. And the Lord gave Joachim, king of Judah, into his hand.",
    );

    let daniel = detections
        .iter()
        .find(|d| d.verse_ref.book_name == "Daniel")
        .expect("'read verse 2' after 'Daniel chapter one' must resolve to Daniel 1:2");
    assert_eq!(daniel.verse_ref.chapter, 1);
    assert_eq!(daniel.verse_ref.verse_start, 2);
}

#[test]
fn bare_verse_reference_resolves_from_context_after_full_citation() {
    // A full citation ("Daniel 3:15") clears the incomplete-reference state.
    // A later bare "Verse 27" (no book anywhere in the fragment) remains
    // visible, but its inferred book must not auto-fire.
    let mut detector = DirectDetector::new();

    detector.detect("Now, Daniel 3:15, can you read that one?");
    let detections = detector.detect(
        "Verse 27. Remember we read it? Verse 27. Therefore, O king, let my counsel be \
         acceptable unto thee.",
    );

    let daniel = detections
        .iter()
        .find(|d| d.verse_ref.book_name == "Daniel")
        .expect("bare 'verse 27' with recent Daniel context must surface a candidate");
    assert_eq!(daniel.verse_ref.chapter, 3);
    assert_eq!(daniel.verse_ref.verse_start, 27);
    assert!(
        daniel.confidence < LIVE_THRESHOLD,
        "an inferred book must not auto-fire (got {:.2})",
        daniel.confidence
    );
}

#[test]
fn bare_chapter_verse_reference_resolves_book_from_context() {
    // "Chapter 2, verse 37" spoken with no book name: the book comes from
    // context (the sermon's active book), chapter/verse are explicit.
    let mut detector = DirectDetector::new();

    detector.detect("Now, Daniel 3:15, can you read that one?");
    let detections =
        detector.detect("Chapter 2, verse 37, the Bible says, you O king are the king of kings.");

    let daniel = detections
        .iter()
        .find(|d| d.verse_ref.book_name == "Daniel")
        .expect("bare 'chapter 2 verse 37' with recent Daniel context must surface a candidate");
    assert_eq!(daniel.verse_ref.chapter, 2);
    assert_eq!(daniel.verse_ref.verse_start, 37);
    assert!(
        daniel.confidence < LIVE_THRESHOLD,
        "an inferred book must not auto-fire (got {:.2})",
        daniel.confidence
    );
}

#[test]
fn supplied_thessalonians_transcript_keeps_scope_and_surfaces_bare_verses() {
    let mut detector = DirectDetector::new();

    let opening = detector.detect(
        "Testing, testing 1, 2 testing. Alright, so when I talk about, um, the \
         resurrection, I'm going to fa- start this, start first in, um, in 1 \
         Thessalonians chapter 4, verse 7.",
    );
    assert!(
        opening.iter().any(|d| {
            d.verse_ref.book_name == "1 Thessalonians"
                && d.verse_ref.chapter == 4
                && d.verse_ref.verse_start == 7
        }),
        "the opening full citation must establish 1 Thessalonians 4: {opening:?}"
    );

    detector.detect(
        "And, um, I mean, as you can see, this verse, it's God talking about a- \
         about calling us unto- not unto uncleanness, but unto holiness. And it's \
         very important to note, because in certain points in life it's very \
         difficult, you know, to- to align yourself with God's will. And in that \
         difficulty, you can see the standard to which we- we reach. And that is \
         holiness, you know, and, um,",
    );
    let verse_eight = detector.detect("let's go to verse 8.");
    let thessalonians_eight = verse_eight
        .iter()
        .find(|d| {
            d.verse_ref.book_name == "1 Thessalonians"
                && d.verse_ref.chapter == 4
                && d.verse_ref.verse_start == 8
        })
        .expect("bare verse 8 must resolve inside the active passage");
    assert!(
        thessalonians_eight.confidence < LIVE_THRESHOLD,
        "inferred verse 8 must remain operator-reviewed (got {:.2})",
        thessalonians_eight.confidence
    );

    detector.detect("Um, and then you see");
    detector.detect("and then you see in.");
    let verse_seventeen = detector.detect(
        "The- in the- in the same chapter, that he's given us his Holy Spirit, \
         you know. And then let's go into verse 17 now",
    );

    assert!(
        verse_seventeen.iter().any(|d| {
            d.verse_ref.book_name == "1 Thessalonians"
                && d.verse_ref.chapter == 4
                && d.verse_ref.verse_start == 17
        }),
        "bare verse 17 must remain in the active 1 Thessalonians 4 passage: \
         {verse_seventeen:?}"
    );
    assert!(
        verse_seventeen
            .iter()
            .all(|d| d.verse_ref.book_name != "James"),
        "ordinary phrase 'same chapter' must not fabricate James: {verse_seventeen:?}"
    );
}

#[test]
fn bare_verse_reference_without_any_context_stays_silent() {
    let mut detector = DirectDetector::new();

    let detections = detector.detect("Verse 27. Remember we read it? Verse 27.");

    assert!(
        detections.is_empty(),
        "bare 'verse 27' with no prior context must not fabricate a reference"
    );
}

#[test]
fn inferred_book_reference_stays_below_live_threshold() {
    let mut detector = DirectDetector::new();

    detector.detect("Now, Daniel 3:15, can you read that one?");
    let detections =
        detector.detect("Chapter 2, verse 37, the Bible says, you O king are the king of kings.");
    let inferred = detections
        .iter()
        .find(|d| d.verse_ref.book_name == "Daniel")
        .expect("the inferred reference must remain visible to the operator");

    assert!(
        inferred.confidence < LIVE_THRESHOLD,
        "a book inferred from mutable context must not auto-fire (got {:.2})",
        inferred.confidence
    );
}

#[test]
fn prose_numbers_do_not_become_context_resolved_references() {
    // Ordinary numbers in commentary after a citation must not turn into
    // verse candidates ("This is around 605, 606 BC", "Start at four").
    let mut detector = DirectDetector::new();

    detector.detect("Now, Daniel 3:15, can you read that one?");
    let bc = detector.detect(
        "This is around 605, 606 BC, and so the record says, and they came and besieged it.",
    );
    let start_at_four = detector
        .detect("Don't read it from one, two, three, you're going to get lost. Start at four.");

    assert!(
        bc.is_empty(),
        "prose numbers must not resolve into context references: {bc:?}"
    );
    assert!(
        start_at_four.is_empty(),
        "prose numbers must not resolve into context references: {start_at_four:?}"
    );
}
