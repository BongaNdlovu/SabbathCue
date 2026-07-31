//! Pure, stateless detection logic peeled out of the live detection loop.
//!
//! Nothing here touches `AppHandle`, managed state, or IPC — these are the
//! transcript-window, reading-mode-decision, and reading-scope-filter helpers
//! that the live loop in `detection.rs` calls into. Keeping them separate makes
//! them trivially unit-testable and shrinks the orchestration surface.

use rhema_detection::{MergedDetection, VerseRef};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DirectReadingCandidate {
    pub(crate) verse_ref: VerseRef,
    pub(crate) confidence: f64,
    pub(crate) is_chapter_only: bool,
}

/// Return the last `max_words` whitespace-delimited words of `text`, re-joined
/// with single spaces.
pub(crate) fn clamp_to_recent_words(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let start = words.len().saturating_sub(max_words);
    words[start..].join(" ")
}

/// Minimum words that must remain for `trim_to_sentence_start` to drop a
/// leading partial sentence. Below this, the mixed window is still a better
/// query than a fragment.
pub(crate) const SENTENCE_TRIM_MIN_WORDS: usize = 6;

/// Drop leading partial sentences from a rolling transcript window so the
/// semantic query starts at a sentence boundary.
///
/// The fixed-width window usually straddles two spoken sentences (the tail of
/// the previous verse plus the head of the current one), which dilutes BM25
/// and the embedding enough that neither verse surfaces. Trimming to the last
/// sentence start turns "One, two, testing. The Lord is my shepherd; I shall
/// not want" into a clean verse query. Keeps the mixed window when fewer than
/// `min_words` would remain.
pub(crate) fn trim_to_sentence_start(text: &str, min_words: usize) -> String {
    let mut current = text.trim();
    while let Some(idx) = current.find(['.', '?', '!']) {
        let rest = current[idx + 1..].trim_start();
        if rest.split_whitespace().count() < min_words {
            break;
        }
        current = rest;
    }
    current.to_string()
}

/// Strip reference-navigation scaffolding ("chapter", "verse", "it says", and
/// bare numbers) from a transcript window before it is used to build FTS5 /
/// vector search queries.
///
/// When a preacher reads a reference aloud ("chapter 7 verse 9 it says ...")
/// those framing words otherwise dominate BM25 matching and pollute the
/// embedding, surfacing irrelevant verses. The spoken reference itself is
/// already owned by the direct path, so only the surrounding verse content
/// should drive paraphrase search. Original casing of kept tokens is preserved;
/// the displayed transcript is unaffected.
pub(crate) fn strip_reference_scaffolding(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let cores: Vec<String> = tokens
        .iter()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect();
    let mut out: Vec<&str> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let core = cores[i].as_str();
        if core.is_empty() {
            continue;
        }
        let digits: String = core.chars().filter(|c| !matches!(c, ',' | '.')).collect();
        let is_number = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit());
        let prev = i.checked_sub(1).map(|p| cores[p].as_str());
        let next = cores.get(i + 1).map(String::as_str);
        let is_scaffold = matches!(
            core,
            "chapter" | "chapters" | "verse" | "verses" | "hoofstuk" | "hoofstukke" | "vers"
        ) || (core == "it" && next == Some("says"))
            || (core == "says" && prev == Some("it"));
        if !is_number && !is_scaffold {
            out.push(token);
        }
    }
    out.join(" ")
}

/// Book number when the window names exactly one book without a complete
/// chapter:verse reference. Returns `None` when no book is named, when several
/// are (e.g. "Malachi … Esther …"), or when a complete reference is present —
/// complete refs are owned by the direct path.
///
/// Uses `BookMatcher` + `parse_reference`, not `DirectDetector::detect`: bare
/// book names are held incomplete and never emitted as detections
/// (`direct/detector.rs` incomplete-ref path).
pub(crate) fn spoken_book_hint(transcript: &str) -> Option<i32> {
    use std::sync::LazyLock;

    use rhema_detection::direct::automaton::BookMatcher;
    use rhema_detection::direct::parser::parse_reference;

    static MATCHER: LazyLock<BookMatcher> = LazyLock::new(BookMatcher::new);
    let matches = MATCHER.find_books(transcript);
    if matches.is_empty() {
        return None;
    }
    let mut books: Vec<i32> = matches.iter().map(|m| m.book_number).collect();
    books.sort_unstable();
    books.dedup();
    if books.len() != 1 {
        return None;
    }
    let book = books[0];
    for book_match in &matches {
        if let Some(vr) = parse_reference(transcript, book_match) {
            if vr.verse_start > 0 {
                // Complete reference — direct path owns it.
                return None;
            }
        }
    }
    Some(book)
}

