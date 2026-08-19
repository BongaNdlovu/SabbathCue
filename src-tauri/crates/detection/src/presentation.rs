//! Single owner of presentation authorization.
//!
//! A detection candidate is evidence, not permission. Every direct, semantic,
//! and request result must pass through [`decide_presentation`] before preview,
//! live output, queueing, or reading mode may change.

use serde::{Deserialize, Serialize};

/// What the church / operator UI is allowed to do with a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresentationDecision {
    Reject,
    Suggestion,
    PreviewAuthorized,
    ReadingAuthorized,
    LiveAuthorized,
}

impl PresentationDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Suggestion => "suggestion",
            Self::PreviewAuthorized => "preview-authorized",
            Self::ReadingAuthorized => "reading-authorized",
            Self::LiveAuthorized => "live-authorized",
        }
    }
}

/// Decision plus the job that earned it. Reading mode is citation-only even
/// when a quotation is live-authorized under automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationGrant {
    pub decision: PresentationDecision,
    pub job: DetectionJob,
}

impl PresentationGrant {
    pub fn may_preview(self) -> bool {
        matches!(
            self.decision,
            PresentationDecision::PreviewAuthorized
                | PresentationDecision::ReadingAuthorized
                | PresentationDecision::LiveAuthorized
        )
    }

    pub fn may_start_reading(self) -> bool {
        self.job == DetectionJob::Citation
            && matches!(
                self.decision,
                PresentationDecision::ReadingAuthorized | PresentationDecision::LiveAuthorized
            )
    }

    pub fn may_go_live(self) -> bool {
        self.decision == PresentationDecision::LiveAuthorized
    }

    pub fn may_auto_queue(self) -> bool {
        self.job == DetectionJob::Citation
            && matches!(
                self.decision,
                PresentationDecision::ReadingAuthorized | PresentationDecision::LiveAuthorized
            )
    }
}

/// The three detection jobs that must not share one auto-navigation path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DetectionJob {
    /// Spoken book + complete chapter:verse grammar.
    Citation,
    /// Distinctive contiguous quotation of verse text.
    Quotation,
    /// “Show me the verse about…” / people-event request.
    Request,
}

impl DetectionJob {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Citation => "citation",
            Self::Quotation => "quotation",
            Self::Request => "request",
        }
    }
}

/// Evidence the engine is allowed to consider. Parser confidence is *not*
/// authorization confidence and is not a field here.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct PresentationEvidence {
    pub job: DetectionJob,
    pub source_is_direct: bool,
    pub is_chapter_only: bool,
    pub is_fuzzy_book: bool,
    pub is_complete_citation: bool,
    pub is_final_utterance: bool,
    pub has_lexical_quote: bool,
    pub quote_coverage: f64,
    pub candidate_margin: f64,
    /// Distinct *final* utterances that independently named this verse.
    pub independent_final_count: u32,
    pub automation_live_enabled: bool,
}

/// User-configured automation policy synchronized from the frontend settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationPolicy {
    pub auto_mode: bool,
    pub semantic_enabled: bool,
    pub direct_threshold: f64,
    pub semantic_threshold: f64,
    pub live_output_enabled: bool,
}

impl Default for PresentationPolicy {
    fn default() -> Self {
        Self {
            auto_mode: true,
            semantic_enabled: true,
            direct_threshold: 0.90,
            semantic_threshold: 0.70,
            live_output_enabled: false,
        }
    }
}

/// Distinctive contiguous quote span + coverage required before a quotation
/// may leave the suggestion band.
pub const LEXICAL_QUOTE_MIN_COVERAGE: f64 = 0.56;
pub const QUOTATION_MIN_MARGIN: f64 = 0.02;

pub fn classify_job(
    source_is_direct: bool,
    looks_like_request: bool,
    has_lexical_quote: bool,
) -> DetectionJob {
    if source_is_direct && !looks_like_request {
        return DetectionJob::Citation;
    }
    if looks_like_request && !has_lexical_quote {
        return DetectionJob::Request;
    }
    DetectionJob::Quotation
}

