use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::types::{Detection, DetectionSource};

/// Default confidence threshold — detections below this are dropped.
const DEFAULT_CONFIDENCE_THRESHOLD: f64 = 0.45;

/// Default semantic visibility threshold - semantic suggestions below this are dropped.
const DEFAULT_SEMANTIC_CONFIDENCE_THRESHOLD: f64 = 0.70;

/// Default auto-queue threshold — detections above this are auto-queued.
const DEFAULT_AUTO_QUEUE_THRESHOLD: f64 = 0.98;

/// Default cooldown in milliseconds between auto-displayed results.
const DEFAULT_COOLDOWN_MS: u64 = 2500;

/// A detection after merging, with an auto-queue flag.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedDetection {
    pub detection: Detection,
    pub auto_queued: bool,
}

#[derive(Clone, Default)]
pub struct AutoQueueCooldown {
    last_auto_display: Arc<Mutex<Option<Instant>>>,
}

/// Merges results from direct reference detection and semantic search
/// into a single ranked list.
///
/// # Dedup strategy
/// When both direct and semantic detectors match the same verse
/// (same `book_number` + `chapter` + `verse_start`), the direct detection
/// is kept because it has higher trust (confidence >= 0.90).
///
/// # Auto-queue
/// High-confidence results are marked `auto_queued = true` so the UI
/// can display them immediately. A cooldown timer prevents flooding
/// the user with too many auto-displayed results.
pub struct DetectionMerger {
    confidence_threshold: f64,
    semantic_confidence_threshold: f64,
    auto_queue_threshold: f64,
    cooldown_ms: u64,
    cooldown: AutoQueueCooldown,
}

impl DetectionMerger {
    pub fn new() -> Self {
        Self::with_cooldown(AutoQueueCooldown::default())
    }

    pub fn with_cooldown(cooldown: AutoQueueCooldown) -> Self {
        Self {
            confidence_threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            semantic_confidence_threshold: DEFAULT_SEMANTIC_CONFIDENCE_THRESHOLD,
            auto_queue_threshold: DEFAULT_AUTO_QUEUE_THRESHOLD,
            cooldown_ms: DEFAULT_COOLDOWN_MS,
            cooldown,
        }
    }

