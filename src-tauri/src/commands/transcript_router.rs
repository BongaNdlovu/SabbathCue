use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEventKind {
    Partial,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRoute {
    pub emit_transcript: bool,
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
    /// A verse-navigation command was already dispatched from a partial of the
    /// in-flight utterance; the matching final must not dispatch it again.
    command_dispatched_on_partial: bool,
    /// Whether any partial arrived since the last final — distinguishes a
    /// genuine repeated utterance from a provider re-sending the same final.
    saw_partial_since_final: bool,
    /// Normalized text of the immediately preceding final (any kind).
    last_final: Option<String>,
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
            self.saw_partial_since_final = true;
            if self.last_partial.as_deref() == Some(normalized.as_str()) {
                return TranscriptRoute {
                    emit_transcript: false,
                    authoritative_detection: None,
                    suppress_reason: Some("duplicate_partial".to_string()),
                };
            }
            self.last_partial = Some(normalized);
            // A partial that already reads as a complete reference detects
            // immediately, for any provider, so the verse surfaces before the
            // endpointing pause. (Was deepgram-only.)
            let complete_reference = looks_like_complete_reference(cleaned);
            // Same fast-path for verse-navigation commands ("next verse"),
            // dispatched at most once per utterance; the matching final is
            // consumed instead of advancing a second time.
            let complete_command = !complete_reference
                && rhema_detection::is_complete_verse_navigation_command(cleaned);
            let dispatch_command = complete_command && !self.command_dispatched_on_partial;
            self.command_dispatched_on_partial = complete_command;
            return TranscriptRoute {
                emit_transcript: true,
                authoritative_detection: (complete_reference || dispatch_command)
                    .then(|| cleaned.to_string()),
                suppress_reason: None,
            };
        }

        self.last_partial = None;
        let command_pending = std::mem::take(&mut self.command_dispatched_on_partial);
        let saw_partial = std::mem::take(&mut self.saw_partial_since_final);

        if rhema_detection::is_complete_verse_navigation_command(cleaned) {
            // Command finals bypass the 12-deep text dedupe: each spoken
            // repeat must advance again. A provider re-send (identical final,
            // no partials in between) is still dropped.
            if !command_pending && !saw_partial && self.last_final.as_deref() == Some(&normalized) {
                return suppressed("duplicate_final");
            }
            self.last_final = Some(normalized);
            return TranscriptRoute {
                emit_transcript: true,
                authoritative_detection: (!command_pending).then(|| cleaned.to_string()),
                suppress_reason: command_pending
                    .then(|| "command_dispatched_on_partial".to_string()),
            };
        }

        if self.recent_finals.iter().any(|prev| prev == &normalized) {
            return suppressed("duplicate_final");
        }
        self.last_final = Some(normalized.clone());
        self.recent_finals.push_back(normalized);
        while self.recent_finals.len() > 12 {
            self.recent_finals.pop_front();
        }

        let authoritative_detection = Some(cleaned.to_string());
        TranscriptRoute {
            emit_transcript: true,
            authoritative_detection,
            suppress_reason: None,
        }
    }
}