pub fn looks_like_verse_request(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let asks = lower.contains("show")
        || lower.contains("talks about")
        || lower.contains("talk about")
        || lower.contains("verse about")
        || lower.contains("passage about")
        || lower.contains("passage where")
        || lower.contains("verse where")
        || lower.contains("go to the verse")
        || lower.contains("go back to the verse");
    asks && (lower.contains("verse") || lower.contains("passage"))
}

/// Authorize one candidate. This is the only function that may grant
/// preview, reading-mode, live, or auto-queue permission.
pub fn decide_presentation(evidence: &PresentationEvidence) -> PresentationGrant {
    let decision = if evidence.is_chapter_only || evidence.is_fuzzy_book {
        if evidence.source_is_direct {
            PresentationDecision::Suggestion
        } else {
            PresentationDecision::Reject
        }
    } else {
        match evidence.job {
            DetectionJob::Citation => decide_citation(evidence),
            DetectionJob::Quotation => decide_quotation(evidence),
            DetectionJob::Request => decide_request(evidence),
        }
    };
    PresentationGrant {
        decision,
        job: evidence.job,
    }
}

fn decide_citation(evidence: &PresentationEvidence) -> PresentationDecision {
    if !evidence.source_is_direct || !evidence.is_complete_citation {
        return PresentationDecision::Suggestion;
    }
    if !evidence.is_final_utterance {
        return PresentationDecision::Suggestion;
    }
    if evidence.automation_live_enabled {
        PresentationDecision::LiveAuthorized
    } else {
        PresentationDecision::ReadingAuthorized
    }
}

fn decide_quotation(evidence: &PresentationEvidence) -> PresentationDecision {
    let lexical_ok = evidence.has_lexical_quote
        && evidence.quote_coverage + f64::EPSILON >= LEXICAL_QUOTE_MIN_COVERAGE
        && evidence.candidate_margin + f64::EPSILON >= QUOTATION_MIN_MARGIN;
    let confirmed = lexical_ok
        && (evidence.is_final_utterance || evidence.independent_final_count >= 2);

    if !confirmed {
        return PresentationDecision::Suggestion;
    }
    if evidence.automation_live_enabled {
        PresentationDecision::LiveAuthorized
    } else {
        PresentationDecision::PreviewAuthorized
    }
}

fn decide_request(evidence: &PresentationEvidence) -> PresentationDecision {
    if evidence.is_final_utterance && evidence.candidate_margin + f64::EPSILON >= QUOTATION_MIN_MARGIN
    {
        return PresentationDecision::PreviewAuthorized;
    }
    PresentationDecision::Suggestion
}

/// Verse-key + time ledger that counts *distinct final utterances*, not
/// overlapping partial/final echoes of the same speech.
#[derive(Debug, Default)]
pub struct EvidenceLedger {
    seen: std::collections::HashMap<String, Vec<(u64, u64)>>,
}

const EVIDENCE_WINDOW_MS: u64 = 8_000;

impl EvidenceLedger {
    pub fn note_final(&mut self, verse_key: &str, utterance_id: u64, now_ms: u64) -> u32 {
        self.seen.retain(|_, entries| {
            entries.retain(|(_, seen_at)| now_ms.saturating_sub(*seen_at) <= EVIDENCE_WINDOW_MS);
            !entries.is_empty()
        });
        let entries = self.seen.entry(verse_key.to_string()).or_default();
        if !entries
            .iter()
            .any(|(existing, _)| *existing == utterance_id)
        {
            entries.push((utterance_id, now_ms));
        }
        u32::try_from(entries.len()).unwrap_or(u32::MAX)
    }

