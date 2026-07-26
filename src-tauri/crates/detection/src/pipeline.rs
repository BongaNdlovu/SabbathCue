use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rhema_bible::Bm25Result;

use crate::direct::detector::DirectDetector;
use crate::merger::{AutoQueueCooldown, DetectionMerger, MergedDetection};
use crate::semantic::detector::cap_pastoral_prayer_address_confidence;
use crate::semantic::detector::SemanticDetector;
use crate::types::{Detection, DetectionSource, VerseRef};

/// Confidence assigned to the best FTS5 BM25 match (rank 0).
const FTS5_RANK0_CONFIDENCE: f64 = 0.68;

/// Confidence decrease per FTS5 rank position (rank 1 = 0.64, rank 2 = 0.60, etc.).
const FTS5_CONFIDENCE_DECAY: f64 = 0.04;

/// Very strong phrase/AND matches should score like a near-verbatim quote.
const FTS5_EXCELLENT_MATCH_RANK: f64 = -24.0;
const FTS5_EXCELLENT_MATCH_CONFIDENCE: f64 = 0.92;

/// FTS5 results below this confidence are not included.
const FTS5_MIN_CONFIDENCE: f64 = 0.50;

/// FTS5 BM25 scores are negative; more negative = stronger match. Live keyword
/// candidates must beat this floor to surface. Calibrated against the real
/// corpus: reference-command keyword noise tops out near -11..-12, while genuine
/// verse-text matches run <= -16, so -13 separates them. (The search UI is
/// unaffected — only the live detection path applies this floor.)
const FTS5_LIVE_RANK_FLOOR: f64 = -13.0;

/// Minimum word count for vector embedding search (short text lacks semantic signal).
const MIN_WORDS_FOR_VECTOR: usize = 4;

const OVERLAP_CONFIDENCE_BOOST: f64 = 0.10;

const LIVE_SEMANTIC_CAP: usize = 5;

/// Quote-overlap verification: how much of a candidate verse's content
/// vocabulary must appear in the spoken fragment before the overlap counts as
/// quote evidence. Guards (minimum matched words, minimum verse vocabulary)
/// keep short verses and scattered keyword coincidences from qualifying.
/// The verse-vocabulary floor sits at the matched-words floor so a fully
/// spoken short verse still qualifies — Psalm 23:1 ("The LORD is my shepherd;
/// I shall not want") has exactly four content words (lord, shepherd, shall,
/// want), so a floor above four silently excluded famous short verses.
const QUOTE_OVERLAP_MIN_FRACTION: f64 = 0.28;
const QUOTE_OVERLAP_MIN_MATCHED: usize = 4;
const QUOTE_OVERLAP_MIN_VERSE_WORDS: usize = 4;
const QUOTE_OVERLAP_MAX_CONFIDENCE: f64 = 0.92;
/// Words shorter than this are too common (the, and, thy, God) to count as
/// quote evidence either way.
const QUOTE_OVERLAP_MIN_WORD_LEN: usize = 4;

/// The main detection pipeline that runs on each transcript segment.
///
/// Orchestrates direct reference detection, semantic search, and merging
/// into a single call. Consumers should create one pipeline and reuse it
/// across transcript segments so that the merger's cooldown state is preserved.
pub struct DetectionPipeline {
    direct: DirectDetector,
    semantic: SemanticDetector,
    merger: DetectionMerger,
}

impl DetectionPipeline {
    pub fn new() -> Self {
        Self::with_cooldown(AutoQueueCooldown::default())
    }

    pub fn with_cooldown(cooldown: AutoQueueCooldown) -> Self {
        Self {
            direct: DirectDetector::new(),
            semantic: SemanticDetector::stub(),
            merger: DetectionMerger::with_cooldown(cooldown),
        }
    }

    /// Replace the semantic detector (e.g., after loading an ONNX model).
    pub fn set_semantic(&mut self, detector: SemanticDetector) {
        self.semantic = detector;
    }

    /// Access the direct detector for configuration.
    pub fn direct_mut(&mut self) -> &mut DirectDetector {
        &mut self.direct
    }

    /// Access the merger for threshold configuration.
    pub fn merger_mut(&mut self) -> &mut DetectionMerger {
        &mut self.merger
    }

    /// Current semantic visibility threshold used by the merger.
    pub fn semantic_confidence_threshold(&self) -> f64 {
        self.merger.semantic_confidence_threshold()
    }

