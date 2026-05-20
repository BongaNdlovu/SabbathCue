//! Small deterministic helpers for STT accuracy smoke tests.
//!
//! The live providers stay responsible for transcription. This module keeps
//! benchmark scoring cheap and repeatable so accuracy changes can be compared
//! without touching the hot path.

use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptFixture {
    pub name: String,
    pub expected: String,
    #[serde(default)]
    pub scripture_terms: Vec<String>,
    #[serde(default)]
    pub actual: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TranscriptBenchResult {
    pub name: String,
    pub similarity: f64,
    pub scripture_terms_found: usize,
    pub scripture_terms_total: usize,
    pub duration_ms: u128,
}

impl TranscriptBenchResult {
    #[expect(
        clippy::cast_precision_loss,
        reason = "benchmark term counts are tiny fixture values"
    )]
    pub fn scripture_term_accuracy(&self) -> f64 {
        if self.scripture_terms_total == 0 {
            return 1.0;
        }

        self.scripture_terms_found as f64 / self.scripture_terms_total as f64
    }
}

pub fn score_fixture(
    fixture: &TranscriptFixture,
    actual: &str,
    elapsed: Duration,
) -> TranscriptBenchResult {
    TranscriptBenchResult {
        name: fixture.name.clone(),
        similarity: token_similarity(&fixture.expected, actual),
        scripture_terms_found: count_scripture_terms(actual, &fixture.scripture_terms),
        scripture_terms_total: fixture.scripture_terms.len(),
        duration_ms: elapsed.as_millis(),
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "benchmark token counts are tiny fixture values"
)]
pub fn token_similarity(expected: &str, actual: &str) -> f64 {
    let expected_tokens = normalized_tokens(expected);
    let actual_tokens = normalized_tokens(actual);

    if expected_tokens.is_empty() && actual_tokens.is_empty() {
        return 1.0;
    }
    if expected_tokens.is_empty() || actual_tokens.is_empty() {
        return 0.0;
    }

    let expected_set = expected_tokens.into_iter().collect::<HashSet<_>>();
    let actual_set = actual_tokens.into_iter().collect::<HashSet<_>>();
    let overlap = expected_set.intersection(&actual_set).count();
    let union = expected_set.union(&actual_set).count();

    overlap as f64 / union as f64
}

fn count_scripture_terms(actual: &str, terms: &[String]) -> usize {
    let normalized_actual = normalize(actual);
    terms
        .iter()
        .filter(|term| normalized_actual.contains(&normalize(term)))
        .count()
}

fn normalized_tokens(input: &str) -> Vec<String> {
    normalize(input)
        .split_whitespace()
        .map(ToString::to_string)
        .collect()
}

fn normalize(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_similarity_rewards_shared_transcript_terms() {
        let score = token_similarity(
            "Please open John chapter three verse sixteen",
            "open John 3 verse sixteen",
        );

        assert!(score > 0.40, "expected useful overlap, got {score}");
    }

    #[test]
    fn score_fixture_tracks_scripture_terms_and_duration() {
        let fixture = TranscriptFixture {
            name: "john".into(),
            expected: "John chapter three verse sixteen".into(),
            scripture_terms: vec!["John".into(), "verse sixteen".into()],
            actual: None,
        };

        let result = score_fixture(
            &fixture,
            "We are reading John chapter 3, verse sixteen.",
            Duration::from_millis(42),
        );

        assert_eq!(result.scripture_terms_found, 2);
        assert_eq!(result.scripture_terms_total, 2);
        assert_eq!(result.duration_ms, 42);
        assert!(result.similarity > 0.0);
    }
}
