//! EGW quote-run matching shared by the live detection path and the
//! `egw_accuracy` harness. Consecutive content-word runs with optional
//! bounded gaps so close paraphrases can score without BM25 flooding.

use crate::pipeline::{content_words, content_words_indexed};

/// Shared-run length at which a paragraph is treated as spoken aloud.
pub const EGW_QUOTE_RUN_FIRE: usize = 6;
/// Shared-run length that, with attribution, is strong enough to auto-queue.
pub const EGW_QUOTE_RUN_AUTO_QUEUE: usize = 8;
/// Shared-run length that becomes an operator hint once attribution is heard.
pub const EGW_QUOTE_RUN_CUED_HINT: usize = 4;
/// Max non-matching words (spoken or candidate) allowed between matches
/// inside a counted run. Sized for close paraphrase substitutions
/// ("word"/"truths", "minds"/"mind") without bridging unrelated sentences.
pub const EGW_RUN_MAX_GAP: usize = 3;
pub const EGW_QUOTE_MAX_CONFIDENCE: f64 = 0.92;

/// Longest run of shared content words, and where that run starts in the
/// paragraph as a UTF-8 **byte** offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedRun {
    pub len: usize,
    pub paragraph_byte_start: usize,
}

fn is_negation(word: &str) -> bool {
    matches!(word, "no" | "not" | "nor" | "never" | "neither" | "without")
}

fn quote_polarity_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| word.len() >= 4 || is_negation(word))
        .collect()
}

fn negation_context_conflicts(source: &[String], other: &[String]) -> bool {
    for (index, token) in source.iter().enumerate() {
        if !is_negation(token) {
            continue;
        }
        let anchor: Vec<&str> = source
            .iter()
            .skip(index + 1)
            .take(3)
            .map(String::as_str)
            .collect();
        if anchor.is_empty() {
            continue;
        }
        for (o_index, o_token) in other.iter().enumerate() {
            if o_token != anchor[0] {
                continue;
            }
            let other_anchor: Vec<&str> = other
                .iter()
                .skip(o_index)
                .take(anchor.len())
                .map(String::as_str)
                .collect();
            if other_anchor != anchor {
                continue;
            }
            let other_negated = o_index
                .checked_sub(1)
                .and_then(|i| other.get(i))
                .is_some_and(|w| is_negation(w));
            if !other_negated {
                return true;
            }
        }
    }
    false
}

/// True when one side asserts a negation the other side drops around the same
/// content anchors — opposite polarity must not count as a quote.
pub fn quote_has_negation_conflict(window: &str, paragraph: &str) -> bool {
    let spoken = quote_polarity_tokens(window);
    let candidate = quote_polarity_tokens(paragraph);
    negation_context_conflicts(&spoken, &candidate)
        || negation_context_conflicts(&candidate, &spoken)
}

/// Spoken tokens that mark a mid-quote navigation digression ("…fold verse 12
/// waiting…"). Gap tolerance must not skip these and splice two quote fragments
/// into one long run — that was the failure mode this guard preserves.
fn is_quote_scaffolding_token(word: &str) -> bool {
    matches!(
        word,
        "verse"
            | "verses"
            | "chapter"
            | "chapters"
            | "page"
            | "pages"
            | "paragraph"
            | "paragraphs"
            | "reference"
            | "text"
    )
}

