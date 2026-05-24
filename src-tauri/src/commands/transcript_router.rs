use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEventKind {
    Partial,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRoute {
    pub emit_transcript: bool,
    pub preview_candidate: Option<String>,
    pub authoritative_detection: Option<String>,
    pub suppress_reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct TranscriptRouteInput<'a> {
    pub provider: &'a str,
    pub kind: TranscriptEventKind,
    pub transcript: &'a str,
    pub confidence: Option<f64>,
}

#[derive(Debug, Default)]
pub struct TranscriptRouter {
    last_partial: Option<String>,
    recent_finals: VecDeque<String>,
}

impl TranscriptRouter {
    pub fn route(&mut self, input: TranscriptRouteInput<'_>) -> TranscriptRoute {
        let cleaned = input.transcript.trim();
        let normalized = normalize_transcript(cleaned);

        if normalized.is_empty() {
            return suppressed("empty");
        }

        if is_noise_label(cleaned) {
            return suppressed("noise_label");
        }

        if looks_like_non_speech(&normalized) {
            return suppressed("non_speech");
        }

        let min_final_confidence = match input.provider {
            "deepgram" => 0.35,
            _ => 0.20,
        };
        if matches!(input.kind, TranscriptEventKind::Final)
            && input
                .confidence
                .is_some_and(|c| c > 0.0 && c < min_final_confidence)
        {
            return suppressed("low_confidence");
        }

        if matches!(input.kind, TranscriptEventKind::Partial) {
            if self.last_partial.as_deref() == Some(normalized.as_str()) {
                return TranscriptRoute {
                    emit_transcript: false,
                    preview_candidate: None,
                    authoritative_detection: None,
                    suppress_reason: Some("duplicate_partial".to_string()),
                };
            }
            self.last_partial = Some(normalized);
            return TranscriptRoute {
                emit_transcript: true,
                preview_candidate: looks_like_complete_reference(cleaned)
                    .then(|| cleaned.to_string()),
                authoritative_detection: None,
                suppress_reason: None,
            };
        }

        self.last_partial = None;
        if self.recent_finals.iter().any(|prev| prev == &normalized) {
            return suppressed("duplicate_final");
        }
        self.recent_finals.push_back(normalized);
        while self.recent_finals.len() > 12 {
            self.recent_finals.pop_front();
        }

        let authoritative_detection = Some(cleaned.to_string());
        TranscriptRoute {
            emit_transcript: true,
            preview_candidate: looks_like_complete_reference(cleaned).then(|| cleaned.to_string()),
            authoritative_detection,
            suppress_reason: None,
        }
    }
}

fn suppressed(reason: &str) -> TranscriptRoute {
    TranscriptRoute {
        emit_transcript: false,
        preview_candidate: None,
        authoritative_detection: None,
        suppress_reason: Some(reason.to_string()),
    }
}

fn normalize_transcript(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

fn is_noise_label(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < 3 {
        return false;
    }

    let bracketed = (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('(') && trimmed.ends_with(')'));
    if !bracketed {
        return false;
    }

    let inner = trimmed
        .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')'))
        .to_lowercase();
    matches!(
        inner.as_str(),
        "music"
            | "applause"
            | "laughter"
            | "noise"
            | "background noise"
            | "silence"
            | "inaudible"
            | "crosstalk"
    )
}

fn looks_like_non_speech(normalized: &str) -> bool {
    if !normalized.chars().any(char::is_alphanumeric) {
        return true;
    }

    let words = normalized.split_whitespace().collect::<Vec<_>>();
    if words.len() >= 4 && words.windows(2).all(|w| w[0] == w[1]) {
        return true;
    }

    false
}

fn looks_like_complete_reference(text: &str) -> bool {
    let lower = text.to_lowercase();
    if !contains_book_hint(&lower) {
        return false;
    }

    lower.contains(':')
        || lower.contains(" verse ")
        || lower.contains(" verses ")
        || has_two_numbers(&lower)
        || (lower.contains(" chapter ") && has_numberish(&lower))
}

