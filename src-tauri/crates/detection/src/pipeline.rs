use std::collections::HashSet;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

/// Confidence for one uniquely identified contiguous quote span.
const EXACT_QUOTE_CONFIDENCE: f64 = 0.92;
/// Strong strict-tier BM25 matches remain visible for operator review, but
/// retrieval rank alone must stay well below quote-strength auto-live bands.
const FTS5_STRONG_REVIEW_RANK: f64 = -24.0;
const FTS5_STRONG_REVIEW_CONFIDENCE: f64 = 0.70;

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

/// A two-term event anchor is stronger than an isolated BM25 rank when live
/// retrieval has to choose which semantic candidates survive the cap. This
/// remains below direct-reference confidence and is still subject to the
/// semantic confirmation rules in the presentation workflow.
const EVENT_ANCHOR_CONFIDENCE: f64 = 0.94;

const LIVE_SEMANTIC_CAP: usize = 5;

/// Quote-overlap verification: how much of a candidate verse's content
/// vocabulary must appear in the spoken fragment before the overlap counts as
/// quote evidence. Guards (minimum matched words, minimum verse vocabulary)
/// keep short verses and scattered keyword coincidences from qualifying.
/// The verse-vocabulary floor sits at the matched-words floor so a fully
const QUOTE_OVERLAP_MIN_FRACTION: f64 = 0.28;
const QUOTE_OVERLAP_MIN_MATCHED: usize = 4;
const QUOTE_OVERLAP_MIN_VERSE_WORDS: usize = 4;
/// Bag-of-words overlap must include at least one ordered adjacent pair. This
/// rejects topical prose that happens to reuse many common verse words in a
/// different order while retaining lightly garbled quotations.
const QUOTE_OVERLAP_MIN_CONTIGUOUS_RUN: usize = 2;
const QUOTE_OVERLAP_FIRE_FRACTION: f64 = 0.56;
const QUOTE_OVERLAP_FIRE_CONFIDENCE: f64 = 0.90;
const QUOTE_OVERLAP_MAX_CONFIDENCE: f64 = 0.98;
/// Minimum contiguous spoken words that must appear in a verse before the
/// match counts as an exact quote. Five is too short: "from the foundation of
/// the world" (6 tokens with "the") is shared by Luke 11:50 and Revelation 13:8,
/// and used to crown the wrong verse. Seven captures "lamb slain from the
/// foundation of the world" while excluding the generic six-word tail.
const EXACT_QUOTE_MIN_WORDS: usize = 7;
/// A short exact span must account for most of the spoken fragment. Otherwise
/// ordinary framing plus one partial verse clause is useful operator evidence,
/// but not enough certainty to auto-display it.
const EXACT_QUOTE_MIN_FRAGMENT_PERCENT: usize = 85;
/// Long contiguous spans are independently distinctive even when the speaker
/// adds framing prose around them.
const EXACT_QUOTE_LONG_SPAN_WORDS: usize = 12;
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
        let total_started = Instant::now();
        let direct_started = Instant::now();
        let direct_results = self.direct.detect(text);
        let direct_ms = direct_started.elapsed().as_secs_f64() * 1_000.0;

        let semantic_started = Instant::now();
        let semantic_results = if text.split_whitespace().count() >= MIN_WORDS_FOR_VECTOR {
            self.semantic.detect(text)
        } else {
            vec![]
        };
        let semantic_ms = semantic_started.elapsed().as_secs_f64() * 1_000.0;

        let merge_started = Instant::now();
        let merged = self.merger.merge(direct_results, semantic_results);
        let merge_ms = merge_started.elapsed().as_secs_f64() * 1_000.0;
        log::info!(
            "[DETECT] path=process direct_ms={direct_ms:.2} semantic_ms={semantic_ms:.2} \
             fts_ms=0.00 merge_ms={merge_ms:.2} total_ms={:.2} results={}",
            total_started.elapsed().as_secs_f64() * 1_000.0,
            merged.len()
        );
        merged
    }

    /// Run only direct (regex/pattern) detection. Instant, no ONNX inference.
    /// Used during live transcription on every `is_final` fragment.
    pub fn process_direct(&mut self, text: &str) -> Vec<MergedDetection> {
        let total_started = Instant::now();
        let direct_started = Instant::now();
        let direct_results = self.direct.detect(text);
        let direct_ms = direct_started.elapsed().as_secs_f64() * 1_000.0;
        let merge_started = Instant::now();
        let merged = self.merger.merge(direct_results, vec![]);
        let merge_ms = merge_started.elapsed().as_secs_f64() * 1_000.0;
        log::debug!(
            "[DETECT] path=direct direct_ms={direct_ms:.2} semantic_ms=0.00 fts_ms=0.00 \
             merge_ms={merge_ms:.2} total_ms={:.2} results={}",
            total_started.elapsed().as_secs_f64() * 1_000.0,
            merged.len()
        );
        merged
    }

    /// Run only semantic (ONNX embedding) detection. Slow, 50-400ms.
    /// Used on `speech_final` only, in a background task.
    pub fn process_semantic(&mut self, text: &str) -> Vec<MergedDetection> {
        if text.split_whitespace().count() < MIN_WORDS_FOR_VECTOR {
            return vec![];
        }
        let total_started = Instant::now();
        let semantic_started = Instant::now();
        let semantic_results = self.semantic.detect(text);
        let semantic_ms = semantic_started.elapsed().as_secs_f64() * 1_000.0;
        let merge_started = Instant::now();
        let merged = self.merger.merge(vec![], semantic_results);
        let merge_ms = merge_started.elapsed().as_secs_f64() * 1_000.0;
        log::info!(
            "[DETECT] path=semantic direct_ms=0.00 semantic_ms={semantic_ms:.2} fts_ms=0.00 \
             merge_ms={merge_ms:.2} total_ms={:.2} results={}",
            total_started.elapsed().as_secs_f64() * 1_000.0,
            merged.len()
        );
        merged
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

    /// Promote candidates from one unambiguous spoken book before the live
    /// semantic cap. This is intentionally a boost, not a filter: a speaker
    /// can name a book while quoting a cross-reference from elsewhere.
    fn prioritize_spoken_book(&self, text: &str, merged: &mut [MergedDetection]) {
        let mut book_numbers = HashSet::new();
        for book_match in self.direct.find_book_mentions(text) {
            book_numbers.insert(book_match.book_number);
        }
        let mut numbers = book_numbers.into_iter();
        let Some(book_number) = numbers.next() else {
            return;
        };
        if numbers.next().is_some() {
            return;
        }
        merged.sort_by_key(|candidate| {
            usize::from(candidate.detection.verse_ref.book_number != book_number)
        });
    }

    /// Run hybrid semantic detection combining vector search with pre-fetched
    /// FTS5 BM25 results. Used by the real-time STT pipeline.
    ///
    /// FTS5-only results are added with rank-derived confidence. Vector and
    /// FTS5 overlap is collapsed into one boosted candidate.
    #[expect(
        clippy::too_many_lines,
        reason = "the hybrid path keeps vector, FTS, overlap, and live-cap gates together"
    )]
    pub fn process_hybrid_with_fts(
        &mut self,
        text: &str,
        fts_results: &[Bm25Result],
    ) -> Vec<MergedDetection> {
        let total_started = Instant::now();
        // Vector search needs enough words for meaningful embeddings;
        // FTS5 keyword matching works with fewer words. FTS query time is paid
        // by the caller (BibleDb); here `fts_ms` is hybrid merge of prefetched rows.
        let semantic_started = Instant::now();
        let mut semantic_detections = if text.split_whitespace().count() >= MIN_WORDS_FOR_VECTOR {
            self.semantic.detect(text)
        } else {
            vec![]
        };
        let semantic_ms = semantic_started.elapsed().as_secs_f64() * 1_000.0;

        if fts_results.is_empty() {
            for detection in &mut semantic_detections {
                detection.confidence = detection.confidence.min(VECTOR_ONLY_CONFIDENCE_CAP);
                if let DetectionSource::Semantic { similarity } = &mut detection.source {
                    *similarity = (*similarity).min(VECTOR_ONLY_CONFIDENCE_CAP);
                }
            }
            let merge_started = Instant::now();
            let mut merged = self.merger.merge(vec![], semantic_detections);
            self.prioritize_spoken_book(text, &mut merged);
            merged.truncate(LIVE_SEMANTIC_CAP);
            let merge_ms = merge_started.elapsed().as_secs_f64() * 1_000.0;
            log::info!(
                "[DETECT] path=hybrid_no_fts direct_ms=0.00 semantic_ms={semantic_ms:.2} \
                 fts_ms=0.00 merge_ms={merge_ms:.2} total_ms={:.2} results={}",
                total_started.elapsed().as_secs_f64() * 1_000.0,
                merged.len()
            );
            return merged;
        }

        let fts_started = Instant::now();

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
        let exact_phrase_keys = exact_quote_keys(text, fts_results);
        let mut fts_keys: HashSet<(i32, i32, i32)> = HashSet::new();

        for (rank, fts) in fts_results.iter().enumerate() {
            let Some((confidence, overlap_confidence)) =
                live_fts_candidate_confidence(text, fts, rank, &exact_phrase_keys)
            else {
                continue;
            };
            let key = (fts.book_number, fts.chapter, fts.verse);
            // Quote evidence already has its own calibrated ordering. Keep
            // concept reranking for paraphrase/event candidates only.
            let has_quote_evidence = overlap_confidence
                .is_some_and(|score| score >= QUOTE_OVERLAP_FIRE_CONFIDENCE)
                || exact_quote_confidence(&exact_phrase_keys, fts).is_some()
                || (exact_phrase_keys.len() <= 1
                    && short_quote_confidence(text, &fts.text).is_some());
            let concept_anchor = if has_quote_evidence {
                None
            } else {
                concept_anchor_confidence(text, &fts.text)
            };
            // The concept score is an internal rerank signal for paraphrase
            // and event candidates; quote candidates retain their evidence
            // confidence so they cannot be displaced by a generic anchor.
            let anchor_rank_score = if concept_anchor.is_some() {
                concept_anchor.unwrap_or(confidence)
            } else {
                confidence
            };
            let confidence = confidence
                .max(event_anchor_confidence(text, &fts.text).unwrap_or_default())
                .max(
                    concept_anchor
                        .map(|score| score.min(CONCEPT_ANCHOR_CONFIDENCE_CAP))
                        .unwrap_or_default(),
                );
            fts_keys.insert(key);
            if vector_keys.contains(&key) {
                if let Some(existing) = semantic_detections
                    .iter_mut()
                    .find(|detection| detection_verse_key(detection) == key)
                {
                    existing.confidence = (existing.confidence + OVERLAP_CONFIDENCE_BOOST)
                        .min(1.0)
                        .max(overlap_confidence.unwrap_or(0.0))
                        .max(confidence);
                    if let DetectionSource::Semantic { similarity } = &mut existing.source {
                        *similarity = (*similarity + OVERLAP_CONFIDENCE_BOOST)
                            .min(1.0)
                            .max(overlap_confidence.unwrap_or(0.0))
                            .max(anchor_rank_score);
                    }
                    existing.has_lexical_quote |= has_quote_evidence || fts.is_phrase_match;
                    existing.quote_coverage = existing
                        .quote_coverage
                        .max(overlap_confidence.unwrap_or(0.0));
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
                    similarity: anchor_rank_score.max(confidence),
                },
                transcript_snippet: snippet.clone(),
                detected_at: now,
                is_chapter_only: false,
                is_fuzzy_book: false,
                has_lexical_quote: has_quote_evidence || fts.is_phrase_match,
                quote_coverage: overlap_confidence.unwrap_or(0.0),
                candidate_margin: 1.0,
                utterance_id: None,
                is_final_utterance: false,
            });
            vector_keys.insert(key);
        }

        // Vector-only survivors (no FTS phrase/AND/OR corroboration) stay in
        // the review band rather than presenting as mid-80s confident fires.
        for detection in &mut semantic_detections {
            let key = detection_verse_key(detection);
            if fts_keys.contains(&key) {
                continue;
            }
            detection.confidence = detection.confidence.min(VECTOR_ONLY_CONFIDENCE_CAP);
            if let DetectionSource::Semantic { similarity } = &mut detection.source {
                *similarity = (*similarity).min(VECTOR_ONLY_CONFIDENCE_CAP);
            }
        }

        let fts_ms = fts_started.elapsed().as_secs_f64() * 1_000.0;

        // Gate every live candidate — FTS-derived and vector alike — by the
        // operator's semantic visibility threshold so raising the slider
        // actually suppresses keyword noise instead of letting FTS hits bypass.
        let merge_started = Instant::now();
        let mut merged = self.merger.merge(vec![], semantic_detections);
        self.prioritize_spoken_book(text, &mut merged);
        merged.truncate(LIVE_SEMANTIC_CAP);
        let merge_ms = merge_started.elapsed().as_secs_f64() * 1_000.0;
        log::info!(
            "[DETECT] path=hybrid direct_ms=0.00 semantic_ms={semantic_ms:.2} \
             fts_ms={fts_ms:.2} merge_ms={merge_ms:.2} total_ms={:.2} results={} fts_rows={}",
            total_started.elapsed().as_secs_f64() * 1_000.0,
            merged.len(),
            fts_results.len()
        );
        merged
    }

    /// Run a standalone semantic search query (for the search UI).
    pub fn semantic_search(&mut self, query: &str, k: usize) -> Vec<(i64, f64)> {
        self.semantic.search_query(query, k)
    }
}