/// Longest shared content-word run, allowing up to `EGW_RUN_MAX_GAP` unmatched
/// words on **either** side inside the span. `len` counts **matched** words only.
/// `paragraph_byte_start` is the byte offset of the first matched candidate word.
pub fn longest_shared_content_run(window: &str, paragraph: &str) -> SharedRun {
    let none = SharedRun {
        len: 0,
        paragraph_byte_start: 0,
    };
    if quote_has_negation_conflict(window, paragraph) {
        return none;
    }
    let spoken: Vec<String> = content_words(window).collect();
    let candidate: Vec<(usize, String)> = content_words_indexed(paragraph).collect();
    if spoken.is_empty() || candidate.is_empty() {
        return none;
    }

    let mut best = 0usize;
    let mut best_start = 0usize;

    // For each spoken/candidate start, walk with a small gap budget on either side.
    for s0 in 0..spoken.len() {
        for c0 in 0..candidate.len() {
            let mut s = s0;
            let mut c = c0;
            let mut matched = 0usize;
            let mut gaps = 0usize;
            let mut first_match_start: Option<usize> = None;
            while s < spoken.len() && c < candidate.len() {
                if spoken[s] == candidate[c].1 {
                    if first_match_start.is_none() {
                        first_match_start = Some(candidate[c].0);
                    }
                    matched += 1;
                    s += 1;
                    c += 1;
                    gaps = 0;
                    continue;
                }
                // Hard stop on spoken scaffolding so "…fold verse 12 waiting…"
                // cannot be gap-spliced into one contiguous quote run.
                if matched > 0 && is_quote_scaffolding_token(&spoken[s]) {
                    break;
                }
                if gaps >= EGW_RUN_MAX_GAP {
                    break;
                }
                // Prefer the skip that re-aligns on the next token when possible.
                let skip_candidate = c + 1 < candidate.len() && spoken[s] == candidate[c + 1].1;
                let skip_spoken = s + 1 < spoken.len()
                    && spoken[s + 1] == candidate[c].1
                    && !is_quote_scaffolding_token(&spoken[s]);
                let skip_both = s + 1 < spoken.len()
                    && c + 1 < candidate.len()
                    && spoken[s + 1] == candidate[c + 1].1
                    && !is_quote_scaffolding_token(&spoken[s]);
                if skip_candidate {
                    c += 1;
                    gaps += 1;
                } else if skip_spoken {
                    s += 1;
                    gaps += 1;
                } else if skip_both {
                    // Substitution: "word"↔"truths", "minds"↔"mind".
                    s += 1;
                    c += 1;
                    gaps += 1;
                } else if c + 1 < candidate.len() {
                    // Candidate insertion / alternate wording ahead of a later match.
                    c += 1;
                    gaps += 1;
                } else if s + 1 < spoken.len() && !is_quote_scaffolding_token(&spoken[s]) {
                    s += 1;
                    gaps += 1;
                } else {
                    break;
                }
            }
            if matched > best {
                best = matched;
                best_start = first_match_start.unwrap_or(candidate[c0].0);
            }
        }
    }

    if best == 0 {
        return none;
    }
    SharedRun {
        len: best,
        paragraph_byte_start: best_start,
    }
}

/// Map a shared-run length to `(confidence, auto_queued)`, or `None` to drop.
#[expect(
    clippy::cast_precision_loss,
    reason = "run lengths are single-digit word counts"
)]
pub fn egw_quote_score(run: usize, cue_active: bool) -> Option<(f64, bool)> {
    if run >= EGW_QUOTE_RUN_AUTO_QUEUE && cue_active {
        return Some((EGW_QUOTE_MAX_CONFIDENCE, true));
    }
    if run >= EGW_QUOTE_RUN_FIRE {
        let over = (run - EGW_QUOTE_RUN_FIRE).min(4) as f64;
        return Some(((0.88 + 0.01 * over).min(EGW_QUOTE_MAX_CONFIDENCE), false));
    }
    if run >= EGW_QUOTE_RUN_CUED_HINT && cue_active {
        let over = (run - EGW_QUOTE_RUN_CUED_HINT) as f64;
        return Some((0.75 + 0.05 * over, false));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_run_tolerates_a_paraphrase_gap() {
        let paragraph = "None but those who have fortified the mind with the truths of the Bible \
                         will stand through the last great conflict.";
        let window =
            "only those whose minds are fortified with the word of God will stand in these last days";

        let run = longest_shared_content_run(window, paragraph);
        assert!(
            run.len >= EGW_QUOTE_RUN_CUED_HINT,
            "a close paraphrase must reach at least hint strength, got {}",
            run.len
        );
    }

    #[test]
    fn shared_run_reports_anchor_on_quoted_sentence() {
        let paragraph = "Balaam loved the wages of unrighteousness. The sin of covetousness \
                         had made him a timeserver. Many flatter themselves that they can depart \
                         from strict integrity for a time, for the sake of some worldly advantage.";
        let window = "many flatter themselves that they can depart from strict integrity";
        let run = longest_shared_content_run(window, paragraph);
        assert!(run.len >= 6, "expected a strong run, got {}", run.len);
        let tail = &paragraph[run.paragraph_byte_start..];
        assert!(
            tail.starts_with("Many flatter themselves"),
            "anchor landed on {:?}",
            &tail[..tail.len().min(40)]
        );
    }

    #[test]
    fn opposite_polarity_is_not_quote_evidence() {
        let paragraph = "The shepherd does not remain in the fold waiting for the wandering sheep.";
        let opposite = "The shepherd does remain in the fold waiting for the wandering sheep.";
        assert_eq!(longest_shared_content_run(opposite, paragraph).len, 0);
    }

    #[test]
    fn spoken_verse_scaffolding_breaks_a_quote_run() {
        let paragraph = "The shepherd does not remain in the fold waiting for the wandering sheep to return of itself.";
        let interrupted =
            "the shepherd does not remain in the fold verse 12 waiting for the wandering sheep to return";
        let spliced =
            "the shepherd does not remain in the fold waiting for the wandering sheep to return";
        let interrupted_len = longest_shared_content_run(interrupted, paragraph).len;
        let spliced_len = longest_shared_content_run(spliced, paragraph).len;
        assert!(spliced_len >= 8, "got {spliced_len}");
        assert!(
            interrupted_len < spliced_len,
            "scaffolding must not splice fragments ({interrupted_len} vs {spliced_len})"
        );
    }
}