    /// Run the full pipeline (direct + semantic + merge). Used by `detect_verses` command.
    pub fn process(&mut self, text: &str) -> Vec<MergedDetection> {
        let direct_results = self.direct.detect(text);

        let semantic_results = if text.split_whitespace().count() >= MIN_WORDS_FOR_VECTOR {
            self.semantic.detect(text)
        } else {
            vec![]
        };

        self.merger.merge(direct_results, semantic_results)
    }

    /// Run only direct (regex/pattern) detection. Instant, no ONNX inference.
    /// Used during live transcription on every `is_final` fragment.
    pub fn process_direct(&mut self, text: &str) -> Vec<MergedDetection> {
        let direct_results = self.direct.detect(text);
        self.merger.merge(direct_results, vec![])
    }

    /// Run only semantic (ONNX embedding) detection. Slow, 50-400ms.
    /// Used on `speech_final` only, in a background task.
    pub fn process_semantic(&mut self, text: &str) -> Vec<MergedDetection> {
        if text.split_whitespace().count() < MIN_WORDS_FOR_VECTOR {
            return vec![];
        }
        let semantic_results = self.semantic.detect(text);
        self.merger.merge(vec![], semantic_results)
    }

    /// Check if semantic search is available (model loaded + index populated).
    pub fn has_semantic(&self) -> bool {
        self.semantic.is_ready()
    }

    /// Embed arbitrary text with the semantic embedder, if one is loaded.
    /// Used by callers that maintain their own vector index (e.g. EGW
    /// context search).
    pub fn embed_text(&self, text: &str) -> Option<Vec<f32>> {
        if !self.has_semantic() {
            return None;
        }
        self.semantic.embed_text(text)
    }

    /// Dimensionality of the semantic embedder's vectors, if one is loaded.
    pub fn embedding_dimension(&self) -> Option<usize> {
        if !self.has_semantic() {
            return None;
        }
        Some(self.semantic.embedding_dimension())
    }

    /// Enable or disable synonym expansion (paraphrase detection mode).
    pub fn set_use_synonyms(&mut self, enabled: bool) {
        self.semantic.set_use_synonyms(enabled);
    }

    /// Returns whether synonym expansion is currently enabled.
    pub fn use_synonyms(&self) -> bool {
        self.semantic.use_synonyms()
    }