/// Score one live FTS candidate, or `None` when it fails the live gates.
///
/// Quote-overlap verification comes first: a candidate whose verse text is
/// substantially present in the fragment is a spoken quote, no matter which
/// FTS tier surfaced it or how BM25 ranked it. Garbled STT breaks phrase/AND
/// tiers, so genuine near-verbatim quotes routinely arrive as keyword-band OR
/// hits. Returns the candidate confidence alongside the overlap confidence,
/// which the caller reuses when collapsing vector/FTS duplicates.
/// Minimum confidence for a non-broad (phrase/AND) FTS hit so verbatim
/// phrase evidence outranks a typical vector-only guess (~0.80–0.85).
const PHRASE_TIER_CONFIDENCE_FLOOR: f64 = 0.88;
/// Pure vector hits without FTS corroboration are capped. Calibration on the
/// closing sermon showed 80–89% was ~half wrong; mid-band topical fires
/// (Matthew 22:42 @86%, Psalms 52:9 @73%) were vector-only. Phrase/overlap
/// paths are unaffected.
const VECTOR_ONLY_CONFIDENCE_CAP: f64 = 0.79;

fn live_fts_candidate_confidence(
    text: &str,
    fts: &Bm25Result,
    rank: usize,
    exact_phrase_keys: &HashSet<(i32, i32, i32)>,
) -> Option<(f64, Option<f64>)> {
    let overlap_confidence = quote_overlap_confidence(text, &fts.text);
    let exact_phrase_confidence = exact_quote_confidence(exact_phrase_keys, fts);
    let short_quote_confidence = (exact_phrase_keys.len() <= 1)
        .then(|| short_quote_confidence(text, &fts.text))
        .flatten();
    // Candidates with verified quote evidence already have calibrated
    // overlap/phrase scores. Keep the generic concept anchor for
    // paraphrase/event candidates only so it cannot alter exact-quote
    // ordering or create a competing runner-up beside a direct quotation.
    let concept_anchor = if overlap_confidence
        .is_some_and(|score| score >= QUOTE_OVERLAP_FIRE_CONFIDENCE)
        || exact_phrase_confidence.is_some()
        || short_quote_confidence.is_some()
    {
        None
    } else {
        concept_anchor_confidence(text, &fts.text)
    };
    let mut rank_confidence = fts_confidence(rank, fts.rank, fts.is_broad_match);
    let distinctive_coverage = query_distinctive_content_coverage(text, &fts.text);
    let distinctive_query_count = distinctive_content_words(text).count();

    // Contiguous phrase-tier hits (and verified exact spoken spans) get a
    // floor above typical vector-only scores. When a candidate matches only an
    // isolated tail subphrase (e.g. "like a roaring lion") while missing major
    // spoken subjects (e.g. "devil"), its phrase floor scales with query coverage.
    if exact_phrase_confidence.is_some() {
        rank_confidence = rank_confidence.max(PHRASE_TIER_CONFIDENCE_FLOOR);
    } else if fts.is_phrase_match {
        if distinctive_coverage >= 0.75 || distinctive_query_count <= 2 {
            rank_confidence = rank_confidence.max(PHRASE_TIER_CONFIDENCE_FLOOR);
        } else {
            rank_confidence = rank_confidence.max(0.70 + 0.15 * distinctive_coverage);
        }
    }
    if distinctive_coverage >= 0.99 && distinctive_query_count >= 2 {
        rank_confidence = rank_confidence.max(PHRASE_TIER_CONFIDENCE_FLOOR);
    }
    let ai_review_candidate = is_indirect_request(text)
        || (text.split_whitespace().count() <= 6
            && shared_content_word_count(text, &fts.text) >= 2);
    let confidence = cap_pastoral_prayer_address_confidence(
        text,
        overlap_confidence
            .into_iter()
            .chain(exact_phrase_confidence)
            .chain(short_quote_confidence)
            .chain(concept_anchor.map(|score| score.min(CONCEPT_ANCHOR_CONFIDENCE_CAP)))
            .fold(rank_confidence, f64::max)
            .max(if ai_review_candidate { 0.70 } else { 0.0 }),
    );
    log::debug!(
        "[DET-SEMANTIC] FTS5 candidate idx={rank} bm25={:.3} {} {}:{} conf={:.0}% overlap={:?} anchor={:?} broad={}",
        fts.rank,
        fts.book_name,
        fts.chapter,
        fts.verse,
        confidence * 100.0,
        overlap_confidence,
        concept_anchor,
        fts.is_broad_match
    );
    if confidence < FTS5_MIN_CONFIDENCE {
        return None;
    }
    // The live rank floor exists to suppress keyword-band OR noise. Phrase and
    // AND hits are not keyword noise — a four-word quoted span often scores
    // only ~-11 BM25 on a long verse, which is above -13 and used to vanish
    // even when the words were spoken verbatim.
    if fts.is_broad_match
        && fts.rank > FTS5_LIVE_RANK_FLOOR
        && overlap_confidence.is_none()
        && exact_phrase_confidence.is_none()
        && concept_anchor.is_none()
        && !ai_review_candidate
    {
        return None;
    }
    Some((confidence, overlap_confidence))
}