/// True when the transcript window is an explicit scripture reference or a
/// voice/reading command that the direct + command paths already handle.
///
/// Live semantic (fuzzy) search defers to those paths for such utterances, so
/// the detections panel reflects what was actually spoken instead of keyword
/// noise from BM25 matching on reference words like "chapter"/"verse".
pub(crate) fn transcript_defers_to_direct(text: &str) -> bool {
    crate::commands::transcript_router::looks_like_complete_reference(text)
        || rhema_detection::is_voice_command_utterance(text)
}

pub(crate) fn is_direct_reading_handoff(detection: &rhema_detection::Detection) -> bool {
    detection.confidence >= 0.90 || detection.is_chapter_only
}

pub(crate) fn direct_reading_candidates(merged: &[MergedDetection]) -> Vec<DirectReadingCandidate> {
    merged
        .iter()
        .filter(|merged| is_direct_reading_handoff(&merged.detection))
        .map(|merged| DirectReadingCandidate {
            verse_ref: merged.detection.verse_ref.clone(),
            confidence: merged.detection.confidence,
            is_chapter_only: merged.detection.is_chapter_only,
        })
        .collect()
}

pub(crate) fn choose_reading_candidate(
    candidates: &[DirectReadingCandidate],
    active_scope: Option<(i32, i32)>,
) -> Option<DirectReadingCandidate> {
    if let Some((book_number, chapter)) = active_scope {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            candidate.verse_ref.book_number == book_number && candidate.verse_ref.chapter == chapter
        }) {
            return Some(candidate.clone());
        }

        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.verse_ref.book_number == book_number)
        {
            return Some(candidate.clone());
        }
    }

    candidates.first().cloned()
}

/// Decide whether a fresh direct detection should (re)start reading mode.
///
/// Same book+chapter normally means "already tracking this" — but a specific
/// verse reference (not a bare chapter default) that names a different verse
/// than the current position re-anchors reading mode to it. Without this, a
/// chapter-only hit ("Malachi 3" → 3:1) pins the cursor at verse 1 even after
/// the speaker announces "verses 16-18", and stray word-overlap can then
/// false-advance to a nearby low verse. Chapter-only hits never re-anchor, so
/// this cannot thrash the cursor back to verse 1.
pub(crate) fn should_restart_reading(
    active: bool,
    current_book: i32,
    current_chapter: i32,
    current_verse: Option<i32>,
    candidate: &DirectReadingCandidate,
) -> bool {
    let recent = &candidate.verse_ref;

    if !active {
        // Not active (fresh or paused) — any explicit reference (re)starts.
        return true;
    }

    if current_book == recent.book_number && current_chapter == recent.chapter {
        // Re-anchor only to a specific verse ahead of where we are. Stale STT
        // windows can replay the original lower verse after reading mode moves on.
        return !candidate.is_chapter_only
            && match current_verse {
                Some(current) => recent.verse_start > current,
                None => true,
            };
    }

    if current_book != recent.book_number {
        // Different book — only an explicit, high-confidence reference restarts.
        return candidate.confidence >= 0.90 || candidate.is_chapter_only;
    }

    // Same book, different chapter — natural progression.
    true
}

/// No verse match in the reading chapter for this long means the speaker has
/// likely moved on; the scope may be released by an out-of-scope semantic hit.
pub(crate) const READING_SCOPE_STALE_SECS: u64 = 20;

/// Short live-speech pause before repeated out-of-scope semantic hits may
/// release reading scope at the operator's threshold. Single hits still wait
/// for `READING_SCOPE_STALE_SECS`.
pub(crate) const READING_SCOPE_LIVE_PAUSE_SECS: u64 = 6;

/// Minimum confidence for an out-of-scope semantic hit to release the reading
/// scope via the fast consecutive-hit streak (before the scope is stale).
/// Kept well above `LIVE_SEMANTIC_MIN_CONFIDENCE` so keyword noise never
/// unlocks the scope mid-reading. The stale-release path uses the operator's
/// live semantic threshold instead.
pub(crate) const READING_SCOPE_RELEASE_MIN_CONFIDENCE: f64 = 0.85;