fn suppressed(reason: &str) -> TranscriptRoute {
    TranscriptRoute {
        emit_transcript: false,
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

pub(crate) fn looks_like_complete_reference(text: &str) -> bool {
    let lower = text.to_lowercase();
    if !contains_book_hint(&lower) {
        return false;
    }

    lower.contains(':')
        || lower.contains(" verse ")
        || lower.contains(" verses ")
        || lower.contains(" vers ")
        || lower.contains(" hoofstuk ")
        || has_two_numberish(&lower)
        || (lower.contains(" chapter ") && has_numberish(&lower))
        || (lower.contains(" hoofstuk ") && has_numberish(&lower))
}

#[expect(clippy::too_many_lines, reason = "book hint list is data, not logic")]
fn contains_book_hint(lower: &str) -> bool {
    const BOOK_HINTS: &[&str] = &[
        "genesis",
        "exodus",
        "eksodus",
        "leviticus",
        "levitikus",
        "numbers",
        "numeri",
        "deuteronomy",
        "deuteronomium",
        "joshua",
        "josua",
        "judges",
        "rigters",
        "ruth",
        "rut",
        "samuel",
        "kings",
        "konings",
        "chronicles",
        "kronieke",
        "ezra",
        "esra",
        "nehemiah",
        "nehemia",
        "esther",
        "ester",
        "job",
        "psalm",
        "proverb",
        "spreuke",
        "ecclesiastes",
        "prediker",
        "isaiah",
        "jesaja",
        "jeremiah",
        "jeremia",
        "lamentations",
        "klaagliedere",
        "ezekiel",
        "esegiel",
        "daniel",
        "hosea",
        "joel",
        "amos",
        "obadiah",
        "obadja",
        "jonah",
        "jona",
        "micah",
        "miga",
        "nahum",
        "habakkuk",
        "zephaniah",
        "sefanja",
        "haggai",
        "zechariah",
        "sagaria",
        "malachi",
        "maleagi",
        "matthew",
        "matteus",
        "mark",
        "markus",
        "luke",
        "lukas",
        "john",
        "johannes",
        "acts",
        "handelinge",
        "romans",
        "romeine",
        "corinthians",
        "korintiers",
        "galatians",
        "galasiers",
        "ephesians",
        "effesiers",
        "philippians",
        "filippense",
        "colossians",
        "kolossense",
        "thessalonians",
        "tessalonisense",
        "timothy",
        "timoteus",
        "titus",
        "philemon",
        "filemon",
        "hebrews",
        "hebreers",
        "james",
        "jakobus",
        "peter",
        "petrus",
        "jude",
        "judas",
        "revelation",
        "openbaring",
    ];

    BOOK_HINTS.iter().any(|book| lower.contains(book))
}

fn has_numberish(lower: &str) -> bool {
    lower.chars().any(|c| c.is_ascii_digit()) || lower.split_whitespace().any(is_numberish_token)
}

fn has_two_numberish(lower: &str) -> bool {
    lower
        .split_whitespace()
        .filter(|word| is_numberish_token(word))
        .take(2)
        .count()
        >= 2
}

fn is_numberish_token(word: &str) -> bool {
    let token = word.trim_matches(|c: char| !c.is_alphanumeric());
    token.parse::<i32>().is_ok()
        || matches!(
            token,
            "one"
                | "een"
                | "two"
                | "twee"
                | "three"
                | "drie"
                | "four"
                | "vier"
                | "five"
                | "vyf"
                | "six"
                | "ses"
                | "seven"
                | "sewe"
                | "eight"
                | "agt"
                | "nine"
                | "nege"
                | "ten"
                | "tien"
                | "eleven"
                | "elf"
                | "twelve"
                | "twaalf"
                | "thirteen"
                | "dertien"
                | "fourteen"
                | "veertien"
                | "fifteen"
                | "vyftien"
                | "sixteen"
                | "sestien"
                | "seventeen"
                | "sewentien"
                | "eighteen"
                | "agtien"
                | "nineteen"
                | "negentien"
                | "twenty"
                | "twintig"
                | "thirty"
                | "dertig"
                | "forty"
                | "veertig"
                | "fifty"
                | "vyftig"
                | "sixty"
                | "sestig"
                | "seventy"
                | "sewentig"
                | "eighty"
                | "tagtig"
                | "ninety"
                | "negentig"
        )
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
        assert!(route.authoritative_detection.is_some());
    }

    #[test]
    fn partial_complete_reference_is_authoritative_for_any_provider() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(TranscriptEventKind::Partial, "John 3 16"));

        assert!(route.emit_transcript);
        assert_eq!(route.authoritative_detection.as_deref(), Some("John 3 16"));
    }

    #[test]
    fn partial_spoken_reference_is_authoritative_for_any_provider() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(TranscriptEventKind::Partial, "John three sixteen"));

        assert!(route.emit_transcript);
        assert_eq!(
            route.authoritative_detection.as_deref(),
            Some("John three sixteen")
        );
    }

    #[test]
    fn partial_afrikaans_reference_is_authoritative() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(
            TranscriptEventKind::Partial,
            "Deuteronomium 16 vers 18",
        ));

        assert_eq!(
            route.authoritative_detection.as_deref(),
            Some("Deuteronomium 16 vers 18")
        );
    }

    #[test]
    fn partial_afrikaans_spoken_number_reference_is_authoritative() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(
            TranscriptEventKind::Partial,
            "Matteus twintig vers vyf en twintig",
        ));

        assert_eq!(
            route.authoritative_detection.as_deref(),
            Some("Matteus twintig vers vyf en twintig")
        );
    }

    #[test]
    fn active_provider_finals_route_hymn_commands_authoritatively() {
        for provider in ["vosk", "deepgram", "soniox"] {
            let mut router = TranscriptRouter::default();
            let route = router.route(TranscriptRouteInput {
                provider,
                kind: TranscriptEventKind::Final,
                transcript: "Seventh-day Adventist hymnal 100",
                confidence: Some(0.95),
            });

            assert!(route.emit_transcript, "{provider}");
            assert_eq!(
                route.authoritative_detection.as_deref(),
                Some("Seventh-day Adventist hymnal 100"),
                "{provider}"
            );
            assert!(route.suppress_reason.is_none(), "{provider}");
        }
    }

    #[test]
    fn deepgram_partial_complete_reference_can_detect_authoritatively() {
        let mut router = TranscriptRouter::default();
        let route = router.route(TranscriptRouteInput {
            provider: "deepgram",
            kind: TranscriptEventKind::Partial,
            transcript: "John 3 16",
            confidence: Some(0.9),
        });

        assert_eq!(route.authoritative_detection.as_deref(), Some("John 3 16"));
    }

    #[test]
    fn deepgram_partial_without_complete_reference_does_not_detect_authoritatively() {
        let mut router = TranscriptRouter::default();
        let route = router.route(TranscriptRouteInput {
            provider: "deepgram",
            kind: TranscriptEventKind::Partial,
            transcript: "we are turning to John",
            confidence: Some(0.9),
        });

        assert!(route.authoritative_detection.is_none());
    }

    #[test]
    fn partial_without_reference_does_not_preview_or_detect() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(TranscriptEventKind::Partial, "we are going to"));

        assert!(route.emit_transcript);
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
    fn partial_verse_navigation_command_dispatches_immediately() {
        let mut router = TranscriptRouter::default();
        let route = router.route(input(
            TranscriptEventKind::Partial,
            "Let's go to the next verse",
        ));

        assert!(route.emit_transcript);
        assert_eq!(
            route.authoritative_detection.as_deref(),
            Some("Let's go to the next verse")
        );
    }

    #[test]
    fn growing_partial_dispatches_navigation_command_once() {
        let mut router = TranscriptRouter::default();

        let building = router.route(input(TranscriptEventKind::Partial, "let's go to the"));
        assert!(building.authoritative_detection.is_none());

        let command = router.route(input(
            TranscriptEventKind::Partial,
            "let's go to the next verse",
        ));
        assert!(command.authoritative_detection.is_some());

        let grown = router.route(input(
            TranscriptEventKind::Partial,
            "let's go to the next verse please",
        ));
        assert!(
            grown.authoritative_detection.is_none(),
            "same utterance must not dispatch the command twice"
        );
    }

    #[test]
    fn final_matching_dispatched_partial_command_is_consumed() {
        let mut router = TranscriptRouter::default();
        router.route(input(
            TranscriptEventKind::Partial,
            "let's go to the next verse",
        ));

        let fin = router.route(input(
            TranscriptEventKind::Final,
            "Let's go to the next verse.",
        ));

        assert!(fin.emit_transcript, "final must still reach the transcript");
        assert!(
            fin.authoritative_detection.is_none(),
            "final must not re-dispatch a command already handled on the partial"
        );
    }

    #[test]
    fn repeated_navigation_command_utterances_each_dispatch() {
        let mut router = TranscriptRouter::default();

        for cycle in 0..3 {
            let partial = router.route(input(
                TranscriptEventKind::Partial,
                "Let's go to the next verse",
            ));
            assert_eq!(
                partial.authoritative_detection.as_deref(),
                Some("Let's go to the next verse"),
                "cycle {cycle}: repeat command must dispatch on its partial"
            );

            let fin = router.route(input(
                TranscriptEventKind::Final,
                "Let's go to the next verse.",
            ));
            assert!(fin.emit_transcript, "cycle {cycle}");
            assert!(fin.authoritative_detection.is_none(), "cycle {cycle}");
        }
    }

    #[test]
    fn final_only_navigation_command_repeats_are_not_text_deduped() {
        let mut router = TranscriptRouter::default();

        let first = router.route(input(TranscriptEventKind::Final, "Next verse."));
        assert!(first.authoritative_detection.is_some());

        // A partial from the next utterance separates the two finals.
        let _ = router.route(input(TranscriptEventKind::Partial, "next"));

        let second = router.route(input(TranscriptEventKind::Final, "Next verse."));
        assert!(
            second.authoritative_detection.is_some(),
            "a repeated spoken command is a new utterance, not a duplicate"
        );
    }

    #[test]
    fn resent_identical_command_final_without_partials_is_suppressed() {
        let mut router = TranscriptRouter::default();

        let first = router.route(input(TranscriptEventKind::Final, "Next verse."));
        assert!(first.authoritative_detection.is_some());

        // Same final again with no partial in between = provider re-send.
        let resent = router.route(input(TranscriptEventKind::Final, "Next verse."));
        assert_eq!(resent.suppress_reason.as_deref(), Some("duplicate_final"));
        assert!(resent.authoritative_detection.is_none());
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