fn is_indirect_request(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("show") && (normalized.contains("verse") || normalized.contains("passage"))
}

/// Return a bounded retrieval boost when the transcript names both an event
/// and its subject in the verse text. Without this, a correct FTS hit can be
/// ranked below generic vector/keyword hits and disappear at the five-item
/// live semantic cap. The boost only changes candidate ordering; it does not
/// bypass semantic visibility or auto-live confirmation gates.
fn event_anchor_confidence(query: &str, verse_text: &str) -> Option<f64> {
    let query = query.to_ascii_lowercase();
    let verse = verse_text.to_ascii_lowercase();
    let baptism_event = query.contains("baptiz")
        && query.contains("jesus")
        && verse.contains("baptiz")
        && verse.contains("jesus")
        // Matthew 3:13 expresses the speaker's event explicitly: Jesus comes
        // to Jordan/John to be baptized. Other passages mention baptism and
        // Jesus while describing a different event, so they must not receive
        // this retrieval priority.
        && (verse.contains("jordan") || verse.contains("unto john"));
    baptism_event.then_some(EVENT_ANCHOR_CONFIDENCE)
}

/// Score a candidate when several distinctive concepts from the spoken event
/// occur in the verse. This is deliberately reference-agnostic: it rewards a
/// subject/event/duration combination without naming a particular verse.
const CONCEPT_ANCHOR_MIN_SHARED_TERMS: usize = 3;
const CONCEPT_ANCHOR_BASE_CONFIDENCE: f64 = 0.72;
const CONCEPT_ANCHOR_CONFIDENCE_CAP: f64 = 0.88;
const CONCEPT_ANCHOR_MAX_RANK_SCORE: f64 = 0.96;

fn concept_anchor_confidence(query: &str, verse_text: &str) -> Option<f64> {
    let query_tokens = anchor_tokens(query);
    let verse_tokens = anchor_tokens(verse_text);
    if query_tokens.len() < CONCEPT_ANCHOR_MIN_SHARED_TERMS
        || verse_tokens.len() < CONCEPT_ANCHOR_MIN_SHARED_TERMS
    {
        return None;
    }

    let query_stems: Vec<String> = query_tokens
        .iter()
        .map(|token| stem_anchor_word(token))
        .collect();
    let verse_stems: HashSet<String> = verse_tokens
        .iter()
        .map(|token| stem_anchor_word(token))
        .collect();
    let shared: HashSet<String> = query_stems
        .iter()
        .filter(|stem| verse_stems.contains(*stem))
        .cloned()
        .collect();
    let verse_numbers = number_signatures(verse_text);
    let numeric_match = number_signatures(query)
        .iter()
        .any(|number| verse_numbers.contains(number));
    let shared_count = shared.len() + usize::from(numeric_match);
    if shared_count < CONCEPT_ANCHOR_MIN_SHARED_TERMS {
        return None;
    }

    // At least one non-generic term keeps ordinary theological wording from
    // becoming an anchor solely because it shares words such as "God" or
    // "love". A matched number also counts as distinctive evidence.
    let distinctive_terms = shared
        .iter()
        .filter(|stem| stem.chars().count() >= 5)
        .count()
        + usize::from(numeric_match);
    if distinctive_terms < 2 {
        return None;
    }

    let query_term_count = query_stems.iter().collect::<HashSet<_>>().len().max(1);
    #[expect(
        clippy::cast_precision_loss,
        reason = "transcript term counts are small"
    )]
    let coverage = shared_count as f64 / query_term_count as f64;
    // A long sermon sentence that happens to share a few topical words is not
    // a distinctive event. Numeric anchors can carry sparse framing because
    // the duration/quantity itself is a strong discriminator.
    if !numeric_match && coverage < 0.45 {
        return None;
    }
    let shared_score = f64::from(u32::try_from(shared_count.min(5)).unwrap_or(5));
    let distinctive_score = f64::from(u32::try_from(distinctive_terms.min(2)).unwrap_or(2));
    let mut confidence = CONCEPT_ANCHOR_BASE_CONFIDENCE
        + (shared_score * 0.025)
        + (distinctive_score * 0.025)
        + (coverage.min(0.8) * 0.04);
    if numeric_match {
        confidence += 0.04;
    }
    Some(confidence.min(CONCEPT_ANCHOR_MAX_RANK_SCORE))
}

fn anchor_tokens(text: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "about", "after", "all", "an", "and", "are", "as", "at", "be", "been", "before", "by",
        "can", "chapter", "come", "coming", "does", "for", "from", "has", "have", "he", "her",
        "him", "his", "i", "in", "into", "is", "it", "its", "more", "of", "on", "or", "passage",
        "please", "read", "says", "show", "some", "than", "that", "the", "their", "them", "there",
        "they", "this", "to", "talks", "verse", "was", "were", "where", "which", "with",
    ];

    text.split(|character: char| !character.is_alphanumeric())
        .filter_map(|word| {
            let lower = word.to_ascii_lowercase();
            (lower.chars().count() >= 3 && !STOP_WORDS.contains(&lower.as_str())).then_some(lower)
        })
        .collect()
}

fn stem_anchor_word(word: &str) -> String {
    let mut stem = word.to_ascii_lowercase();
    if stem.len() > 5 && stem.ends_with("ing") {
        stem.truncate(stem.len() - 3);
    } else if stem.len() > 4 && stem.ends_with("ed") {
        stem.truncate(stem.len() - 2);
    } else if stem.len() > 4 && stem.ends_with("ies") {
        stem.truncate(stem.len() - 3);
        stem.push('y');
    } else if stem.len() > 4 && stem.ends_with("es") {
        stem.truncate(stem.len() - 2);
    } else if stem.len() > 4 && stem.ends_with('s') {
        stem.truncate(stem.len() - 1);
    }
    stem
}

fn number_signatures(text: &str) -> HashSet<i32> {
    let tokens: Vec<String> = text
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|word| !word.is_empty())
        .collect();
    let mut signatures = HashSet::new();
    for (index, token) in tokens.iter().enumerate() {
        if let Ok(number) = token.parse::<i32>() {
            signatures.insert(number);
            continue;
        }
        let Some(number) = crate::direct::parser::parse_spoken_number(token) else {
            continue;
        };
        if matches!(
            tokens.get(index + 1).map(String::as_str),
            Some("hundred" | "honderd")
        ) && (1..=9).contains(&number)
        {
            signatures.insert(number * 100);
        } else {
            signatures.insert(number);
        }
    }
    signatures
}

fn shared_content_word_count(left: &str, right: &str) -> usize {
    let left: HashSet<String> = content_words(left).collect();
    let right: HashSet<String> = content_words(right).collect();
    left.intersection(&right).count()
}