    pub fn reset(&mut self) {
        self.seen.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn citation_final() -> PresentationEvidence {
        PresentationEvidence {
            job: DetectionJob::Citation,
            source_is_direct: true,
            is_chapter_only: false,
            is_fuzzy_book: false,
            is_complete_citation: true,
            is_final_utterance: true,
            has_lexical_quote: false,
            quote_coverage: 0.0,
            candidate_margin: 1.0,
            independent_final_count: 1,
            automation_live_enabled: true,
        }
    }

    #[test]
    fn chapter_only_is_never_action_authorizing() {
        let mut evidence = citation_final();
        evidence.is_chapter_only = true;
        evidence.is_complete_citation = false;
        let grant = decide_presentation(&evidence);
        assert_eq!(grant.decision, PresentationDecision::Suggestion);
        assert!(!grant.may_start_reading());
        assert!(!grant.may_preview());
        assert!(!grant.may_go_live());
        assert!(!grant.may_auto_queue());
    }

    #[test]
    fn fuzzy_book_is_never_action_authorizing() {
        let mut evidence = citation_final();
        evidence.is_fuzzy_book = true;
        let grant = decide_presentation(&evidence);
        assert_eq!(grant.decision, PresentationDecision::Suggestion);
        assert!(!grant.may_start_reading());
    }

    #[test]
    fn complete_final_citation_may_go_live_and_start_reading() {
        let grant = decide_presentation(&citation_final());
        assert_eq!(grant.decision, PresentationDecision::LiveAuthorized);
        assert!(grant.may_start_reading());
        assert!(grant.may_go_live());
    }

    #[test]
    fn partial_citation_is_suggestion_only() {
        let mut evidence = citation_final();
        evidence.is_final_utterance = false;
        assert_eq!(
            decide_presentation(&evidence).decision,
            PresentationDecision::Suggestion
        );
    }

    #[test]
    fn high_embedding_without_lexical_quote_is_suggestion() {
        let evidence = PresentationEvidence {
            job: DetectionJob::Quotation,
            source_is_direct: false,
            is_chapter_only: false,
            is_fuzzy_book: false,
            is_complete_citation: false,
            is_final_utterance: true,
            has_lexical_quote: false,
            quote_coverage: 0.98,
            candidate_margin: 0.5,
            independent_final_count: 1,
            automation_live_enabled: true,
        };
        assert_eq!(
            decide_presentation(&evidence).decision,
            PresentationDecision::Suggestion
        );
    }

    #[test]
    fn verified_quotation_may_go_live_but_never_start_reading() {
        let evidence = PresentationEvidence {
            job: DetectionJob::Quotation,
            source_is_direct: false,
            is_chapter_only: false,
            is_fuzzy_book: false,
            is_complete_citation: false,
            is_final_utterance: true,
            has_lexical_quote: true,
            quote_coverage: 0.70,
            candidate_margin: 0.05,
            independent_final_count: 1,
            automation_live_enabled: true,
        };
        let grant = decide_presentation(&evidence);
        assert_eq!(grant.decision, PresentationDecision::LiveAuthorized);
        assert!(grant.may_preview());
        assert!(grant.may_go_live());
        assert!(!grant.may_start_reading());
        assert!(!grant.may_auto_queue());
    }

    #[test]
    fn request_never_starts_reading_mode() {
        let evidence = PresentationEvidence {
            job: DetectionJob::Request,
            source_is_direct: false,
            is_chapter_only: false,
            is_fuzzy_book: false,
            is_complete_citation: false,
            is_final_utterance: true,
            has_lexical_quote: false,
            quote_coverage: 0.0,
            candidate_margin: 0.20,
            independent_final_count: 1,
            automation_live_enabled: true,
        };
        let grant = decide_presentation(&evidence);
        assert_eq!(grant.decision, PresentationDecision::PreviewAuthorized);
        assert!(grant.may_preview());
        assert!(!grant.may_start_reading());
        assert!(!grant.may_go_live());
    }

    #[test]
    fn overlapping_same_utterance_does_not_count_as_two_finals() {
        let mut ledger = EvidenceLedger::default();
        assert_eq!(ledger.note_final("43:3:16", 7, 1_000), 1);
        assert_eq!(ledger.note_final("43:3:16", 7, 1_200), 1);
        assert_eq!(ledger.note_final("43:3:16", 8, 1_400), 2);
    }

    #[test]
    fn classify_request_vs_quotation() {
        assert_eq!(
            classify_job(false, true, false),
            DetectionJob::Request
        );
        assert_eq!(
            classify_job(false, false, true),
            DetectionJob::Quotation
        );
        assert_eq!(
            classify_job(true, false, false),
            DetectionJob::Citation
        );
        assert!(looks_like_verse_request(
            "show me the verse about Paul and Silas singing in prison"
        ));
        assert!(!looks_like_verse_request(
            "the Lord is my shepherd I shall not want"
        ));
    }
}