fn contains_book_hint(lower: &str) -> bool {
    const BOOK_HINTS: &[&str] = &[
        "genesis",
        "exodus",
        "leviticus",
        "numbers",
        "deuteronomy",
        "joshua",
        "judges",
        "ruth",
        "samuel",
        "kings",
        "chronicles",
        "ezra",
        "nehemiah",
        "esther",
        "job",
        "psalm",
        "proverb",
        "ecclesiastes",
        "isaiah",
        "jeremiah",
        "lamentations",
        "ezekiel",
        "daniel",
        "hosea",
        "joel",
        "amos",
        "obadiah",
        "jonah",
        "micah",
        "nahum",
        "habakkuk",
        "zephaniah",
        "haggai",
        "zechariah",
        "malachi",
        "matthew",
        "mark",
        "luke",
        "john",
        "acts",
        "romans",
        "corinthians",
        "galatians",
        "ephesians",
        "philippians",
        "colossians",
        "thessalonians",
        "timothy",
        "titus",
        "philemon",
        "hebrews",
        "james",
        "peter",
        "jude",
        "revelation",
    ];

    BOOK_HINTS.iter().any(|book| lower.contains(book))
}

fn has_numberish(lower: &str) -> bool {
    lower.chars().any(|c| c.is_ascii_digit())
        || lower.split_whitespace().any(|word| {
            matches!(
                word.trim_matches(|c: char| !c.is_alphanumeric()),
                "one"
                    | "two"
                    | "three"
                    | "four"
                    | "five"
                    | "six"
                    | "seven"
                    | "eight"
                    | "nine"
                    | "ten"
                    | "eleven"
                    | "twelve"
                    | "thirteen"
                    | "fourteen"
                    | "fifteen"
                    | "sixteen"
                    | "seventeen"
                    | "eighteen"
                    | "nineteen"
                    | "twenty"
                    | "thirty"
                    | "forty"
                    | "fifty"
                    | "sixty"
                    | "seventy"
                    | "eighty"
                    | "ninety"
            )
        })
}

fn has_two_numbers(lower: &str) -> bool {
    lower
        .split_whitespace()
        .filter(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .parse::<i32>()
                .is_ok()
        })
        .take(2)
        .count()
        >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: TranscriptEventKind, transcript: &str) -> TranscriptRouteInput<'_> {
        TranscriptRouteInput {
            provider: "test",
            kind,
            transcript,
            confidence: Some(0.9),
        }
    }

    #[test]
    fn final_speech_goes_to_transcript_and_authoritative_detection() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(
            TranscriptEventKind::Final,
            "Let us turn to Exodus 20 verse 4, keeping the sabbath holy",
        ));

        assert!(route.emit_transcript);
        assert_eq!(
            route.preview_candidate.as_deref(),
            Some("Let us turn to Exodus 20 verse 4, keeping the sabbath holy")
        );
        assert!(route.authoritative_detection.is_some());
    }

    #[test]
    fn ordinary_final_speech_is_transcribed_but_not_previewed() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(
            TranscriptEventKind::Final,
            "Today we are talking about obedience and grace",
        ));

        assert!(route.emit_transcript);
        assert!(route.preview_candidate.is_none());
        assert!(route.authoritative_detection.is_some());
    }

    #[test]
    fn partial_complete_reference_can_preview_only() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(TranscriptEventKind::Partial, "John 3 16"));

        assert!(route.emit_transcript);
        assert_eq!(route.preview_candidate.as_deref(), Some("John 3 16"));
        assert!(route.authoritative_detection.is_none());
    }

    #[test]
    fn partial_without_reference_does_not_preview_or_detect() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(TranscriptEventKind::Partial, "we are going to"));

        assert!(route.emit_transcript);
        assert!(route.preview_candidate.is_none());
        assert!(route.authoritative_detection.is_none());
    }

    #[test]
    fn suppresses_noise_labels_and_duplicate_finals() {
        let mut router = TranscriptRouter::default();
        let noise = router.route(input(TranscriptEventKind::Final, "[music]"));
        assert_eq!(noise.suppress_reason.as_deref(), Some("noise_label"));

        let first = router.route(input(TranscriptEventKind::Final, "John 3 16"));
        let duplicate = router.route(input(TranscriptEventKind::Final, "john 3 16"));

        assert!(first.emit_transcript);
        assert_eq!(
            duplicate.suppress_reason.as_deref(),
            Some("duplicate_final")
        );
    }

    #[test]
    fn suppresses_low_confidence_final_when_provider_supplies_confidence() {
        let mut router = TranscriptRouter::default();
        let route = router.route(TranscriptRouteInput {
            provider: "deepgram",
            kind: TranscriptEventKind::Final,
            transcript: "random low confidence words",
            confidence: Some(0.2),
        });

        assert_eq!(route.suppress_reason.as_deref(), Some("low_confidence"));
    }
}
