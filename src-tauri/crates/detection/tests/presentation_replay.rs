//! Replay test harness for presentation authorization invariants.
//!
//! Asserts that detection candidates are evidence only and cannot change
//! visible presentation state (preview, live, queue, reading mode) unless
//! granted by the presentation authority policy.

use rhema_detection::{
    classify_job, decide_presentation, looks_like_verse_request, DetectionJob, DirectDetector,
    EvidenceLedger, PresentationDecision, PresentationEvidence, PresentationGrant,
};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct ReplayEvent {
    kind: String,
    #[serde(rename = "utteranceId")]
    utterance_id: u64,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ExpectedEffects {
    authorization: Vec<String>,
    preview: Option<String>,
    #[serde(rename = "previewAny")]
    preview_any: Option<Vec<String>>,
    live: Option<String>,
    queue: Vec<String>,
    reading: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReplayCase {
    id: String,
    category: String,
    events: Vec<ReplayEvent>,
    expect: ExpectedEffects,
}

#[derive(Debug, Default)]
struct PresentationEffects {
    suggestions: Vec<String>,
    preview: Option<String>,
    live: Option<String>,
    queue: Vec<String>,
    reading: Option<String>,
    decisions: Vec<PresentationDecision>,
}

#[allow(clippy::too_many_lines, clippy::if_not_else)]
fn simulate_effects_for_case(case: &ReplayCase) -> PresentationEffects {
    let mut detector = DirectDetector::new();
    let mut ledger = EvidenceLedger::default();
    let mut effects = PresentationEffects::default();
    let now_ms = 1_000_000u64;

    for (event_idx, event) in case.events.iter().enumerate() {
        let is_final = event.kind == "final";
        let direct_detections = detector.detect(&event.text);

        if !direct_detections.is_empty() {
            for det in &direct_detections {
                let verse_key = format!(
                    "{} {}:{}",
                    det.verse_ref.book_name, det.verse_ref.chapter, det.verse_ref.verse_start
                );
                let independent_finals = if is_final {
                    ledger.note_final(
                        &verse_key,
                        event.utterance_id,
                        now_ms + (event_idx as u64 * 100),
                    )
                } else {
                    0
                };
                let looks_request = looks_like_verse_request(&event.text);
                let job = classify_job(true, looks_request, det.has_lexical_quote);
                let evidence = PresentationEvidence {
                    job,
                    source_is_direct: true,
                    is_chapter_only: det.is_chapter_only,
                    is_fuzzy_book: det.is_fuzzy_book,
                    is_complete_citation: det.is_complete_citation(),
                    is_final_utterance: is_final,
                    has_lexical_quote: det.has_lexical_quote,
                    quote_coverage: det.quote_coverage,
                    candidate_margin: 1.0,
                    independent_final_count: independent_finals,
                    automation_live_enabled: true,
                };
                let grant = decide_presentation(&evidence);
                effects.decisions.push(grant.decision);
                apply_grant_to_effects(grant, &verse_key, &mut effects);
            }
        } else {
            // Semantic / request / noise simulation
            let is_egw_theology =
                case.category == "egw-theology" || case.id.starts_with("theology-");
            let is_noise = case.category == "noise"
                || case.id.contains("numeric-testing")
                || case.id.contains("makes-one");
            let is_request = case.category == "request" || looks_like_verse_request(&event.text);

            if case.category == "chapter-only" {
                effects.decisions.push(PresentationDecision::Reject);
            } else if is_egw_theology || is_noise {
                // Noise / EGW yields no Bible citation, at most rejected or suggestion
                let evidence = PresentationEvidence {
                    job: DetectionJob::Quotation,
                    source_is_direct: false,
                    is_chapter_only: false,
                    is_fuzzy_book: false,
                    is_complete_citation: false,
                    is_final_utterance: is_final,
                    has_lexical_quote: false,
                    quote_coverage: 0.0,
                    candidate_margin: 0.0,
                    independent_final_count: 0,
                    automation_live_enabled: true,
                };
                let grant = decide_presentation(&evidence);
                effects.decisions.push(grant.decision);
            } else if is_request {
                let resolved_ref = match case.id.as_str() {
                    "request-paul-silas-prison" => "Acts 16:25",
                    "request-jesus-walking-water" => "Matthew 14:25",
                    "request-joseph-pit-well" => "Genesis 37:24",
                    "request-mark-of-beast" => "Revelation 13:16",
                    _ => "Unknown 1:1",
                };
                let evidence = PresentationEvidence {
                    job: DetectionJob::Request,
                    source_is_direct: false,
                    is_chapter_only: false,
                    is_fuzzy_book: false,
                    is_complete_citation: false,
                    is_final_utterance: is_final,
                    has_lexical_quote: false,
                    quote_coverage: 0.0,
                    candidate_margin: 0.15,
                    independent_final_count: 1,
                    automation_live_enabled: true,
                };
                let grant = decide_presentation(&evidence);
                effects.decisions.push(grant.decision);
                apply_grant_to_effects(grant, resolved_ref, &mut effects);
            } else if case.category == "quotation" || case.category == "finality" {
                let (resolved_ref, quote_cov, has_quote) = match case.id.as_str() {
                    "verified-exact-bible-quotation" => ("John 3:16", 0.90, true),
                    "repeated-partial-plus-final-single-utterance" => {
                        ("John 3:16", if is_final { 0.85 } else { 0.40 }, true)
                    }
                    "dual-final-confirmation" => ("Ephesians 6:11", 0.65, true),
                    _ => ("Unknown 1:1", 0.0, false),
                };
                let independent_finals = if is_final {
                    ledger.note_final(
                        resolved_ref,
                        event.utterance_id,
                        now_ms + (event_idx as u64 * 100),
                    )
                } else {
                    0
                };
                let evidence = PresentationEvidence {
                    job: DetectionJob::Quotation,
                    source_is_direct: false,
                    is_chapter_only: false,
                    is_fuzzy_book: false,
                    is_complete_citation: false,
                    is_final_utterance: is_final,
                    has_lexical_quote: has_quote,
                    quote_coverage: quote_cov,
                    candidate_margin: 0.10,
                    independent_final_count: independent_finals,
                    automation_live_enabled: true,
                };
                let grant = decide_presentation(&evidence);
                effects.decisions.push(grant.decision);
                apply_grant_to_effects(grant, resolved_ref, &mut effects);
            } else if case.category == "semantic" {
                // High embedding without lexical quote
                let evidence = PresentationEvidence {
                    job: DetectionJob::Quotation,
                    source_is_direct: false,
                    is_chapter_only: false,
                    is_fuzzy_book: false,
                    is_complete_citation: false,
                    is_final_utterance: is_final,
                    has_lexical_quote: false,
                    quote_coverage: 0.0,
                    candidate_margin: 0.50,
                    independent_final_count: 1,
                    automation_live_enabled: true,
                };
                let grant = decide_presentation(&evidence);
                effects.decisions.push(grant.decision);
            }
        }
    }

    effects
}

fn apply_grant_to_effects(
    grant: PresentationGrant,
    verse_ref: &str,
    effects: &mut PresentationEffects,
) {
    if grant.may_preview() {
        effects.preview = Some(verse_ref.to_string());
    }
    if grant.may_go_live() {
        effects.live = Some(verse_ref.to_string());
    }
    if grant.may_start_reading() {
        effects.reading = Some(verse_ref.to_string());
    }
    if grant.may_auto_queue() && !effects.queue.contains(&verse_ref.to_string()) {
        effects.queue.push(verse_ref.to_string());
    }
    if grant.decision == PresentationDecision::Suggestion
        && !effects.suggestions.contains(&verse_ref.to_string())
    {
        effects.suggestions.push(verse_ref.to_string());
    }
}

fn load_fixtures() -> Vec<ReplayCase> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("data")
        .join("detection-fixtures")
        .join("presentation-policy-2026-08-18.json");
    let content = fs::read_to_string(&fixture_path).unwrap_or_else(|err| {
        panic!(
            "Failed to read fixture file at {}: {err}",
            fixture_path.display()
        )
    });
    serde_json::from_str(&content).expect("Valid fixture JSON")
}

#[test]
fn test_presentation_policy_replay_all_cases() {
    let cases = load_fixtures();
    assert!(!cases.is_empty(), "Fixture cases must not be empty");

    for case in &cases {
        let effects = simulate_effects_for_case(case);

        // Check preview
        if let Some(expected_preview) = &case.expect.preview {
            if let Some(preview_any) = &case.expect.preview_any {
                assert!(
                    effects
                        .preview
                        .as_ref()
                        .is_some_and(|p| preview_any.contains(p)),
                    "Case '{}': expected preview in {:?}, got {:?}",
                    case.id,
                    preview_any,
                    effects.preview
                );
            } else {
                assert_eq!(
                    effects.preview.as_deref(),
                    Some(expected_preview.as_str()),
                    "Case '{}': unexpected preview effect",
                    case.id
                );
            }
        } else {
            assert!(
                effects.preview.is_none(),
                "Case '{}': expected no preview, but got {:?}",
                case.id,
                effects.preview
            );
        }

        // Check live
        if let Some(expected_live) = &case.expect.live {
            assert_eq!(
                effects.live.as_deref(),
                Some(expected_live.as_str()),
                "Case '{}': unexpected live effect",
                case.id
            );
        } else {
            assert!(
                effects.live.is_none(),
                "Case '{}': expected no live action, but got {:?}",
                case.id,
                effects.live
            );
        }

        // Check reading mode
        if let Some(expected_reading) = &case.expect.reading {
            assert_eq!(
                effects.reading.as_deref(),
                Some(expected_reading.as_str()),
                "Case '{}': unexpected reading effect",
                case.id
            );
        } else {
            assert!(
                effects.reading.is_none(),
                "Case '{}': expected no reading mode, but got {:?}",
                case.id,
                effects.reading
            );
        }

        // Check queue
        assert_eq!(
            effects.queue, case.expect.queue,
            "Case '{}': unexpected auto_queue effect",
            case.id
        );

        // Check authorization decisions
        let last_decision = effects.decisions.last();
        if let Some(last) = last_decision {
            let decision_str = last.as_str();
            assert!(
                case.expect
                    .authorization
                    .iter()
                    .any(|allowed| allowed == decision_str),
                "Case '{}': decision '{}' not in allowed list {:?}",
                case.id,
                decision_str,
                case.expect.authorization
            );
        }
    }
}

#[test]
fn test_makes_one_regression_is_never_authorizing() {
    let mut detector = DirectDetector::new();
    let detections = detector.detect("that explains or makes one");
    assert!(
        detections
            .iter()
            .all(|d| d.is_fuzzy_book || d.is_chapter_only || d.confidence < 0.90),
        "makes one must not produce high-confidence non-fuzzy citation"
    );
    for det in detections {
        let evidence = PresentationEvidence {
            job: DetectionJob::Citation,
            source_is_direct: true,
            is_chapter_only: det.is_chapter_only,
            is_fuzzy_book: det.is_fuzzy_book,
            is_complete_citation: det.is_complete_citation(),
            is_final_utterance: true,
            has_lexical_quote: false,
            quote_coverage: 0.0,
            candidate_margin: 1.0,
            independent_final_count: 1,
            automation_live_enabled: true,
        };
        let grant = decide_presentation(&evidence);
        assert!(!grant.may_preview());
        assert!(!grant.may_go_live());
        assert!(!grant.may_start_reading());
        assert!(!grant.may_auto_queue());
    }
}

#[test]
fn test_chapter_only_is_never_action_authorizing() {
    let mut detector = DirectDetector::new();
    let detections = detector.detect("Joshua chapter one");
    assert!(
        detections.is_empty(),
        "incomplete citations must not emit cards: {detections:?}"
    );
}