    /// Merge direct and semantic detections into a ranked list.
    ///
    /// 1. Combine all detections.
    /// 2. Dedup: if direct and semantic found the same verse, keep direct.
    /// 3. Sort direct references before semantic suggestions, then by confidence.
    /// 4. Drop anything below its source-specific visibility threshold.
    /// 5. Mark `auto_queued = true` for the highest-ranked eligible item
    ///    (only one auto-queue per merge pass).
    /// 6. Apply cooldown: if last auto-display was < `cooldown_ms` ago,
    ///    don't auto-queue.
    pub fn merge(
        &mut self,
        direct: Vec<Detection>,
        semantic: Vec<Detection>,
    ) -> Vec<MergedDetection> {
        // 1. Combine
        let mut all: Vec<Detection> = Vec::with_capacity(direct.len() + semantic.len());
        all.extend(direct);
        all.extend(semantic);

        // 2. Dedup same-verse detections. Direct references dominate semantic
        //    detections; same-source duplicates keep the higher confidence.
        let mut deduped: Vec<Detection> = Vec::with_capacity(all.len());
        for detection in all {
            if let Some(existing) = deduped
                .iter_mut()
                .find(|existing| same_verse(existing, &detection))
            {
                if should_replace(existing, &detection) {
                    *existing = detection;
                }
            } else {
                deduped.push(detection);
            }
        }

        // 3. Direct references are more trustworthy than semantic suggestions.
        deduped.sort_by(|a, b| {
            source_priority(b).cmp(&source_priority(a)).then_with(|| {
                b.rank_score()
                    .partial_cmp(&a.rank_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        // 4. Drop below source-specific visibility thresholds.
        deduped.retain(|d| d.confidence >= self.visibility_threshold(d));

        // 5 & 6. Build merged list with auto-queue decisions.
        // Only the highest-ranked eligible detection per merge pass can auto-queue.
        let now = Instant::now();
        let mut last_auto_display = match self.cooldown.last_auto_display.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                log::error!("[DET] Auto-queue cooldown lock poisoned; recovering");
                poisoned.into_inner()
            }
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "cooldown millis won't exceed u64"
        )]
        let cooldown_ok = match *last_auto_display {
            Some(last) => now.duration_since(last).as_millis() as u64 >= self.cooldown_ms,
            None => true,
        };

        let mut auto_queue_used = false;
        let mut results = Vec::with_capacity(deduped.len());
        for detection in deduped {
            // Only direct references auto-queue. Semantic suggestions are
            // operator review hints, not display-worthy on their own.
            //
            // Chapter-only placeholders (verse defaults to 1, e.g. "Revelation
            // chapter 20" before "verse 12" is spoken) must not auto-queue —
            // they would jump the queue/reading to the wrong verse. They still
            // surface as an operator hint and are refined once the real verse
            // arrives.
            //
            // Single-digit *Bible* full citations are stripped of auto_q later
            // in the live direct path (content_type == "bible") so EGW page
            // paragraphs that reuse this merge helper keep working.
            let eligible = detection.confidence >= self.auto_queue_threshold
                && cooldown_ok
                && matches!(detection.source, DetectionSource::DirectReference)
                && !detection.is_chapter_only
                && !detection.is_fuzzy_book;
            let auto_queued = eligible && !auto_queue_used;
            if auto_queued {
                auto_queue_used = true;
                *last_auto_display = Some(now);
            }
            results.push(MergedDetection {
                detection,
                auto_queued,
            });
        }

        results
    }

    /// Update the minimum confidence threshold.
    pub fn set_confidence_threshold(&mut self, threshold: f64) {
        self.confidence_threshold = threshold;
    }

    /// Update the minimum confidence threshold for semantic suggestions.
    pub fn set_semantic_confidence_threshold(&mut self, threshold: f64) {
        self.semantic_confidence_threshold = threshold;
    }

    /// Update the auto-queue threshold.
    pub fn set_auto_queue_threshold(&mut self, threshold: f64) {
        self.auto_queue_threshold = threshold;
    }

    /// Update the cooldown between auto-displayed results.
    pub fn set_cooldown_ms(&mut self, ms: u64) {
        self.cooldown_ms = ms;
    }

    pub fn confidence_threshold(&self) -> f64 {
        self.confidence_threshold
    }

    pub fn semantic_confidence_threshold(&self) -> f64 {
        self.semantic_confidence_threshold
    }

    pub fn auto_queue_threshold(&self) -> f64 {
        self.auto_queue_threshold
    }

    fn visibility_threshold(&self, detection: &Detection) -> f64 {
        match detection.source {
            DetectionSource::DirectReference => self.confidence_threshold,
            DetectionSource::Semantic { .. } => self.semantic_confidence_threshold,
        }
    }
}

impl Default for DetectionMerger {
    fn default() -> Self {
        Self::new()
    }
}

fn same_verse(a: &Detection, b: &Detection) -> bool {
    if let (Some(a_id), Some(b_id)) = (a.verse_id, b.verse_id) {
        return a_id == b_id;
    }

    match (verse_ref_key(a), verse_ref_key(b)) {
        (Some(a_key), Some(b_key)) => a_key == b_key,
        _ => false,
    }
}

fn verse_ref_key(detection: &Detection) -> Option<(i32, i32, i32)> {
    let ref_ = &detection.verse_ref;
    (ref_.book_number > 0 && ref_.chapter > 0 && ref_.verse_start > 0).then_some((
        ref_.book_number,
        ref_.chapter,
        ref_.verse_start,
    ))
}

fn should_replace(existing: &Detection, incoming: &Detection) -> bool {
    let existing_direct = matches!(existing.source, DetectionSource::DirectReference);
    let incoming_direct = matches!(incoming.source, DetectionSource::DirectReference);

    match (existing_direct, incoming_direct) {
        (false, true) => true,
        (true, false) => false,
        _ => incoming.rank_score() > existing.rank_score(),
    }
}

fn source_priority(detection: &Detection) -> u8 {
    u8::from(matches!(detection.source, DetectionSource::DirectReference))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DetectionSource, VerseRef};

    fn make_detection(
        book_number: i32,
        book_name: &str,
        chapter: i32,
        verse_start: i32,
        confidence: f64,
        source: DetectionSource,
    ) -> Detection {
        Detection {
            verse_ref: VerseRef {
                book_number,
                book_name: book_name.to_string(),
                chapter,
                verse_start,
                verse_end: None,
            },
            verse_id: None,
            confidence,
            source,
            transcript_snippet: format!("{book_name} {chapter}:{verse_start}"),
            detected_at: 0,
            is_chapter_only: false,
            ..Detection::default()
        }
    }

    fn make_semantic_id_detection(verse_id: i64, confidence: f64) -> Detection {
        Detection {
            verse_ref: VerseRef {
                book_number: 0,
                book_name: String::new(),
                chapter: 0,
                verse_start: 0,
                verse_end: None,
            },
            verse_id: Some(verse_id),
            confidence,
            source: DetectionSource::Semantic {
                similarity: confidence,
            },
            transcript_snippet: format!("semantic hit {verse_id}"),
            detected_at: 0,
            is_chapter_only: false,
            ..Detection::default()
        }
    }

    #[test]
    fn test_merger_dedup_keeps_direct() {
        let mut merger = DetectionMerger::new();

        let direct = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.96,
            DetectionSource::DirectReference,
        )];
        let semantic = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.72,
            DetectionSource::Semantic { similarity: 0.72 },
        )];

        let results = merger.merge(direct, semantic);
        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0].detection.source,
            DetectionSource::DirectReference
        ));
        assert!((results[0].detection.confidence - 0.96).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merger_dedups_semantic_same_verse_keeps_higher_confidence() {
        let mut merger = DetectionMerger::new();

        let semantic = vec![
            make_detection(
                43,
                "John",
                3,
                16,
                0.62,
                DetectionSource::Semantic { similarity: 0.62 },
            ),
            make_detection(
                43,
                "John",
                3,
                16,
                0.78,
                DetectionSource::Semantic { similarity: 0.78 },
            ),
        ];

        let results = merger.merge(vec![], semantic);
        assert_eq!(results.len(), 1);
        assert!((results[0].detection.confidence - 0.78).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merger_keeps_unresolved_semantic_verse_ids_distinct() {
        let mut merger = DetectionMerger::new();

        let semantic = vec![
            make_semantic_id_detection(1001, 0.72),
            make_semantic_id_detection(1002, 0.81),
            make_semantic_id_detection(1003, 0.70),
        ];

        let results = merger.merge(vec![], semantic);

        assert_eq!(results.len(), 3);
        let ids: Vec<Option<i64>> = results.iter().map(|r| r.detection.verse_id).collect();
        assert_eq!(ids, vec![Some(1002), Some(1001), Some(1003)]);
    }

    #[test]
    fn test_merger_dedups_matching_semantic_verse_ids() {
        let mut merger = DetectionMerger::new();

        let semantic = vec![
            make_semantic_id_detection(1001, 0.72),
            make_semantic_id_detection(1001, 0.81),
        ];

        let results = merger.merge(vec![], semantic);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].detection.verse_id, Some(1001));
        assert!((results[0].detection.confidence - 0.81).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merger_direct_dominates_semantic_even_if_semantic_confidence_is_higher() {
        let mut merger = DetectionMerger::new();

        let direct = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.90,
            DetectionSource::DirectReference,
        )];
        let semantic = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.99,
            DetectionSource::Semantic { similarity: 0.99 },
        )];

        let results = merger.merge(direct, semantic);
        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0].detection.source,
            DetectionSource::DirectReference
        ));
        assert!((results[0].detection.confidence - 0.90).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merger_keeps_distinct_verses() {
        let mut merger = DetectionMerger::new();

        let direct = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.96,
            DetectionSource::DirectReference,
        )];
        let semantic = vec![make_detection(
            45,
            "Romans",
            8,
            28,
            0.70,
            DetectionSource::Semantic { similarity: 0.70 },
        )];

        let results = merger.merge(direct, semantic);
        assert_eq!(results.len(), 2);
        // Sorted by confidence descending
        assert_eq!(results[0].detection.verse_ref.book_name, "John");
        assert_eq!(results[1].detection.verse_ref.book_name, "Romans");
    }

    #[test]
    fn test_merger_direct_ranks_above_stronger_distinct_semantic_result() {
        let mut merger = DetectionMerger::new();

        let direct = vec![make_detection(
            27,
            "Daniel",
            7,
            9,
            0.88,
            DetectionSource::DirectReference,
        )];
        let semantic = vec![make_detection(
            66,
            "Revelation",
            20,
            12,
            0.99,
            DetectionSource::Semantic { similarity: 0.99 },
        )];

        let results = merger.merge(direct, semantic);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].detection.verse_ref.book_name, "Daniel");
        assert_eq!(results[1].detection.verse_ref.book_name, "Revelation");
    }

    #[test]
    fn test_merger_drops_below_threshold() {
        let mut merger = DetectionMerger::new();

        let direct = vec![];
        let semantic = vec![
            make_detection(
                43,
                "John",
                3,
                16,
                0.70,
                DetectionSource::Semantic { similarity: 0.70 },
            ),
            make_detection(
                45,
                "Romans",
                8,
                28,
                0.69,
                DetectionSource::Semantic { similarity: 0.69 },
            ),
        ];

        let results = merger.merge(direct, semantic);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].detection.verse_ref.book_name, "John");
    }

    #[test]
    fn test_merger_uses_separate_direct_and_semantic_visibility_thresholds() {
        let mut merger = DetectionMerger::new();
        merger.set_confidence_threshold(0.70);
        merger.set_semantic_confidence_threshold(0.65);

        let direct = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.69,
            DetectionSource::DirectReference,
        )];
        let semantic = vec![
            make_detection(
                45,
                "Romans",
                8,
                28,
                0.66,
                DetectionSource::Semantic { similarity: 0.66 },
            ),
            make_detection(
                66,
                "Revelation",
                21,
                4,
                0.64,
                DetectionSource::Semantic { similarity: 0.64 },
            ),
        ];

        let results = merger.merge(direct, semantic);

        assert!((merger.confidence_threshold() - 0.70).abs() < f64::EPSILON);
        assert!((merger.semantic_confidence_threshold() - 0.65).abs() < f64::EPSILON);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].detection.verse_ref.book_name, "Romans");
    }

    #[test]
    fn test_merger_auto_queue() {
        let mut merger = DetectionMerger::new();

        let direct = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.99,
            DetectionSource::DirectReference,
        )];

        let results = merger.merge(direct, vec![]);
        assert_eq!(results.len(), 1);
        // 0.99 >= 0.98 auto_queue_threshold and no cooldown yet
        assert!(results[0].auto_queued);
    }

    #[test]
    fn chapter_only_direct_detection_does_not_auto_queue() {
        // "Revelation chapter 20" (before the verse is spoken) surfaces a
        // chapter-only placeholder at 0.88 — verse defaults to 1. It must NOT
        // auto-queue, or the operator's queue/reading jumps to the wrong verse
        // (Revelation 20:1) until the real verse ("verse 12") arrives. It stays
        // visible as an operator hint (>= operator threshold), just not queued.
        let mut merger = DetectionMerger::new();

        let mut chapter_only = make_detection(
            66,
            "Revelation",
            20,
            1,
            0.88,
            DetectionSource::DirectReference,
        );
        chapter_only.is_chapter_only = true;

        let results = merger.merge(vec![chapter_only], vec![]);
        assert_eq!(results.len(), 1, "chapter-only result still surfaces");
        assert!(
            !results[0].auto_queued,
            "chapter-only placeholder (verse 1) must not auto-queue"
        );
    }

    #[test]
    fn test_merger_semantic_never_auto_queues_even_above_threshold() {
        let mut merger = DetectionMerger::new();

        let semantic = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.99,
            DetectionSource::Semantic { similarity: 0.99 },
        )];

        let results = merger.merge(vec![], semantic);
        assert_eq!(results.len(), 1);
        // 0.99 >= 0.98 auto_queue_threshold, but semantic is auto-queue-ineligible.
        assert!(!results[0].auto_queued);
    }

    #[test]
    fn test_merger_auto_queue_below_threshold() {
        let mut merger = DetectionMerger::new();

        let semantic = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.70,
            DetectionSource::Semantic { similarity: 0.70 },
        )];

        let results = merger.merge(vec![], semantic);
        assert_eq!(results.len(), 1);
        // 0.70 < 0.98 auto_queue_threshold
        assert!(!results[0].auto_queued);
    }

    #[test]
    fn test_merger_sort_order_prioritizes_direct_then_confidence() {
        let mut merger = DetectionMerger::new();

        let direct = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.90,
            DetectionSource::DirectReference,
        )];
        let semantic = vec![
            make_detection(
                45,
                "Romans",
                8,
                28,
                0.95,
                DetectionSource::Semantic { similarity: 0.95 },
            ),
            make_detection(
                1,
                "Genesis",
                1,
                1,
                0.70,
                DetectionSource::Semantic { similarity: 0.70 },
            ),
        ];

        let results = merger.merge(direct, semantic);
        assert_eq!(results.len(), 3);
        assert!(matches!(
            results[0].detection.source,
            DetectionSource::DirectReference
        ));
        assert!((results[0].detection.confidence - 0.90).abs() < f64::EPSILON);
        assert!((results[1].detection.confidence - 0.95).abs() < f64::EPSILON);
        assert!((results[2].detection.confidence - 0.70).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merger_empty_inputs() {
        let mut merger = DetectionMerger::new();
        let results = merger.merge(vec![], vec![]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_merger_only_one_auto_queue_per_batch() {
        let mut merger = DetectionMerger::new();

        let direct = vec![
            make_detection(43, "John", 3, 16, 0.99, DetectionSource::DirectReference),
            make_detection(45, "Romans", 8, 28, 0.98, DetectionSource::DirectReference),
        ];
        let semantic = vec![make_detection(
            1,
            "Genesis",
            1,
            1,
            0.85,
            DetectionSource::Semantic { similarity: 0.85 },
        )];

        let results = merger.merge(direct, semantic);
        assert_eq!(results.len(), 3);
        // Only the highest-ranked (John 3:16) should be auto-queued.
        let auto_queued_count = results.iter().filter(|r| r.auto_queued).count();
        assert_eq!(auto_queued_count, 1);
        assert!(results[0].auto_queued);
        assert!(!results[1].auto_queued);
        assert!(!results[2].auto_queued);
    }

    #[test]
    fn test_merger_shared_cooldown_blocks_second_merger_auto_queue() {
        let cooldown = AutoQueueCooldown::default();
        let mut direct_merger = DetectionMerger::with_cooldown(cooldown.clone());
        let mut semantic_merger = DetectionMerger::with_cooldown(cooldown);

        let direct = vec![make_detection(
            43,
            "John",
            3,
            16,
            0.99,
            DetectionSource::DirectReference,
        )];
        let semantic = vec![make_detection(
            45,
            "Romans",
            8,
            28,
            0.95,
            DetectionSource::Semantic { similarity: 0.95 },
        )];

        let direct_results = direct_merger.merge(direct, vec![]);
        let semantic_results = semantic_merger.merge(vec![], semantic);

        assert!(direct_results[0].auto_queued);
        assert!(!semantic_results[0].auto_queued);
    }
}