    /// Run hybrid semantic detection combining vector search with pre-fetched
    /// FTS5 BM25 results. Used by the real-time STT pipeline.
    ///
    /// FTS5-only results are added with rank-derived confidence. Vector and
    /// FTS5 overlap is collapsed into one boosted candidate.
    pub fn process_hybrid_with_fts(
        &mut self,
        text: &str,
        fts_results: &[Bm25Result],
    ) -> Vec<MergedDetection> {
        // Vector search needs enough words for meaningful embeddings;
        // FTS5 keyword matching works with fewer words.
        let mut semantic_detections = if text.split_whitespace().count() >= MIN_WORDS_FOR_VECTOR {
            self.semantic.detect(text)
        } else {
            vec![]
        };

        if fts_results.is_empty() {
            let mut merged = self.merger.merge(vec![], semantic_detections);
            merged.truncate(LIVE_SEMANTIC_CAP);
            return merged;
        }

        #[expect(
            clippy::cast_possible_truncation,
            reason = "timestamp millis won't exceed u64"
        )]
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let snippet = text.to_string();
        let mut vector_keys: HashSet<(i32, i32, i32)> = semantic_detections
            .iter()
            .map(detection_verse_key)
            .collect();

        for (rank, fts) in fts_results.iter().enumerate() {
            // Quote-overlap verification: a candidate whose verse text is
            // substantially present in the fragment is a spoken quote, no
            // matter which FTS tier surfaced it or how BM25 ranked it.
            // Garbled STT breaks phrase/AND tiers, so genuine near-verbatim
            // quotes routinely arrive as keyword-band OR hits.
            let overlap_confidence = quote_overlap_confidence(text, &fts.text);
            let rank_confidence = fts_confidence(rank, fts.rank, fts.is_broad_match);
            let confidence = cap_pastoral_prayer_address_confidence(
                text,
                overlap_confidence.map_or(rank_confidence, |overlap| overlap.max(rank_confidence)),
            );
            log::debug!(
                "[DET-SEMANTIC] FTS5 candidate idx={rank} bm25={:.3} {} {}:{} conf={:.0}% overlap={:?}",
                fts.rank,
                fts.book_name,
                fts.chapter,
                fts.verse,
                confidence * 100.0,
                overlap_confidence
            );
            if confidence < FTS5_MIN_CONFIDENCE {
                continue;
            }
            if fts.rank > FTS5_LIVE_RANK_FLOOR && overlap_confidence.is_none() {
                continue;
            }
            let key = (fts.book_number, fts.chapter, fts.verse);
            if vector_keys.contains(&key) {
                if let Some(existing) = semantic_detections
                    .iter_mut()
                    .find(|detection| detection_verse_key(detection) == key)
                {
                    existing.confidence = (existing.confidence + OVERLAP_CONFIDENCE_BOOST)
                        .min(1.0)
                        .max(overlap_confidence.unwrap_or(0.0));
                    if let DetectionSource::Semantic { similarity } = &mut existing.source {
                        *similarity = (*similarity + OVERLAP_CONFIDENCE_BOOST)
                            .min(1.0)
                            .max(overlap_confidence.unwrap_or(0.0));
                    }
                }
                continue;
            }
            log::debug!(
                "[HYBRID] FTS5 hit: {} {}:{} rank={} conf={:.0}%",
                fts.book_name,
                fts.chapter,
                fts.verse,
                rank,
                confidence * 100.0
            );
            semantic_detections.push(Detection {
                verse_ref: VerseRef {
                    book_number: fts.book_number,
                    book_name: fts.book_name.clone(),
                    chapter: fts.chapter,
                    verse_start: fts.verse,
                    verse_end: None,
                },
                verse_id: None,
                confidence,
                source: DetectionSource::Semantic {
                    similarity: confidence,
                },
                transcript_snippet: snippet.clone(),
                detected_at: now,
                is_chapter_only: false,
            });
            vector_keys.insert(key);
        }

        // Gate every live candidate — FTS-derived and vector alike — by the
        // operator's semantic visibility threshold so raising the slider
        // actually suppresses keyword noise instead of letting FTS hits bypass.
        let mut merged = self.merger.merge(vec![], semantic_detections);
        merged.truncate(LIVE_SEMANTIC_CAP);
        merged
    }

    /// Run a standalone semantic search query (for the search UI).
    pub fn semantic_search(&mut self, query: &str, k: usize) -> Vec<(i64, f64)> {
        self.semantic.search_query(query, k)
    }
}

/// Confidence earned by quote overlap: the fraction of the candidate verse's
/// content vocabulary present in the spoken fragment, mapped onto
/// hint-to-quote confidence. `None` when the evidence is too thin to count
/// (short verse, few matched words, low fraction).
///
/// Word matching is exact on lowercased tokens of at least
/// `QUOTE_OVERLAP_MIN_WORD_LEN` letters, so archaic/garbled inflections
/// (shewing/showing) count against the fraction — a candidate only reaches
/// fire strength when most of the verse really was spoken.
#[expect(clippy::cast_precision_loss, reason = "verse word counts are tiny")]
fn quote_overlap_confidence(fragment: &str, verse_text: &str) -> Option<f64> {
    if verse_text.is_empty() {
        return None;
    }
    let fragment_words: HashSet<String> = content_words(fragment).collect();
    let verse_words: HashSet<String> = content_words(verse_text).collect();
    if verse_words.len() < QUOTE_OVERLAP_MIN_VERSE_WORDS {
        return None;
    }
    let matched = verse_words
        .iter()
        .filter(|word| fragment_words.contains(*word))
        .count();
    if matched < QUOTE_OVERLAP_MIN_MATCHED {
        return None;
    }
    let fraction = matched as f64 / verse_words.len() as f64;
    if fraction < QUOTE_OVERLAP_MIN_FRACTION {
        return None;
    }
    // 0.28 → ~0.71 (barely-visible hint), 0.56 → ~0.90 (fire), capped 0.92.
    Some((0.52 + 0.68 * fraction).min(QUOTE_OVERLAP_MAX_CONFIDENCE))
}

pub fn content_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= QUOTE_OVERLAP_MIN_WORD_LEN)
        .map(str::to_lowercase)
}

