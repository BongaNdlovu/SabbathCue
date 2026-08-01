//! Regressions distilled from a real Balaam / end-time sermon transcript.
//! The sermon mixes explicit citations, quoted scripture, prophetic book names,
//! and ordinary numbers in long-form preaching prose.

use rhema_bible::Bm25Result;
use rhema_detection::{DetectionPipeline, DirectDetector};

#[test]
fn balaam_sermon_explicit_references_survive_real_preaching_prose() {
    let cases = [
        (
            "What did Jesus say? If you love me, keep my commandments. \
             We know that very well, right? That's John 14, 15.",
            "John",
            14,
            15,
            None,
        ),
        (
            "So, let's pick up the story in Numbers, chapter 23, verse 1.",
            "Numbers",
            23,
            1,
            None,
        ),
        (
            "And this is what he says. This just amazes me. Numbers 23 and verse 8.",
            "Numbers",
            23,
            8,
            None,
        ),
        (
            "But notice in Numbers 24, verse 17, this time Balaam comes with a prophecy.",
            "Numbers",
            24,
            17,
            None,
        ),
        (
            "Now all these things Paul writes, 1 Corinthians 10 verses 11.",
            "1 Corinthians",
            10,
            11,
            None,
        ),
        (
            "So what does the apostle Paul says our response should be? \
             He says this in Philippians 4 verse 8.",
            "Philippians",
            4,
            8,
            None,
        ),
        (
            "What is the definition of sin? 1 John 3 verse 4 says sin is the transgression of the law.",
            "1 John",
            3,
            4,
            None,
        ),
        (
            "Revelation 16 verses 12 to 14 shows three unclean spirits like frogs.",
            "Revelation",
            16,
            12,
            Some(14),
        ),
    ];

    for (transcript, book, chapter, verse, verse_end) in cases {
        let mut detector = DirectDetector::new();
        let detections = detector.detect(transcript);

        assert!(
            detections.iter().any(|detection| {
                detection.verse_ref.book_name == book
                    && detection.verse_ref.chapter == chapter
                    && detection.verse_ref.verse_start == verse
                    && detection.verse_ref.verse_end == verse_end
            }),
            "expected {book} {chapter}:{verse:?} in {transcript:?}, got {detections:?}"
        );
    }
}

#[test]
fn balaam_sermon_quote_surfaces_through_hybrid_fts_overlap() {
    let mut pipeline = DetectionPipeline::new();
    let fts_results = vec![Bm25Result {
        book_number: 19,
        book_name: "Psalms".to_string(),
        chapter: 1,
        verse: 1,
        rank: -10.0,
        is_broad_match: true,
        is_phrase_match: false,
        text: "Blessed is the man that walketh not in the counsel of the ungodly, \
               nor standeth in the way of sinners, nor sitteth in the seat of the scornful."
            .to_string(),
    }];

    let results = pipeline.process_hybrid_with_fts(
        "Blessed is the man who walks not in the counsel of the ungodly, \
         nor stands in the path of sinners, nor sits in the seat of the scornful.",
        &fts_results,
    );

    assert_eq!(
        results.len(),
        1,
        "Psalm 1 quote should surface: {results:?}"
    );
    assert_eq!(results[0].detection.verse_ref.book_name, "Psalms");
    assert_eq!(results[0].detection.verse_ref.chapter, 1);
    assert_eq!(results[0].detection.verse_ref.verse_start, 1);
    assert!(
        !results[0].auto_queued,
        "semantic/FTS quote hints must not auto-queue directly"
    );
}

#[test]
fn balaam_sermon_prose_numbers_do_not_fabricate_direct_references() {
    let mut detector = DirectDetector::new();

    let chunks = [
        "John tells us in John 22, He is the one, He has a scepter shall rise out of Israel.",
        "There were two mountains as Israel entered into the promised land.",
        "You have the sanctuary in the center, and then on each side the sanctuary.",
        "How many tribes are there? Twelve. You've got three on each side.",
        "These were the final generation of those that rebelled against Christ.",
    ];

    for chunk in chunks {
        let detections = detector.detect(chunk);
        assert!(
            detections.is_empty(),
            "ordinary sermon prose must not fabricate direct references in {chunk:?}: {detections:?}"
        );
    }
}
