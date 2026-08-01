//! Regressions distilled from the 2026-07 closing-sermon golden fixture.
//! The segment mixes normal sermon story, pastoral prayer, and sung hymn
//! fragments. These tests keep generic prayer language from becoming a
//! live-strength semantic hit while preserving real semantic quote behavior.

use rhema_bible::Bm25Result;
use rhema_detection::semantic::embedder::StubEmbedder;
use rhema_detection::semantic::index::{SearchResult, VectorIndex};
use rhema_detection::{DetectionError, DetectionPipeline, SemanticDetector};

struct FakeIndex {
    results: Vec<SearchResult>,
}

impl VectorIndex for FakeIndex {
    fn search(&self, _query: &[f32], k: usize) -> Result<Vec<SearchResult>, DetectionError> {
        Ok(self.results.iter().take(k).cloned().collect())
    }

    fn len(&self) -> usize {
        self.results.len()
    }
}

fn detector_with_similarity(similarity: f64) -> SemanticDetector {
    SemanticDetector::new(
        Box::new(StubEmbedder::new(128)),
        Box::new(FakeIndex {
            results: vec![SearchResult {
                verse_id: 11_015_011,
                similarity,
            }],
        }),
    )
}

#[test]
fn closing_prayer_address_cannot_reach_live_fire_confidence() {
    let mut detector = detector_with_similarity(0.95);

    let detections = detector.detect(
        "And father, I pray that you may convict them, convert them right now on the spot, dear Lord, because you are able to do the impossible, dear father.",
    );

    assert_eq!(detections.len(), 1);
    assert!(
        detections[0].confidence < 0.90,
        "closing prayer address should remain a held semantic hint: {detections:?}"
    );
}

#[test]
fn high_confidence_non_prayer_semantic_match_is_not_capped() {
    let mut detector = detector_with_similarity(0.95);

    let detections =
        detector.detect("for God so loved the world that he gave his only begotten son");

    assert_eq!(detections.len(), 1);
    assert!(
        detections[0].confidence >= 0.95,
        "real semantic matches should keep their confidence: {detections:?}"
    );
}

#[test]
fn closing_prayer_fts_candidate_cannot_reach_live_fire_confidence() {
    let mut pipeline = DetectionPipeline::new();
    let fts_results = vec![Bm25Result {
        book_number: 11,
        book_name: "1 Kings".to_string(),
        chapter: 15,
        verse: 11,
        rank: -24.0,
        is_broad_match: false,
        is_phrase_match: false,
        text: "And Asa did that which was right in the eyes of the LORD, as did David his father."
            .to_string(),
    }];

    let results = pipeline.process_hybrid_with_fts(
        "And father, I pray that you may convict them, convert them right now on the spot, dear Lord, because you are able to do the impossible, dear father.",
        &fts_results,
    );

    let kings = results
        .iter()
        .find(|result| result.detection.verse_ref.book_name == "1 Kings")
        .expect("FTS candidate should remain available as a held hint");
    assert!(
        kings.detection.confidence < 0.90,
        "pastoral prayer FTS candidate must not reach live fire: {results:?}"
    );
}