#[expect(clippy::cast_precision_loss, reason = "rank index is small")]
fn fts_confidence(rank: usize, bm25_rank: f64, is_broad_match: bool) -> f64 {
    let rank_confidence = FTS5_RANK0_CONFIDENCE - (rank as f64 * FTS5_CONFIDENCE_DECAY);
    if bm25_rank <= FTS5_EXCELLENT_MATCH_RANK && !is_broad_match {
        rank_confidence.max(FTS5_EXCELLENT_MATCH_CONFIDENCE)
    } else {
        // Keyword-band matches keep their honest rank-derived confidence instead
        // of being floored up to a fixed "strong" score. Otherwise scattered
        // common-word hits masquerade as confident detections and flood the live
        // panel regardless of the operator's semantic threshold.
        rank_confidence
    }
}

fn detection_verse_key(detection: &Detection) -> (i32, i32, i32) {
    (
        detection.verse_ref.book_number,
        detection.verse_ref.chapter,
        detection.verse_ref.verse_start,
    )
}

impl Default for DetectionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::detector::SemanticDetector;
    use crate::semantic::embedder::StubEmbedder;
    use crate::semantic::index::{SearchResult, VectorIndex};
    use crate::DetectionError;
    use rhema_bible::Bm25Result;

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

    #[test]
    fn test_pipeline_direct_only() {
        let mut pipeline = DetectionPipeline::new();
        let results = pipeline.process("Jesus said in John 3:16 that God loved the world");
        assert!(!results.is_empty());
        assert_eq!(results[0].detection.verse_ref.book_name, "John");
        assert_eq!(results[0].detection.verse_ref.chapter, 3);
        assert_eq!(results[0].detection.verse_ref.verse_start, 16);
    }

    #[test]
    fn active_provider_transcripts_keep_pipeline_direct_accuracy() {
        let cases = [
            ("vosk", "john chapter three verse sixteen"),
            ("deepgram", "John 3:16"),
            ("deepgram", "John three sixteen"),
        ];

        for (provider, transcript) in cases {
            let mut pipeline = DetectionPipeline::new();
            let results = pipeline.process_direct(transcript);
            assert_eq!(results.len(), 1, "{provider} direct transcript");
            assert_eq!(
                results[0].detection.verse_ref.book_name, "John",
                "{provider}"
            );
            assert_eq!(results[0].detection.verse_ref.chapter, 3, "{provider}");
            assert_eq!(results[0].detection.verse_ref.verse_start, 16, "{provider}");
            assert!(
                matches!(
                    results[0].detection.source,
                    DetectionSource::DirectReference
                ),
                "{provider} transcript should stay direct"
            );
        }
    }

    #[test]
    fn natural_speech_with_direct_reference_mistake_stays_direct() {
        let mut pipeline = DetectionPipeline::new();
        let results = pipeline
            .process_direct("pastor said let's read from Filipians chapter four verse thirteen");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].detection.verse_ref.book_name, "Philippians");
        assert_eq!(results[0].detection.verse_ref.chapter, 4);
        assert_eq!(results[0].detection.verse_ref.verse_start, 13);
        assert!(matches!(
            results[0].detection.source,
            DetectionSource::DirectReference
        ));
    }

    #[test]
    fn niv_worded_jeremiah_quote_surfaces_via_fts_quote_overlap() {
        // 2026-07-07 incident: the speaker quoted Jeremiah 29:11 in NIV
        // wording. The redistributable embeddings corpus is KJV-family, so the
        // vector leg cannot catch NIV phrasing — the FTS quote-overlap path
        // over the full translation table is the designed catcher and must
        // produce a live candidate for it.
        let mut pipeline = DetectionPipeline::new();

        let spoken = "so the plans that i have for you are not to harm you but to prosper you";
        let fts = vec![Bm25Result {
            rank: -6.0, // keyword-band OR hit, not a phrase-tier match
            book_number: 24,
            book_name: "Jeremiah".to_string(),
            chapter: 29,
            verse: 11,
            is_broad_match: true,
            text: "\"For I know the plans I have for you,\" declares the LORD, \
                   \"plans to prosper you and not to harm you, plans to give you hope and a future.\""
                .to_string(),
        }];

        let results = pipeline.process_hybrid_with_fts(spoken, &fts);

        let jeremiah = results
            .iter()
            .find(|r| {
                r.detection.verse_ref.book_number == 24
                    && r.detection.verse_ref.chapter == 29
                    && r.detection.verse_ref.verse_start == 11
            })
            .expect("NIV-worded Jeremiah 29:11 quote must surface as a live candidate");
        assert!(
            jeremiah.detection.confidence >= 0.75,
            "quote-overlap confidence should clear the live threshold, got {}",
            jeremiah.detection.confidence
        );
    }

    #[test]
    fn test_pipeline_no_match() {
        let mut pipeline = DetectionPipeline::new();
        let results = pipeline.process("The weather is nice today");
        assert!(results.is_empty());
    }

    #[test]
    fn test_pipeline_multiple_references() {
        let mut pipeline = DetectionPipeline::new();
        let results =
            pipeline.process("Compare John 3:16 with Romans 5:8 for understanding God's love");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_pipeline_semantic_not_ready_by_default() {
        let pipeline = DetectionPipeline::new();
        assert!(!pipeline.has_semantic());
    }

    #[test]
    fn test_pipeline_semantic_keeps_distinct_vector_hits_after_merge() {
        let mut pipeline = DetectionPipeline::new();
        let mut semantic = SemanticDetector::new(
            Box::new(StubEmbedder::new(128)),
            Box::new(FakeIndex {
                results: vec![
                    SearchResult {
                        verse_id: 1001,
                        similarity: 0.86,
                    },
                    SearchResult {
                        verse_id: 1002,
                        similarity: 0.79,
                    },
                    SearchResult {
                        verse_id: 1003,
                        similarity: 0.72,
                    },
                ],
            }),
        );
        semantic.set_use_synonyms(false);
        pipeline.set_semantic(semantic);

        let results =
            pipeline.process_semantic("God loved the world enough to give his only son for us");

        assert_eq!(results.len(), 3);
        let ids: Vec<Option<i64>> = results.iter().map(|r| r.detection.verse_id).collect();
        assert_eq!(ids, vec![Some(1001), Some(1002), Some(1003)]);
        assert!(results
            .iter()
            .all(|r| matches!(r.detection.source, DetectionSource::Semantic { .. })));
    }

    #[test]
    fn active_provider_transcripts_keep_pipeline_semantic_accuracy() {
        let cases = [
            ("vosk", "for god so loved the world and gave his son"),
            (
                "deepgram",
                "God loved the world enough to give his only Son.",
            ),
            (
                "deepgram",
                "God loved the world so much that he gave his only son",
            ),
        ];

        for (provider, transcript) in cases {
            let mut pipeline = DetectionPipeline::new();
            let mut semantic = SemanticDetector::new(
                Box::new(StubEmbedder::new(128)),
                Box::new(FakeIndex {
                    results: vec![SearchResult {
                        verse_id: 43_003_016,
                        similarity: 0.88,
                    }],
                }),
            );
            semantic.set_use_synonyms(false);
            pipeline.set_semantic(semantic);

            let results = pipeline.process_semantic(transcript);

            assert_eq!(results.len(), 1, "{provider} semantic transcript");
            assert_eq!(
                results[0].detection.verse_id,
                Some(43_003_016),
                "{provider}"
            );
            assert!(
                matches!(
                    results[0].detection.source,
                    DetectionSource::Semantic { .. }
                ),
                "{provider} transcript should stay semantic"
            );
            assert!(results[0].detection.confidence >= 0.88, "{provider}");
        }
    }

    #[test]
    fn human_quote_with_common_word_mistake_stays_semantic() {
        let mut pipeline = DetectionPipeline::new();
        let mut semantic = SemanticDetector::new(
            Box::new(StubEmbedder::new(128)),
            Box::new(FakeIndex {
                results: vec![SearchResult {
                    verse_id: 19_023_001,
                    similarity: 0.89,
                }],
            }),
        );
        semantic.set_use_synonyms(false);
        pipeline.set_semantic(semantic);

        let results = pipeline.process_semantic("the lord is my shepard I shall not want");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].detection.verse_id, Some(19_023_001));
        assert!(matches!(
            results[0].detection.source,
            DetectionSource::Semantic { .. }
        ));
    }

    #[test]
    fn test_pipeline_auto_queue_for_direct() {
        let mut pipeline = DetectionPipeline::new();
        let results = pipeline.process("John 3:16");
        assert!(!results.is_empty());
        // Exact direct references have 1.0 confidence, above the conservative
        // default auto_queue_threshold (0.98), so should be auto-queued.
        assert!(results[0].auto_queued);
    }

    #[test]
    fn test_pipeline_hybrid_with_fts_returns_results() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 3,
                verse: 16,
                rank: -24.0,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 45,
                book_name: "Romans".to_string(),
                chapter: 5,
                verse: 8,
                rank: -24.0,
                is_broad_match: false,
                text: String::new(),
            },
        ];

        let results = pipeline.process_hybrid_with_fts("test text", &fts_results);

        // Should return FTS5-backed results even without vector search
        assert!(!results.is_empty());
        // Results should include the FTS5 hits
        let verse_refs: Vec<String> = results
            .iter()
            .map(|r| {
                format!(
                    "{} {}:{}",
                    r.detection.verse_ref.book_name,
                    r.detection.verse_ref.chapter,
                    r.detection.verse_ref.verse_start
                )
            })
            .collect();
        assert!(verse_refs.iter().any(|r| r.contains("John")));
    }

    #[test]
    fn test_pipeline_hybrid_with_fts_empty_fts() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![];

        let results = pipeline.process_hybrid_with_fts("test text", &fts_results);

        // Should return empty when no FTS5 results
        assert!(results.is_empty());
    }

    #[test]
    fn quoted_verse_text_with_misheard_word_surfaces_fts_match() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 19,
            book_name: "Psalms".to_string(),
            chapter: 23,
            verse: 1,
            rank: -24.0,
            is_broad_match: false,
            text: String::new(),
        }];

        let results = pipeline
            .process_hybrid_with_fts("the lord is my shepard I shall not want", &fts_results);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].detection.verse_ref.book_name, "Psalms");
        assert_eq!(results[0].detection.verse_ref.chapter, 23);
        assert_eq!(results[0].detection.verse_ref.verse_start, 1);
    }

    #[test]
    fn test_pipeline_hybrid_with_fts_confidence_decay() {
        // Earlier FTS ranks carry higher confidence than later ones. (Tested on
        // the pure function: the live path gates sub-rank-0 keyword hits by the
        // operator threshold, so they no longer all survive into the merge.)
        let rank0 = fts_confidence(0, -20.0, false);
        let rank3 = fts_confidence(3, -20.0, false);
        assert!(rank0 > rank3, "earlier ranks must score higher");
    }

    #[test]
    fn test_pipeline_hybrid_with_fts_caps_at_five() {
        let mut pipeline = DetectionPipeline::new();
        // Near-verbatim (excellent) BM25 ranks so all six clear the operator
        // threshold and the cap is what truncates the list to five.
        let fts_results = vec![
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 3,
                verse: 16,
                rank: -28.0,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 45,
                book_name: "Romans".to_string(),
                chapter: 8,
                verse: 28,
                rank: -27.0,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 1,
                book_name: "Genesis".to_string(),
                chapter: 1,
                verse: 1,
                rank: -26.0,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 19,
                book_name: "Psalms".to_string(),
                chapter: 23,
                verse: 1,
                rank: -25.0,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 23,
                book_name: "Isaiah".to_string(),
                chapter: 53,
                verse: 5,
                rank: -24.5,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 40,
                book_name: "Matthew".to_string(),
                chapter: 5,
                verse: 3,
                rank: -24.0,
                is_broad_match: false,
                text: String::new(),
            },
        ];

        let results =
            pipeline.process_hybrid_with_fts("test text with many references", &fts_results);

        assert_eq!(results.len(), LIVE_SEMANTIC_CAP);
    }

    #[test]
    fn test_pipeline_hybrid_dedup_fts_vector_overlap() {
        // When FTS5 and vector search find the same verse, only one
        // candidate is emitted (not duplicates).
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 43,
            book_name: "John".to_string(),
            chapter: 3,
            verse: 16,
            rank: -24.0,
            is_broad_match: false,
            text: String::new(),
        }];

        let results = pipeline.process_hybrid_with_fts("John three sixteen", &fts_results);

        // Since the semantic detector is a stub (no vector hits), we just
        // get FTS5-only results. But verify no duplicate verse_refs.
        let mut seen = std::collections::HashSet::new();
        for r in &results {
            let key = format!(
                "{}-{}-{}",
                r.detection.verse_ref.book_number,
                r.detection.verse_ref.chapter,
                r.detection.verse_ref.verse_start
            );
            assert!(
                seen.insert(key),
                "hybrid pipeline must not emit duplicate verse refs"
            );
        }
    }

    #[test]
    fn live_fts_floor_drops_keyword_noise_keeps_strong_matches() {
        // Calibrated against the real corpus: reference-command keyword noise
        // (e.g. "samuel verse one three") tops out around BM25 -11..-12, while
        // genuine verse-text matches are <= -16. The live floor must drop the
        // former and keep the latter.
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 3,
                verse: 16,
                rank: -24.0, // near-verbatim genuine match
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 23,
                book_name: "Isaiah".to_string(),
                chapter: 41,
                verse: 27,
                rank: -11.5, // keyword noise
                is_broad_match: false,
                text: String::new(),
            },
        ];

        let results = pipeline.process_hybrid_with_fts("god so loved the world", &fts_results);

        assert!(
            results
                .iter()
                .any(|r| r.detection.verse_ref.book_name == "John"),
            "strong match must survive the floor"
        );
        assert!(
            !results
                .iter()
                .any(|r| r.detection.verse_ref.book_name == "Isaiah"),
            "keyword-noise match below the floor must be dropped"
        );
    }

    #[test]
    fn live_fts_confidence_floors_only_near_verbatim_quotes() {
        let excellent = fts_confidence(0, -24.0, false);
        let broad_excellent_rank = fts_confidence(0, -24.0, true);
        let keyword_band = fts_confidence(0, -17.0, false);

        assert!(
            excellent >= FTS5_EXCELLENT_MATCH_CONFIDENCE,
            "excellent verse-text BM25 matches should score like near-verbatim quotes"
        );
        // Keyword-band matches keep their honest rank-derived score rather than
        // being floored up to a fixed "strong" confidence that masquerades as a
        // quote and bypasses the operator's semantic threshold.
        assert!(
            keyword_band < excellent,
            "keyword-band FTS matches must not masquerade as quote-strength"
        );
        assert!(
            broad_excellent_rank < excellent,
            "OR-tier FTS matches must not masquerade as quote-strength"
        );
        assert!(
            (keyword_band - FTS5_RANK0_CONFIDENCE).abs() < f64::EPSILON,
            "keyword-band rank-0 match scores its honest rank confidence"
        );
    }

    #[test]
    fn test_pipeline_hybrid_drops_weak_fts_below_rank_floor() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 3,
                verse: 16,
                rank: -24.0,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 1,
                book_name: "Genesis".to_string(),
                chapter: 1,
                verse: 1,
                rank: -11.0,
                is_broad_match: false,
                text: String::new(),
            },
        ];

        let results = pipeline.process_hybrid_with_fts("god so loved the world", &fts_results);

        assert!(results
            .iter()
            .any(|r| r.detection.verse_ref.book_name == "John"));
        assert!(!results
            .iter()
            .any(|r| r.detection.verse_ref.book_name == "Genesis"));
    }

    const DANIEL_4_27_KJV: &str = "Wherefore, O king, let my counsel be acceptable unto thee, and break off thy sins by righteousness, and thine iniquities by shewing mercy to the poor; if it may be a lengthening of thy tranquillity.";

    #[test]
    fn overlap_verified_quote_fires_despite_keyword_band_rank() {
        // Real sermon utterance: "Verse 27 …" framing + near-verbatim KJV
        // Daniel 4:27 with STT drift (Therefore/Wherefore, your/thy,
        // showing/shewing). The whole-fragment phrase/AND tiers miss, so the
        // hit arrives via the OR tier with a keyword-band rank — but almost
        // every word of the verse is present in the fragment, and that
        // evidence must carry it to quote-strength confidence.
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 27,
            book_name: "Daniel".to_string(),
            chapter: 4,
            verse: 27,
            rank: -11.0,
            is_broad_match: true,
            text: DANIEL_4_27_KJV.to_string(),
        }];

        let results = pipeline.process_hybrid_with_fts(
            "Verse 27. Remember we read it? Verse 27. Therefore, O king, let my counsel be acceptable unto thee. Break off your sins by righteousness and thy iniquities by showing mercy to the poor. It may be a lengthening of your tranquility.",
            &fts_results,
        );

        assert_eq!(results.len(), 1, "overlap-verified quote must survive");
        assert!(
            results[0].detection.confidence >= 0.92,
            "near-verbatim quote must reach fire confidence (got {:.2})",
            results[0].detection.confidence
        );
    }

    #[test]
    fn partial_quote_surfaces_as_hint_below_fire_threshold() {
        // Psalm 23:5 half-quoted and garbled ("absence of my enemies"):
        // enough overlap to show the operator a candidate, not enough to air.
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 19,
            book_name: "Psalms".to_string(),
            chapter: 23,
            verse: 5,
            rank: -9.0,
            is_broad_match: true,
            text: "Thou preparest a table before me in the presence of mine enemies: thou anointest my head with oil; my cup runneth over.".to_string(),
        }];

        let results = pipeline.process_hybrid_with_fts(
            "He prepares the table before me in the absence of my enemies. I eat in the presence.",
            &fts_results,
        );

        assert_eq!(
            results.len(),
            1,
            "partial quote must surface as a candidate"
        );
        let confidence = results[0].detection.confidence;
        assert!(
            (0.70..0.90).contains(&confidence),
            "partial quote is a hint, not a live fire (got {confidence:.2})"
        );
    }

    #[test]
    fn scattered_keyword_hit_is_not_boosted_by_overlap() {
        // Theme-laden sermon speech sharing a few words with a verse must not
        // gain quote-strength confidence.
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 43,
            book_name: "John".to_string(),
            chapter: 3,
            verse: 16,
            rank: -11.0,
            is_broad_match: true,
            text: "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.".to_string(),
        }];

        let results = pipeline.process_hybrid_with_fts(
            "god has been so good to our church family this whole year and we love this world",
            &fts_results,
        );

        assert!(
            results.is_empty(),
            "scattered keyword overlap must stay suppressed: {results:?}"
        );
    }

    #[test]
    fn short_verse_mention_is_not_boosted_by_overlap() {
        // Very short verses ("Jesus wept") reach high overlap fractions from
        // a single common word — they must not be quote-boosted.
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 43,
            book_name: "John".to_string(),
            chapter: 11,
            verse: 35,
            rank: -11.0,
            is_broad_match: true,
            text: "Jesus wept.".to_string(),
        }];

        let results = pipeline.process_hybrid_with_fts(
            "and jesus was there with the disciples that day",
            &fts_results,
        );

        assert!(
            results.is_empty(),
            "short-verse keyword mention must stay suppressed: {results:?}"
        );
    }

    #[test]
    fn short_verbatim_verse_earns_quote_overlap_confidence() {
        // Psalm 23:1 has only four content words (lord, shepherd, shall, want),
        // so a verse-vocabulary floor above four excluded it from quote overlap:
        // a verbatim quote fell back to a ~0.72 vector score and lost to
        // thematically similar shepherd verses (Ezekiel 34:10). A keyword-band
        // FTS rank means only quote overlap can carry it, and a fully spoken
        // short verse must reach fire strength.
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            rank: -9.0,
            book_number: 19,
            book_name: "Psalms".to_string(),
            chapter: 23,
            verse: 1,
            is_broad_match: true,
            text: "The LORD is my shepherd; I shall not want.".to_string(),
        }];

        let results = pipeline
            .process_hybrid_with_fts("the lord is my shepherd i shall not want", &fts_results);

        let psalm = results
            .iter()
            .find(|r| {
                r.detection.verse_ref.book_number == 19
                    && r.detection.verse_ref.chapter == 23
                    && r.detection.verse_ref.verse_start == 1
            })
            .expect("verbatim Psalm 23:1 must surface as a live candidate");
        assert!(
            psalm.detection.confidence >= 0.92,
            "verbatim short verse must reach quote-strength confidence (got {})",
            psalm.detection.confidence
        );
    }

    #[test]
    fn default_semantic_threshold_suppresses_live_fts_keyword_flood() {
        // Keyword coincidences on common words can land around BM25 -16..-17.
        // They must not surface at the default threshold; they are no longer
        // floored to a fixed high confidence that bypasses the threshold.
        let fts_results = vec![
            Bm25Result {
                book_number: 27,
                book_name: "Daniel".to_string(),
                chapter: 2,
                verse: 19,
                rank: -17.0,
                is_broad_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 23,
                book_name: "Isaiah".to_string(),
                chapter: 29,
                verse: 12,
                rank: -16.5,
                is_broad_match: false,
                text: String::new(),
            },
        ];

        let mut at_default = DetectionPipeline::new();
        let default_hits =
            at_default.process_hybrid_with_fts("god gives wisdom to the kings", &fts_results);
        assert!(
            default_hits.is_empty(),
            "default semantic threshold suppresses keyword-band FTS flood"
        );
    }
}
