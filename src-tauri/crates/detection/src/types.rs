use serde::{Deserialize, Serialize};

/// A reference to a specific Bible verse or verse range.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct VerseRef {
    pub book_number: i32,
    pub book_name: String,
    pub chapter: i32,
    pub verse_start: i32,
    pub verse_end: Option<i32>,
}

/// Indicates how a detection was made.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DetectionSource {
    DirectReference,
    Semantic { similarity: f64 },
}

/// A single detected Bible reference in transcript text.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Detection {
    pub verse_ref: VerseRef,
    /// Database primary key from semantic search (verses.id).
    /// Only set for semantic detections; direct detections use `verse_ref` fields instead.
    pub verse_id: Option<i64>,
    pub confidence: f64,
    pub source: DetectionSource,
    pub transcript_snippet: String,
    pub detected_at: u64,
    /// True when the detection was emitted from a chapter-only reference (no verse spoken yet).
    /// The verse defaults to 1 and may be refined later when the speaker says the verse number.
    #[serde(default)]
    pub is_chapter_only: bool,
    /// True when the book name was recovered by fuzzy edit-distance, not a
    /// canonical or alias automaton hit. Never action-authorizing.
    #[serde(default)]
    pub is_fuzzy_book: bool,
    /// True when a distinctive contiguous quote span was verified against verse text.
    #[serde(default)]
    pub has_lexical_quote: bool,
    /// Fraction of the spoken fragment covered by the verified quote span, if any.
    #[serde(default)]
    pub quote_coverage: f64,
    /// Score gap above the runner-up candidate in the same batch.
    #[serde(default)]
    pub candidate_margin: f64,
    /// STT utterance sequence ID for deduplication and independent final tracking.
    #[serde(default)]
    pub utterance_id: Option<u64>,
    /// True when emitted from a final transcript rather than a partial.
    #[serde(default)]
    pub is_final_utterance: bool,
}

impl Detection {
    /// Internal ordering score. Semantic similarity stores the ensemble rank,
    /// while confidence remains the operator-facing match strength.
    pub fn rank_score(&self) -> f64 {
        match self.source {
            DetectionSource::DirectReference => self.confidence,
            DetectionSource::Semantic { similarity } => similarity,
        }
    }

    pub fn is_complete_citation(&self) -> bool {
        !self.is_chapter_only
            && !self.is_fuzzy_book
            && self.verse_ref.book_number > 0
            && self.verse_ref.chapter > 0
            && self.verse_ref.verse_start > 0
            && matches!(self.source, DetectionSource::DirectReference)
    }
}

impl Default for Detection {
    fn default() -> Self {
        Self {
            verse_ref: VerseRef {
                book_number: 0,
                book_name: String::new(),
                chapter: 0,
                verse_start: 0,
                verse_end: None,
            },
            verse_id: None,
            confidence: 0.0,
            source: DetectionSource::DirectReference,
            transcript_snippet: String::new(),
            detected_at: 0,
            is_chapter_only: false,
            is_fuzzy_book: false,
            has_lexical_quote: false,
            quote_coverage: 0.0,
            candidate_margin: 1.0,
            utterance_id: None,
            is_final_utterance: false,
        }
    }
}