/// While reading mode is advancing, out-of-scope semantic hits are
/// parallel-passage echoes and stay suppressed. Once the scope is stale (no
/// verse matched for `READING_SCOPE_STALE_SECS` — including never, when a bare
/// citation anchored the scope but the speaker moved on without reading), a
/// semantic hit outside the anchored chapter — a different book, or another
/// chapter of the same book — means the speaker has moved on and the scope
/// should be released instead of blanket-suppressing until the reading-mode
/// timeout.
///
/// `min_confidence` is the operator's live semantic threshold: any hit that
/// clears it would be SHOWN were the scope not active, so it is strong enough
/// evidence to release a scope that is already stale. (A fixed 0.85 bar here
/// left real quote-overlap hits at ~0.79 suppressed for the full timeout.)
pub(crate) fn should_release_stale_reading_scope(
    results: &[crate::commands::detection::DetectionResult],
    scope_book_number: i32,
    scope_chapter: i32,
    seconds_since_last_verse_match: u64,
    min_confidence: f64,
) -> bool {
    seconds_since_last_verse_match >= READING_SCOPE_STALE_SECS
        && out_of_scope_bible_book(results, scope_book_number, scope_chapter, min_confidence)
            .is_some()
}

/// Book+chapter eligible for a repeated-hit live-pause release. The caller must
/// still require the same out-of-scope scope to repeat before releasing.
pub(crate) fn live_pause_out_of_scope_bible_book(
    results: &[crate::commands::detection::DetectionResult],
    scope_book_number: i32,
    scope_chapter: i32,
    seconds_since_last_verse_match: u64,
    min_confidence: f64,
) -> Option<(i32, i32)> {
    if seconds_since_last_verse_match < READING_SCOPE_LIVE_PAUSE_SECS {
        return None;
    }

    out_of_scope_bible_book(results, scope_book_number, scope_chapter, min_confidence)
}

/// Consecutive strong out-of-scope hits on the same book+chapter needed to
/// release the reading scope before the staleness clock. One echo pass is
/// noise; a repeat on the same passage means the speaker has moved there.
pub(crate) const READING_SCOPE_RELEASE_STREAK: u32 = 2;

/// Book+chapter of a semantic Bible hit outside the anchored book+chapter
/// scope at or above `min_confidence`, if any. Same-book hits in a *different
/// chapter* count: reading a chapter aloud does not strongly match its
/// sibling chapters, so such a hit is real navigation, not an echo.
pub(crate) fn out_of_scope_bible_book(
    results: &[crate::commands::detection::DetectionResult],
    scope_book_number: i32,
    scope_chapter: i32,
    min_confidence: f64,
) -> Option<(i32, i32)> {
    results
        .iter()
        .find(|result| {
            result.content_type == "bible"
                && (result.book_number != scope_book_number || result.chapter != scope_chapter)
                && result.confidence >= min_confidence
        })
        .map(|result| (result.book_number, result.chapter))
}

/// Strong out-of-scope hit for the fast streak release, which fires while the
/// scope is NOT yet stale — the high bar keeps parallel-passage echoes from
/// unlocking suppression during genuine reading.
pub(crate) fn strong_out_of_scope_bible_book(
    results: &[crate::commands::detection::DetectionResult],
    scope_book_number: i32,
    scope_chapter: i32,
) -> Option<(i32, i32)> {
    out_of_scope_bible_book(
        results,
        scope_book_number,
        scope_chapter,
        READING_SCOPE_RELEASE_MIN_CONFIDENCE,
    )
}

pub(crate) fn filter_semantic_results_to_reading_scope(
    results: Vec<crate::commands::detection::DetectionResult>,
    scope: Option<(i32, i32)>,
) -> Vec<crate::commands::detection::DetectionResult> {
    let Some((book_number, chapter)) = scope else {
        return results;
    };

    results
        .into_iter()
        .filter(|result| {
            result.content_type != "bible"
                || (result.book_number == book_number && result.chapter == chapter)
        })
        .collect()
}

pub(crate) fn filter_direct_results_to_scope_if_present(
    results: Vec<crate::commands::detection::DetectionResult>,
    scope: Option<(i32, i32)>,
) -> Vec<crate::commands::detection::DetectionResult> {
    let Some((book_number, chapter)) = scope else {
        return results;
    };

    let has_active_match = results.iter().any(|result| {
        result.content_type == "bible"
            && result.book_number == book_number
            && result.chapter == chapter
    });
    if !has_active_match {
        return results;
    }

    results
        .into_iter()
        .filter(|result| {
            result.content_type != "bible"
                || (result.book_number == book_number && result.chapter == chapter)
        })
        .collect()
}