fn short_quote_confidence(fragment: &str, verse_text: &str) -> Option<f64> {
    let verse_words: Vec<String> = content_words(verse_text).collect();
    fragment
        .split(['.', '!', '?'])
        .filter_map(|clause| {
            let clause_words: Vec<String> = content_words(clause).collect();
            if !(3..=6).contains(&clause_words.len()) {
                return None;
            }
            let shared = longest_ordered_shared_words(&clause_words, &verse_words);
            let distinctive_shared = clause_words
                .iter()
                .filter(|word| word.len() >= 7 && verse_words.contains(word))
                .count();
            let quote_like = shared >= 4 || (shared >= 3 && distinctive_shared >= 2);
            (quote_like && shared * 4 >= clause_words.len() * 3).then_some(0.92)
        })
        .max_by(f64::total_cmp)
}

fn longest_ordered_shared_words(left: &[String], right: &[String]) -> usize {
    let mut previous = vec![0usize; right.len() + 1];
    for left_word in left {
        let mut current = vec![0usize; right.len() + 1];
        for (index, right_word) in right.iter().enumerate() {
            if left_word == right_word {
                current[index + 1] = previous[index] + 1;
            } else {
                current[index + 1] = current[index].max(previous[index + 1]);
            }
        }
        previous = current;
    }
    previous[right.len()]
}

/// Measure what fraction of a verse's distinctive content vocabulary is
/// present in the spoken fragment, mapped onto hint-to-quote confidence.
/// `None` when the evidence is too thin to count (short verse, few matched
/// words, low fraction).
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
    let fragment_words: Vec<String> = content_words(fragment).collect();
    let verse_words: HashSet<String> = content_words(verse_text).collect();
    if verse_words.len() < QUOTE_OVERLAP_MIN_VERSE_WORDS {
        return None;
    }
    // Limit bag-of-words evidence to a local region. Without this bound,
    // unrelated words spread across a long STT block can assemble into a
    // short verse that was never spoken.
    let window_len = verse_words
        .len()
        .saturating_mul(2)
        .max(QUOTE_OVERLAP_MIN_MATCHED);
    // Track how much of the matching window the verse explains alongside how
    // much of the verse was spoken. A verse whose vocabulary nests inside a
    // longer quotation otherwise wins on verse-coverage alone: reading John
    // 3:16 in full covers ~96% of John 3:15's shorter text and outranked the
    // verse actually being read (2026-08-04, John 3:15 96% vs John 3:16 95%).
    let (matched, fragment_coverage) = fragment_words
        .windows(window_len.min(fragment_words.len()).max(1))
        .map(|window| {
            let local: HashSet<&str> = window.iter().map(String::as_str).collect();
            let hits = verse_words
                .iter()
                .filter(|word| local.contains(word.as_str()))
                .count();
            let coverage = if local.is_empty() {
                0.0
            } else {
                hits as f64 / local.len() as f64
            };
            (hits, coverage)
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .unwrap_or((0, 0.0));
    if matched < QUOTE_OVERLAP_MIN_MATCHED {
        return None;
    }
    if longest_shared_contiguous_word_run(fragment, verse_text) < QUOTE_OVERLAP_MIN_CONTIGUOUS_RUN {
        return None;
    }
    // Both directions must hold. Where the quotation and the verse are the same
    // span - the ordinary case - the two are near-identical and this is a no-op;
    // it only bites when the verse is a fragment of what was actually spoken.
    let fraction = (matched as f64 / verse_words.len() as f64).min(fragment_coverage);
    if fraction < QUOTE_OVERLAP_MIN_FRACTION && !is_exact_quote_fragment(fragment, verse_text) {
        return None;
    }
    let effective_fraction = if is_exact_quote_fragment(fragment, verse_text) {
        fraction.max(fragment_coverage)
    } else {
        fraction
    };
    // A barely qualifying overlap is a review hint; 0.56 reaches live
    // confidence. Above that boundary, retain overlap quality up to 0.98 so
    // the most complete quotation wins deterministic ranking.
    if effective_fraction <= QUOTE_OVERLAP_FIRE_FRACTION {
        return Some(0.52 + 0.68 * effective_fraction);
    }
    let high_overlap =
        (effective_fraction - QUOTE_OVERLAP_FIRE_FRACTION) / (1.0 - QUOTE_OVERLAP_FIRE_FRACTION);
    Some(
        QUOTE_OVERLAP_FIRE_CONFIDENCE
            + (QUOTE_OVERLAP_MAX_CONFIDENCE - QUOTE_OVERLAP_FIRE_CONFIDENCE) * high_overlap,
    )
}

fn longest_shared_contiguous_word_run(fragment: &str, verse_text: &str) -> usize {
    // Use the unfiltered word stream here. Removing short connectors would
    // manufacture adjacency (for example, "only ... those") that the speaker
    // never quoted, while real paraphrases still preserve ordinary pairs such
    // as "harm you" or "prosper you".
    let fragment_words = normalized_words(fragment);
    let verse_words = normalized_words(verse_text);
    let mut previous = vec![0usize; verse_words.len() + 1];
    let mut best = 0usize;

    for fragment_word in &fragment_words {
        let mut current = vec![0usize; verse_words.len() + 1];
        for (index, verse_word) in verse_words.iter().enumerate() {
            if fragment_word == verse_word {
                current[index + 1] = previous[index] + 1;
                best = best.max(current[index + 1]);
            }
        }
        previous = current;
    }
    best
}

fn is_exact_quote_fragment(fragment: &str, verse_text: &str) -> bool {
    let fragment_words = normalized_words(fragment);
    if fragment_words.len() < EXACT_QUOTE_MIN_WORDS {
        return false;
    }
    let verse_words = normalized_words(verse_text);
    // Speakers frame quotes with prose, but a short partial clause inside a
    // larger contextual utterance is hint evidence rather than a complete
    // quote. Accept either a long distinctive span or a short span comprising
    // most of the spoken fragment.
    let max_len = fragment_words.len().min(16);
    for len in (EXACT_QUOTE_MIN_WORDS..=max_len).rev() {
        for start in 0..=fragment_words.len() - len {
            let span = &fragment_words[start..start + len];
            let covers_most_of_fragment = len.saturating_mul(100)
                >= fragment_words
                    .len()
                    .saturating_mul(EXACT_QUOTE_MIN_FRAGMENT_PERCENT);
            if (len >= EXACT_QUOTE_LONG_SPAN_WORDS || covers_most_of_fragment)
                && verse_words.windows(len).any(|window| window == span)
            {
                return true;
            }
        }
    }
    false
}

fn exact_quote_keys(fragment: &str, fts_results: &[Bm25Result]) -> HashSet<(i32, i32, i32)> {
    fts_results
        .iter()
        .filter(|fts| {
            is_exact_quote_fragment(fragment, &fts.text)
                // A short ordered overlap is useful only when the FTS tier
                // identified the candidate narrowly. Broad OR hits can share
                // generic words such as "write this ... children" and must
                // remain review evidence instead of receiving quote strength.
                || (!fts.is_broad_match && short_quote_confidence(fragment, &fts.text).is_some())
        })
        .map(|fts| (fts.book_number, fts.chapter, fts.verse))
        .collect()
}

fn exact_quote_confidence(
    exact_phrase_keys: &HashSet<(i32, i32, i32)>,
    fts: &Bm25Result,
) -> Option<f64> {
    exact_phrase_keys
        .contains(&(fts.book_number, fts.chapter, fts.verse))
        .then_some(if exact_phrase_keys.len() == 1 {
            EXACT_QUOTE_CONFIDENCE
        } else {
            0.89
        })
}

fn normalized_words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub fn content_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= QUOTE_OVERLAP_MIN_WORD_LEN)
        .map(str::to_lowercase)
}

/// `content_words` paired with each word's byte offset in the source text.
/// Filtering and lowercasing must stay identical to `content_words` — callers
/// rely on the two producing the same sequence.
pub fn content_words_indexed(text: &str) -> impl Iterator<Item = (usize, String)> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= QUOTE_OVERLAP_MIN_WORD_LEN)
        .map(move |word| {
            let offset = word.as_ptr() as usize - text.as_ptr() as usize;
            (offset, word.to_lowercase())
        })
}

pub fn distinctive_content_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= 3 && !rhema_bible::is_stop_word(word))
        .map(str::to_lowercase)
}

