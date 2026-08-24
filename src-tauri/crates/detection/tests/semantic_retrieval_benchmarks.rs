//! Semantic retrieval benchmarks and cache correctness.
//!
//! These run as ordinary `cargo test` gates (time + ranking assertions), not
//! Criterion benches, so CI fails when retrieval or cache behaviour drifts.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rhema_bible::Bm25Result;
use rhema_detection::semantic::cache::EmbeddingCache;
use rhema_detection::semantic::detector::SemanticDetector;
use rhema_detection::semantic::embedder::{StubEmbedder, TextEmbedder};
use rhema_detection::semantic::index::{SearchResult, VectorIndex};
use rhema_detection::DetectionError;
use rhema_detection::DetectionPipeline;
use rhema_detection::{decide_presentation, DetectionJob, PresentationDecision, PresentationEvidence};

const JOHN_316_KJV: &str = "For God so loved the world, that he gave his only begotten Son, \
     that whosoever believeth in him should not perish, but have everlasting life.";

fn john_316_fts(rank: f64) -> Bm25Result {
    Bm25Result {
        book_number: 43,
        book_name: "John".to_string(),
        chapter: 3,
        verse: 16,
        rank,
        is_broad_match: false,
        is_phrase_match: false,
        text: JOHN_316_KJV.to_string(),
    }
}

#[test]
fn reordered_verse_text_does_not_outrank_the_spoken_order() {
    let spoken = "For God so loved the world that he gave his only begotten Son";
    let reordered = "Son begotten only his gave he that world the loved so God For";
    let mut pipeline = DetectionPipeline::new();
    let ordered = pipeline.process_hybrid_with_fts(spoken, &[john_316_fts(-20.0)]);
    let shuffled = pipeline.process_hybrid_with_fts(reordered, &[john_316_fts(-20.0)]);

    assert!(
        !ordered.is_empty(),
        "ordered quotation must retrieve John 3:16"
    );
    let ordered_conf = ordered[0].detection.confidence;
    let shuffled_conf = shuffled.first().map_or(0.0, |hit| hit.detection.confidence);
    assert!(
        ordered_conf > shuffled_conf + 0.05,
        "reordered bag-of-words must not beat the spoken order ({ordered_conf} vs {shuffled_conf})"
    );
}

#[test]
fn long_sermon_with_a_late_quote_stays_within_the_latency_gate() {
    let mut filler = String::new();
    for i in 0..400 {
        filler.push_str("beloved congregation we continue in the word of God today ");
        if i % 40 == 0 {
            filler.push_str("amen. ");
        }
    }
    filler.push_str(JOHN_316_KJV);

    let mut pipeline = DetectionPipeline::new();
    let started = Instant::now();
    let results = pipeline.process_hybrid_with_fts(&filler, &[john_316_fts(-22.0)]);
    let elapsed = started.elapsed();

    assert!(
        !results.is_empty(),
        "a quote at the end of a long sermon must still retrieve"
    );
    assert!(
        elapsed < Duration::from_millis(80),
        "hybrid retrieval on a long sermon must stay under 80ms, took {elapsed:?}"
    );
}

#[test]
fn tied_candidates_preview_instead_of_vanishing() {
    let grant = decide_presentation(&PresentationEvidence {
        job: DetectionJob::Quotation,
        source_is_direct: false,
        is_chapter_only: false,
        is_fuzzy_book: false,
        is_complete_citation: false,
        is_final_utterance: true,
        has_lexical_quote: true,
        quote_coverage: 0.86,
        candidate_margin: 0.0,
        independent_final_count: 1,
        automation_live_enabled: true,
    });
    assert_eq!(grant.decision, PresentationDecision::PreviewAuthorized);
    assert!(grant.may_preview());
    assert!(!grant.may_go_live());
}

#[test]
fn embedding_cache_returns_the_same_hit_and_evicts_least_recent() {
    let mut cache = EmbeddingCache::new(2);
    cache.insert("a".into(), (vec![1.0], vec![SearchResult { verse_id: 1, similarity: 0.9 }]));
    cache.insert("b".into(), (vec![2.0], vec![SearchResult { verse_id: 2, similarity: 0.8 }]));

    let first = cache.get("a").expect("a stays hot");
    assert_eq!(first.1[0].verse_id, 1);

    cache.insert("c".into(), (vec![3.0], vec![]));
    assert!(cache.get("b").is_none(), "least-recent b must be evicted");
    assert!(cache.get("a").is_some());
    assert!(cache.get("c").is_some());
}

struct CountingEmbedder {
    inner: StubEmbedder,
    embeds: Arc<AtomicUsize>,
}

impl TextEmbedder for CountingEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, DetectionError> {
        self.embeds.fetch_add(1, Ordering::SeqCst);
        self.inner.embed(text)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
}

struct ConstIndex {
    results: Vec<SearchResult>,
}

impl VectorIndex for ConstIndex {
    fn search(&self, _query: &[f32], k: usize) -> Result<Vec<SearchResult>, DetectionError> {
        Ok(self.results.iter().take(k).cloned().collect())
    }

    fn len(&self) -> usize {
        self.results.len()
    }
}

#[test]
fn direct_embedding_path_does_not_reembed_a_cached_chunk() {
    let embeds = Arc::new(AtomicUsize::new(0));
    let mut detector = SemanticDetector::new(
        Box::new(CountingEmbedder {
            inner: StubEmbedder::new(8),
            embeds: embeds.clone(),
        }),
        Box::new(ConstIndex {
            results: vec![SearchResult {
                verse_id: 43,
                similarity: 0.91,
            }],
        }),
    );
    detector.set_use_synonyms(false);

    let chunk = "for God so loved the world that he gave his only begotten son";
    let first = detector.detect(chunk);
    let after_first = embeds.load(Ordering::SeqCst);
    let second = detector.detect(chunk);

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert!(after_first >= 1, "first pass must embed");
    assert_eq!(
        embeds.load(Ordering::SeqCst),
        after_first,
        "the second identical chunk must hit the embedding cache"
    );
}