#[expect(clippy::cast_precision_loss, reason = "word count is tiny")]
fn query_distinctive_content_coverage(query: &str, verse_text: &str) -> f64 {
    let query_words: Vec<String> = distinctive_content_words(query).collect();
    if query_words.is_empty() {
        return 1.0;
    }
    let verse_words: HashSet<String> = distinctive_content_words(verse_text).collect();
    let matched = query_words.iter().filter(|w| verse_words.contains(*w)).count();
    matched as f64 / query_words.len() as f64
}

#[expect(clippy::cast_precision_loss, reason = "rank index is small")]
fn fts_confidence(rank: usize, bm25_rank: f64, is_broad_match: bool) -> f64 {
    let rank_confidence = FTS5_RANK0_CONFIDENCE - (rank as f64 * FTS5_CONFIDENCE_DECAY);
    // BM25 rank measures retrieval relevance, not certainty that the speaker
    // quoted a verse. A strict-tier excellent rank may reach the operator's
    // review floor, while separate contiguous-span and overlap checks are the
    // only routes to quote-strength confidence.
    if bm25_rank <= FTS5_STRONG_REVIEW_RANK && !is_broad_match {
        rank_confidence.max(FTS5_STRONG_REVIEW_CONFIDENCE)
    } else {
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
            is_phrase_match: false,
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
    fn indirect_verse_request_keeps_a_weak_broad_candidate_for_ai_review() {
        let candidate = Bm25Result {
            rank: -5.0,
            book_number: 41,
            book_name: "Mark".to_string(),
            chapter: 4,
            verse: 39,
            is_broad_match: true,
            is_phrase_match: false,
            text: "And he rebuked the wind and said unto the sea Peace be still and there was a great calm".to_string(),
        };

        let scored = live_fts_candidate_confidence(
            "Please show the verse that talks about Jesus coming the storm in the boat",
            &candidate,
            0,
            &HashSet::new(),
        );

        assert!(
            scored.is_some_and(|(confidence, _)| confidence >= 0.70),
            "explicit indirect requests need review candidates for AI ranking: {scored:?}"
        );
    }

    #[test]
    fn distinctive_concept_anchor_raises_enoch_event_match() {
        let candidate = Bm25Result {
            rank: -10.0,
            book_number: 1,
            book_name: "Genesis".to_string(),
            chapter: 5,
            verse: 22,
            is_broad_match: false,
            is_phrase_match: false,
            text: "And Enoch walked with God after he begat Methuselah three hundred years, and begat sons and daughters".to_string(),
        };

        let scored = live_fts_candidate_confidence(
            "There is a verse about Enoch walking with God for more than 300 years",
            &candidate,
            0,
            &HashSet::new(),
        );

        assert!(
            scored.is_some_and(|(confidence, _)| confidence >= 0.80),
            "distinctive subject, event, and duration anchors should rerank the match: {scored:?}"
        );
    }

    #[test]
    fn concept_anchor_reranks_duration_and_event_over_duration_alone() {
        let mut pipeline = DetectionPipeline::new();
        let results = pipeline.process_hybrid_with_fts(
            "There is a verse about Enoch walking with God for more than 300 years",
            &[
                Bm25Result {
                    rank: -10.0,
                    book_number: 1,
                    book_name: "Genesis".to_string(),
                    chapter: 5,
                    verse: 23,
                    is_broad_match: false,
                    is_phrase_match: false,
                    text: "And all the days of Enoch were three hundred sixty and five years".to_string(),
                },
                Bm25Result {
                    rank: -10.0,
                    book_number: 1,
                    book_name: "Genesis".to_string(),
                    chapter: 5,
                    verse: 22,
                    is_broad_match: false,
                    is_phrase_match: false,
                    text: "And Enoch walked with God after he begat Methuselah three hundred years, and begat sons and daughters".to_string(),
                },
            ],
        );

        assert_eq!(
            results[0].detection.verse_ref.verse_start, 22,
            "the candidate matching subject, event, and duration must rank first"
        );
        assert!(
            results[0].detection.rank_score() > results[1].detection.rank_score(),
            "the distinctive event candidate needs a higher internal rerank score"
        );
        assert!(
            results[0].detection.confidence <= CONCEPT_ANCHOR_CONFIDENCE_CAP,
            "concept reranking must stay in the review band"
        );
    }

    #[test]
    fn generic_theological_overlap_does_not_create_a_concept_anchor() {
        assert_eq!(
            concept_anchor_confidence(
                "There is a verse about God's love for the world",
                "For God so loved the world, that he gave his only begotten Son"
            ),
            None
        );
    }

    #[test]
    fn two_matching_scene_names_keep_a_weak_broad_candidate_for_ai_review() {
        let candidate = Bm25Result {
            rank: -5.0,
            book_number: 44,
            book_name: "Acts".to_string(),
            chapter: 16,
            verse: 25,
            is_broad_match: true,
            is_phrase_match: false,
            text: "And at midnight Paul and Silas prayed and sang praises unto God and the prisoners heard them".to_string(),
        };

        let scored = live_fts_candidate_confidence(
            "Paul and Silas in prison",
            &candidate,
            0,
            &HashSet::new(),
        );

        assert!(
            scored.is_some_and(|(confidence, _)| confidence >= 0.70),
            "two matching scene names need review candidates for AI ranking: {scored:?}"
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
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 45,
                book_name: "Romans".to_string(),
                chapter: 5,
                verse: 8,
                rank: -24.0,
                is_broad_match: false,
                is_phrase_match: false,
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
            is_phrase_match: false,
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
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 45,
                book_name: "Romans".to_string(),
                chapter: 8,
                verse: 28,
                rank: -27.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 1,
                book_name: "Genesis".to_string(),
                chapter: 1,
                verse: 1,
                rank: -26.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 19,
                book_name: "Psalms".to_string(),
                chapter: 23,
                verse: 1,
                rank: -25.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 23,
                book_name: "Isaiah".to_string(),
                chapter: 53,
                verse: 5,
                rank: -24.5,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 40,
                book_name: "Matthew".to_string(),
                chapter: 5,
                verse: 3,
                rank: -24.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
        ];

        let results =
            pipeline.process_hybrid_with_fts("test text with many references", &fts_results);

        assert_eq!(results.len(), LIVE_SEMANTIC_CAP);
    }

    #[test]
    fn baptism_event_anchor_survives_live_candidate_cap() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 3,
                verse: 22,
                rank: -30.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "After these things came Jesus and his disciples into the land of Judaea."
                    .to_string(),
            },
            Bm25Result {
                book_number: 40,
                book_name: "Matthew".to_string(),
                chapter: 3,
                verse: 1,
                rank: -29.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "In those days came John the Baptist, preaching in the wilderness of Judaea."
                    .to_string(),
            },
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 4,
                verse: 1,
                rank: -28.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "When therefore the Lord knew how the Pharisees had heard that Jesus made and baptized more disciples than John."
                    .to_string(),
            },
            Bm25Result {
                book_number: 42,
                book_name: "Luke".to_string(),
                chapter: 3,
                verse: 21,
                rank: -27.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "Now when all the people were baptized, it came to pass, that Jesus also being baptized, and praying, the heaven was opened."
                    .to_string(),
            },
            Bm25Result {
                book_number: 45,
                book_name: "Romans".to_string(),
                chapter: 6,
                verse: 3,
                rank: -26.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "Know ye not, that so many of us as were baptized into Jesus Christ were baptized into his death?"
                    .to_string(),
            },
            Bm25Result {
                book_number: 40,
                book_name: "Matthew".to_string(),
                chapter: 3,
                verse: 13,
                rank: -18.0,
                is_broad_match: true,
                is_phrase_match: true,
                text: "Then cometh Jesus from Galilee to Jordan unto John, to be baptized of him."
                    .to_string(),
            },
        ];

        let results = pipeline.process_hybrid_with_fts(
            "the verse which talks about John the Baptist baptizing Jesus",
            &fts_results,
        );

        assert_eq!(results.len(), LIVE_SEMANTIC_CAP);
        assert!(results.iter().any(|result| {
            result.detection.verse_ref.book_name == "Matthew"
                && result.detection.verse_ref.chapter == 3
                && result.detection.verse_ref.verse_start == 13
        }));
    }

    #[test]
    fn spoken_book_is_boosted_before_live_candidate_cap() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 3,
                verse: 16,
                rank: -28.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 45,
                book_name: "Romans".to_string(),
                chapter: 8,
                verse: 28,
                rank: -27.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 1,
                book_name: "Genesis".to_string(),
                chapter: 1,
                verse: 1,
                rank: -26.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 19,
                book_name: "Psalms".to_string(),
                chapter: 23,
                verse: 1,
                rank: -25.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 23,
                book_name: "Isaiah".to_string(),
                chapter: 53,
                verse: 5,
                rank: -24.5,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 2,
                book_name: "Exodus".to_string(),
                chapter: 20,
                verse: 8,
                rank: -24.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: String::new(),
            },
        ];

        let results =
            pipeline.process_hybrid_with_fts("a reading from the book of Exodus", &fts_results);

        assert_eq!(results.len(), LIVE_SEMANTIC_CAP);
        assert_eq!(results[0].detection.verse_ref.book_number, 2);
        assert!(results
            .iter()
            .any(|result| { result.detection.verse_ref.book_name == "John" }));
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
            is_phrase_match: false,
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
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 23,
                book_name: "Isaiah".to_string(),
                chapter: 41,
                verse: 27,
                rank: -11.5, // keyword noise
                is_broad_match: false,
                is_phrase_match: false,
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
    fn live_fts_confidence_does_not_treat_rank_as_quote_evidence() {
        let excellent = fts_confidence(0, -24.0, false);
        let broad_excellent_rank = fts_confidence(0, -24.0, true);
        let keyword_band = fts_confidence(0, -17.0, false);

        assert!(
            (excellent - FTS5_STRONG_REVIEW_CONFIDENCE).abs() < f64::EPSILON,
            "a strong strict-tier BM25 rank may reach only review confidence"
        );
        // Keyword-band matches keep their honest rank-derived score rather than
        // being floored up to a fixed "strong" confidence that masquerades as a
        // quote and bypasses the operator's semantic threshold.
        assert!(
            keyword_band < excellent,
            "ordinary keyword-band matches must retain their rank confidence"
        );
        assert!(
            broad_excellent_rank < excellent,
            "broad matches must not receive the strict-tier review floor"
        );
        assert!(excellent < 0.90, "BM25 rank must not masquerade as a quote");
        assert!(
            (keyword_band - FTS5_RANK0_CONFIDENCE).abs() < f64::EPSILON,
            "keyword-band rank-0 match scores its honest rank confidence"
        );
    }

    #[test]
    fn strong_bm25_paraphrase_does_not_masquerade_as_a_quote() {
        let spoken = "God will wipe away every tear and there will be no more death or pain";
        let fts = Bm25Result {
            book_number: 66,
            book_name: "Revelation".to_string(),
            chapter: 21,
            verse: 4,
            rank: -38.250_856_466_548_43,
            is_broad_match: false,
            is_phrase_match: true,
            text: "He will wipe away every tear from their eyes. Death will be no more; neither will there be mourning, nor crying, nor pain any more. The first things have passed away.”".to_string(),
        };
        let exact_keys = exact_quote_keys(spoken, std::slice::from_ref(&fts));
        let confidence = live_fts_candidate_confidence(spoken, &fts, 0, &exact_keys)
            .expect("the paraphrase remains a useful below-threshold hint")
            .0;

        assert!(
            confidence < 0.90,
            "a BM25 rank alone is not quote evidence; got {confidence}"
        );
    }

    #[test]
    fn framing_words_keep_a_partial_quote_in_the_review_band() {
        let spoken = "Read verse 10, for Demas hath forsaken me, having loved this present world.";
        let verse = "For Demas hath forsaken me, having loved this present world, and is departed unto Thessalonica; Crescens to Galatia, Titus unto Dalmatia.";

        assert!(
            !is_exact_quote_fragment(spoken, verse),
            "a short partial quotation with contextual framing is hint evidence, not a confident complete quote"
        );
    }

    #[test]
    fn content_words_indexed_matches_content_words_sequence() {
        let text = "History, great conflict — between Christ & Satan.";
        let plain: Vec<String> = content_words(text).collect();
        let indexed: Vec<String> = content_words_indexed(text).map(|(_, w)| w).collect();
        assert_eq!(indexed, plain);
        let offsets: Vec<usize> = content_words_indexed(text).map(|(o, _)| o).collect();
        assert!(offsets.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn broad_matches_keep_their_rank_ordering() {
        // The old broad floor collapsed every strong OR hit to exactly 0.70.
        let first = fts_confidence(0, -20.0, true);
        let second = fts_confidence(1, -20.0, true);
        let third = fts_confidence(2, -20.0, true);

        assert!(
            first > second,
            "rank 0 must outscore rank 1: {first} vs {second}"
        );
        assert!(
            second > third,
            "rank 1 must outscore rank 2: {second} vs {third}"
        );
    }

    #[test]
    fn broad_evidence_alone_does_not_reach_the_default_operator_threshold() {
        // Broad OR-tier evidence may surface for review but must not read as a
        // confident detection at the 0.70 default.
        assert!(fts_confidence(0, -24.0, true) < 0.70);
    }

    #[test]
    fn scattered_common_words_do_not_verify_a_topical_vector_match() {
        let mut pipeline = DetectionPipeline::new();
        let mut semantic = SemanticDetector::new(
            Box::new(StubEmbedder::new(128)),
            Box::new(FakeIndex {
                results: vec![SearchResult {
                    verse_id: 43_017_020,
                    similarity: 0.81,
                }],
            }),
        );
        semantic.set_use_synonyms(false);
        pipeline.set_semantic(semantic);
        let spoken = "Only those whose minds are fortified with the word of God will be able to stand in these last days and go through to the heavenly Canaan.";
        let fts_results = vec![Bm25Result {
            book_number: 43,
            book_name: "John".to_string(),
            chapter: 17,
            verse: 20,
            rank: -14.369,
            is_broad_match: true,
            is_phrase_match: false,
            text: "Not for these only do I pray, but for those also who will believe in me through their word.".to_string(),
        }];

        assert_eq!(
            quote_overlap_confidence(spoken, &fts_results[0].text),
            None,
            "shared words in a different order are topical similarity, not quote evidence"
        );

        let results = pipeline.process_hybrid_with_fts(spoken, &fts_results);
        assert!(
            results.iter().all(|result| {
                result.detection.verse_ref.book_number != 43
                    || result.detection.verse_ref.chapter != 17
                    || result.detection.verse_ref.verse_start != 20
            }),
            "scattered common words must not surface John 17:20 as a Bible quote: {results:?}"
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
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 1,
                book_name: "Genesis".to_string(),
                chapter: 1,
                verse: 1,
                rank: -11.0,
                is_broad_match: false,
                is_phrase_match: false,
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
            is_phrase_match: false,
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
    fn quote_overlap_prefers_full_thessalonians_verse_over_shared_opening() {
        let fragment = "But I would not have you to be ignorant, brethren, concerning them \
                        which are asleep, that ye sorrow not even as others which have no hope.";
        let thessalonians = "But I would not have you to be ignorant, brethren, concerning them \
                            which are asleep, that ye sorrow not, even as others which have no hope.";
        let corinthians =
            "Now concerning spiritual gifts, brethren, I would not have you ignorant.";

        let expected = quote_overlap_confidence(fragment, thessalonians).unwrap();
        let distractor = quote_overlap_confidence(fragment, corinthians).unwrap();

        assert!(
            expected > distractor,
            "full verse overlap {expected} must outrank shared opening {distractor}"
        );
    }

    #[test]
    fn quote_overlap_prefers_complete_john_316_over_embedded_john_315() {
        let fragment = "For God so loved the world that he gave his only begotten son, so that \
                        whosoever believeth in him should not perish, but have everlasting life.";
        let john_316 = "For God so loved the world, that he gave his only begotten Son, that \
                        whosoever believeth in him should not perish, but have everlasting life.";
        let john_315 = "That whosoever believeth in him should not perish, but have eternal life.";

        let expected = quote_overlap_confidence(fragment, john_316).unwrap();
        let distractor = quote_overlap_confidence(fragment, john_315).unwrap();

        assert!(
            expected > distractor,
            "complete verse overlap {expected} must outrank embedded verse {distractor}"
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
            is_phrase_match: false,
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
        // Partial quotes may earn overlap boost; broad floor is gone so pure
        // rank confidence alone sits below 0.70. Keep the upper bound so this
        // never auto-fires as a high-confidence live hit.
        assert!(
            (0.50..0.90).contains(&confidence),
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
            is_phrase_match: false,
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
    fn distant_words_in_long_sermon_do_not_form_a_genesis_quote() {
        let transcript = "I know from someone I personally know the reason they kept on practicing the witchcraft and they kept on going back to the devil and asking him for power to cast spells on and to seduce people is because they were told by the devil, If you stop, I'm going to kill you and I'm going to kill your whole family. I have a word for you. The God of heaven is far greater than Satan. If you yield your life to Christ, Satan and his forces do not stand a chance. He always loses when confronted with";
        let genesis = "You know that I have served your father with all of my strength.";

        assert_eq!(
            quote_overlap_confidence(transcript, genesis),
            None,
            "distant topical words must not assemble into quote evidence"
        );
    }

    #[test]
    fn short_shared_phrase_stays_in_review_band() {
        let confidence = live_fts_candidate_confidence(
            "If you hear his voice, harden not your heart.",
            &Bm25Result {
                book_number: 19,
                book_name: "Psalms".to_string(),
                chapter: 95,
                verse: 8,
                rank: -11.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "Harden not your heart, as in the provocation.".to_string(),
            },
            0,
            &HashSet::new(),
        )
        .expect("phrase-tier quote")
        .0;

        assert!(
            (0.80..0.90).contains(&confidence),
            "short phrases shared across scripture need operator review: {confidence}"
        );
    }

    #[test]
    fn short_modernized_quote_reaches_live_threshold() {
        let confidence = short_quote_confidence(
            "We say, Lord, help me. I believe, but help my unbelief. Deliver me.",
            "And straightway the father of the child cried out, and said with tears, Lord, I believe; help thou mine unbelief.",
        );

        assert_eq!(confidence, Some(0.92));
    }

    #[test]
    fn broad_or_hit_does_not_become_exact_quote_evidence_from_short_overlap() {
        let fragment =
            "I write this to you, my children, that you do not sin. But if anyone sin, but that is only possible to those who have aligned their will.";
        let broad_or_hit = Bm25Result {
            book_number: 5,
            book_name: "Deuteronomy".to_string(),
            chapter: 31,
            verse: 19,
            rank: -10.603,
            is_broad_match: true,
            is_phrase_match: false,
            text: "Now therefore write this song for yourselves, and teach it to the children of Israel. Put it in their mouths, that this song may be a witness for me against the children of Israel.".to_string(),
        };

        let keys = exact_quote_keys(fragment, &[broad_or_hit]);

        assert!(
            !keys.contains(&(5, 31, 19)),
            "a broad OR hit must not receive exact-quote confidence from a short generic overlap"
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
            is_phrase_match: false,
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
            is_phrase_match: false,
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
                is_phrase_match: false,
                text: String::new(),
            },
            Bm25Result {
                book_number: 23,
                book_name: "Isaiah".to_string(),
                chapter: 29,
                verse: 12,
                rank: -16.5,
                is_broad_match: false,
                is_phrase_match: false,
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

    #[test]
    fn unique_short_exact_quote_reaches_live_confidence() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 43,
            book_name: "John".to_string(),
            chapter: 14,
            verse: 6,
            rank: -14.938,
            is_broad_match: false,
            is_phrase_match: false,
            text: "Jesus saith unto him, I am the way, the truth, and the life: no man cometh unto the Father, but by me.".to_string(),
        }];

        let results =
            pipeline.process_hybrid_with_fts("I am the way the truth and the life", &fts_results);
        let john = results
            .iter()
            .find(|result| result.detection.verse_ref.book_number == 43)
            .expect("a unique exact quotation must remain visible");

        assert!(
            john.detection.confidence >= 0.90,
            "a unique exact quotation must reach live confidence: {john:?}"
        );
    }

    #[test]
    fn unique_exact_quote_survives_the_keyword_rank_floor() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 19,
            book_name: "Psalms".to_string(),
            chapter: 46,
            verse: 10,
            rank: -12.104,
            is_broad_match: false,
            is_phrase_match: false,
            text: "Be still, and know that I am God: I will be exalted among the heathen, I will be exalted in the earth.".to_string(),
        }];

        let results =
            pipeline.process_hybrid_with_fts("Be still and know that I am God", &fts_results);

        assert!(
            results.iter().any(|result| {
                result.detection.verse_ref.book_number == 19
                    && result.detection.verse_ref.chapter == 46
                    && result.detection.verse_ref.verse_start == 10
                    && result.detection.confidence >= 0.90
            }),
            "exact contiguous evidence must supersede the keyword-only rank floor: {results:?}"
        );
    }

    #[test]
    fn exact_phrase_shared_by_multiple_verses_stays_below_live_confidence() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 14,
                verse: 1,
                rank: -18.209,
                is_broad_match: false,
                is_phrase_match: false,
                text: "Let not your heart be troubled: ye believe in God, believe also in me."
                    .to_string(),
            },
            Bm25Result {
                book_number: 43,
                book_name: "John".to_string(),
                chapter: 14,
                verse: 27,
                rank: -14.976,
                is_broad_match: false,
                is_phrase_match: false,
                text: "Peace I leave with you, my peace I give unto you: not as the world giveth, give I unto you. Let not your heart be troubled, neither let it be afraid.".to_string(),
            },
        ];

        let results =
            pipeline.process_hybrid_with_fts("Let not your heart be troubled", &fts_results);

        assert!(
            results
                .iter()
                .all(|result| result.detection.confidence < 0.90),
            "an exact phrase shared by multiple verses needs operator review: {results:?}"
        );
    }

    #[test]
    fn strong_broad_paraphrase_surfaces_only_as_review_hint() {
        let mut pipeline = DetectionPipeline::new();
        // After floor removal, rank-0 broad is 0.68 — below the default 0.70
        // operator threshold by design (see broad_evidence_alone_does_not_reach…).
        // Lower the review slider so the honest score is still visible.
        pipeline
            .merger_mut()
            .set_semantic_confidence_threshold(0.60);
        let fts_results = vec![Bm25Result {
            book_number: 42,
            book_name: "Luke".to_string(),
            chapter: 10,
            verse: 2,
            rank: -20.460,
            is_broad_match: true,
            is_phrase_match: false,
            text: "Then he said to them, The harvest is indeed plentiful, but the laborers are few. Pray therefore to the Lord of the harvest, that he may send out laborers into his harvest.".to_string(),
        }];

        let results = pipeline.process_hybrid_with_fts(
            "Is the harvest ready? What is the problem? The laborers. The harvest is all around us.",
            &fts_results,
        );
        let luke = results
            .iter()
            .find(|result| result.detection.verse_ref.book_number == 42)
            .expect("a strong broad match must remain visible for operator review");

        // Honest rank-derived confidence: review band, not live-fire.
        assert!(
            (0.50..0.70).contains(&luke.detection.confidence),
            "a broad paraphrase is a review hint below the 0.70 default: {luke:?}"
        );
    }

    #[test]
    fn strong_broad_paraphrase_stays_hidden_at_default_operator_threshold() {
        let mut pipeline = DetectionPipeline::new();
        let fts_results = vec![Bm25Result {
            book_number: 42,
            book_name: "Luke".to_string(),
            chapter: 10,
            verse: 2,
            rank: -20.460,
            is_broad_match: true,
            is_phrase_match: false,
            text: "Then he said to them, The harvest is indeed plentiful, but the laborers are few. Pray therefore to the Lord of the harvest, that he may send out laborers into his harvest.".to_string(),
        }];

        let results = pipeline.process_hybrid_with_fts(
            "Is the harvest ready? What is the problem? The laborers. The harvest is all around us.",
            &fts_results,
        );
        assert!(
            results
                .iter()
                .all(|r| r.detection.verse_ref.book_number != 42),
            "broad-only evidence must not clear the default 0.70 threshold without quote overlap"
        );
    }

    #[test]
    fn phrase_tier_hit_bypasses_live_rank_floor() {
        // Real phrase-tier BM25 for short quotes is often only ~-11 to -12,
        // above the -13 keyword floor. Phrase evidence must still surface.
        let conf = live_fts_candidate_confidence(
            "the lamb slain from the foundation of the world",
            &Bm25Result {
                book_number: 66,
                book_name: "Revelation".to_string(),
                chapter: 13,
                verse: 8,
                rank: -11.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "And all that dwell upon the earth shall worship him, whose names are not written in the book of life of the Lamb slain from the foundation of the world.".to_string(),
            },
            0,
            &HashSet::new(),
        );
        assert!(
            conf.is_some(),
            "phrase-tier hit must not be dropped by the keyword floor"
        );
    }

    #[test]
    fn verbatim_phrase_evidence_outranks_a_vector_only_band() {
        // Contiguous quoted span inside framing prose must clear the phrase
        // floor so it beats a typical 0.83 vector-only guess (Luke 11:50).
        let text = "I want that blood. The lamb slain from the foundation of the world.";
        let verse = "And all that dwell upon the earth shall worship him, whose names are not written in the book of life of the Lamb slain from the foundation of the world.";
        let fts = Bm25Result {
            book_number: 66,
            book_name: "Revelation".to_string(),
            chapter: 13,
            verse: 8,
            rank: -11.0,
            is_broad_match: false,
            is_phrase_match: true,
            text: verse.to_string(),
        };
        let keys = exact_quote_keys(text, std::slice::from_ref(&fts));
        let phrase = live_fts_candidate_confidence(text, &fts, 1, &keys)
            .expect("phrase")
            .0;
        assert!(
            phrase > 0.83 + 0.02,
            "verbatim phrase {phrase} must clearly beat a 0.83 vector-only guess"
        );
    }

    #[test]
    fn topical_vector_only_is_capped_below_confident_fire_band() {
        // No FTS results: pure vector path. Stub returns nothing, so inject via
        // hybrid with empty FTS and a synthetic semantic detector is heavy —
        // assert the constant contract instead and rely on harness for e2e.
        const {
            assert!(
                VECTOR_ONLY_CONFIDENCE_CAP < 0.80,
                "vector-only cap must keep topical hits out of the 80%+ fire band"
            );
        }
    }

    #[test]
    fn devil_roaring_lion_ranks_peter_over_ezekiel() {
        let mut pipeline = DetectionPipeline::new();
        let fts = vec![
            Bm25Result {
                book_number: 60,
                book_name: "1 Peter".to_string(),
                chapter: 5,
                verse: 8,
                rank: -12.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "Be sober, be vigilant; because your adversary the devil, as a roaring lion, walketh about, seeking whom he may devour:".to_string(),
            },
            Bm25Result {
                book_number: 26,
                book_name: "Ezekiel".to_string(),
                chapter: 22,
                verse: 25,
                rank: -10.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "There is a conspiracy of her prophets in the midst thereof, like a roaring lion ravening the prey; they have devoured souls;".to_string(),
            },
        ];

        let results = pipeline.process_hybrid_with_fts("the devil is like a roaring lion", &fts);
        assert!(
            !results.is_empty(),
            "must return candidate detections for roaring lion quote"
        );
        let top = &results[0];
        assert_eq!(
            top.detection.verse_ref.book_number, 60,
            "1 Peter 5:8 must rank first over Ezekiel 22:25, got {} {}:{}",
            top.detection.verse_ref.book_name,
            top.detection.verse_ref.chapter,
            top.detection.verse_ref.verse_start
        );
        assert_eq!(top.detection.verse_ref.chapter, 5);
        assert_eq!(top.detection.verse_ref.verse_start, 8);
    }

    #[test]
    fn generic_three_word_overlap_does_not_auto_live_as_a_short_quote() {
        let confidence = live_fts_candidate_confidence(
            "When lawlessness is in the church. These things are happening in the church.",
            &Bm25Result {
                book_number: 42,
                book_name: "Luke".to_string(),
                chapter: 21,
                verse: 31,
                rank: -10.0,
                is_broad_match: false,
                is_phrase_match: true,
                text: "So likewise ye, when ye see these things come to pass, know ye that the kingdom of God is nigh at hand.".to_string(),
            },
            0,
            &HashSet::new(),
        )
        .expect("the candidate remains visible for review");

        assert!(
            confidence.0 < QUOTE_OVERLAP_FIRE_CONFIDENCE,
            "generic three-word overlap must stay below auto-live confidence: {}",
            confidence.0
        );
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "comprehensive key verses test fixtures")]
    fn user_reported_key_verses_rank_correctly_under_distinctive_coverage() {
        let mut pipeline = DetectionPipeline::new();

        // 1. Genesis 6:5
        let gen_fts = vec![
            Bm25Result {
                book_number: 1,
                book_name: "Genesis".to_string(),
                chapter: 6,
                verse: 5,
                rank: -12.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "And GOD saw that the wickedness of man was great in the earth, and that every imagination of the thoughts of his heart was only evil continually.".to_string(),
            },
            Bm25Result {
                book_number: 1,
                book_name: "Genesis".to_string(),
                chapter: 8,
                verse: 21,
                rank: -11.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "the imagination of man's heart is evil from his youth;".to_string(),
            },
        ];
        let gen_res = pipeline.process_hybrid_with_fts("every imagination of the thoughts of his heart was only evil continually", &gen_fts);
        assert_eq!(gen_res[0].detection.verse_ref.book_number, 1);
        assert_eq!(gen_res[0].detection.verse_ref.chapter, 6);
        assert_eq!(gen_res[0].detection.verse_ref.verse_start, 5);

        // 2. Joshua 1:8
        let josh_fts = vec![
            Bm25Result {
                book_number: 6,
                book_name: "Joshua".to_string(),
                chapter: 1,
                verse: 8,
                rank: -13.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "This book of the law shall not depart out of thy mouth; but thou shalt meditate therein day and night, that thou mayest observe to do according to all that is written therein: for then thou shalt make thy way prosperous, and then thou shalt have good success.".to_string(),
            },
            Bm25Result {
                book_number: 19,
                book_name: "Psalms".to_string(),
                chapter: 1,
                verse: 2,
                rank: -10.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "But his delight is in the law of the LORD; and in his law doth he meditate day and night.".to_string(),
            },
        ];
        let josh_res = pipeline.process_hybrid_with_fts("this book of the law shall not depart out of thy mouth but thou shalt meditate therein day and night", &josh_fts);
        assert_eq!(josh_res[0].detection.verse_ref.book_number, 6);
        assert_eq!(josh_res[0].detection.verse_ref.chapter, 1);
        assert_eq!(josh_res[0].detection.verse_ref.verse_start, 8);

        // 3. Zechariah 1:3
        let zech_fts = vec![
            Bm25Result {
                book_number: 38,
                book_name: "Zechariah".to_string(),
                chapter: 1,
                verse: 3,
                rank: -11.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "Therefore say thou unto them, Thus saith the LORD of hosts; Turn ye unto me, saith the LORD of hosts, and I will turn unto you, saith the LORD of hosts.".to_string(),
            },
            Bm25Result {
                book_number: 39,
                book_name: "Malachi".to_string(),
                chapter: 3,
                verse: 7,
                rank: -10.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "Return unto me, and I will return unto you, saith the LORD of hosts.".to_string(),
            },
        ];
        let zech_res = pipeline.process_hybrid_with_fts("turn ye unto me saith the Lord of hosts and I will turn unto you", &zech_fts);
        assert_eq!(zech_res[0].detection.verse_ref.book_number, 38);
        assert_eq!(zech_res[0].detection.verse_ref.chapter, 1);
        assert_eq!(zech_res[0].detection.verse_ref.verse_start, 3);

        // 4. Jude 1:3
        let jude_fts = vec![
            Bm25Result {
                book_number: 65,
                book_name: "Jude".to_string(),
                chapter: 1,
                verse: 3,
                rank: -12.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "Beloved, when I gave all diligence to write unto you of the common salvation, it was needful for me to write unto you, and exhort you that ye should earnestly contend for the faith which was once delivered unto the saints.".to_string(),
            },
        ];
        let jude_res = pipeline.process_hybrid_with_fts("earnestly contend for the faith which was once delivered unto the saints", &jude_fts);
        assert_eq!(jude_res[0].detection.verse_ref.book_number, 65);
        assert_eq!(jude_res[0].detection.verse_ref.chapter, 1);
        assert_eq!(jude_res[0].detection.verse_ref.verse_start, 3);

        // 5. Psalm 23:1
        let ps_fts = vec![
            Bm25Result {
                book_number: 19,
                book_name: "Psalms".to_string(),
                chapter: 23,
                verse: 1,
                rank: -10.0,
                is_broad_match: false,
                is_phrase_match: false,
                text: "The LORD is my shepherd; I shall not want.".to_string(),
            },
        ];
        let ps_res = pipeline.process_hybrid_with_fts("the Lord is my shepherd I shall not want", &ps_fts);
        assert_eq!(ps_res[0].detection.verse_ref.book_number, 19);
        assert_eq!(ps_res[0].detection.verse_ref.chapter, 23);
        assert_eq!(ps_res[0].detection.verse_ref.verse_start, 1);
    }
}
