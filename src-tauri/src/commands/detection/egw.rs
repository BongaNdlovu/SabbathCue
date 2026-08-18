use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use rhema_bible::EgwBook;

use crate::state::AppState;

use super::result::{egw_to_result, DetectionResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParsedNumber {
    value: i32,
    next_index: usize,
}

fn normalize_reference_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

use rhema_detection::{egw_quote_score, longest_shared_content_run};

fn integer_token(token: &str) -> Option<i32> {
    if token.chars().all(|ch| ch.is_ascii_digit()) {
        return token.parse::<i32>().ok().filter(|value| *value > 0);
    }
    None
}

fn unit_word(token: &str) -> Option<i32> {
    match token {
        "one" | "first" => Some(1),
        "two" | "second" => Some(2),
        "three" | "third" => Some(3),
        "four" | "fourth" => Some(4),
        "five" | "fifth" => Some(5),
        "six" | "sixth" => Some(6),
        "seven" | "seventh" => Some(7),
        "eight" | "eighth" => Some(8),
        "nine" | "ninth" => Some(9),
        _ => None,
    }
}

fn teen_word(token: &str) -> Option<i32> {
    match token {
        "ten" | "tenth" => Some(10),
        "eleven" | "eleventh" => Some(11),
        "twelve" | "twelfth" => Some(12),
        "thirteen" | "thirteenth" => Some(13),
        "fourteen" | "fourteenth" => Some(14),
        "fifteen" | "fifteenth" => Some(15),
        "sixteen" | "sixteenth" => Some(16),
        "seventeen" | "seventeenth" => Some(17),
        "eighteen" | "eighteenth" => Some(18),
        "nineteen" | "nineteenth" => Some(19),
        _ => None,
    }
}

fn tens_word(token: &str) -> Option<i32> {
    match token {
        "twenty" | "twentieth" => Some(20),
        "thirty" | "thirtieth" => Some(30),
        "forty" | "fortieth" => Some(40),
        "fifty" | "fiftieth" => Some(50),
        "sixty" | "sixtieth" => Some(60),
        "seventy" | "seventieth" => Some(70),
        "eighty" | "eightieth" => Some(80),
        "ninety" | "ninetieth" => Some(90),
        _ => None,
    }
}

fn parse_under_hundred(tokens: &[&str], index: usize) -> Option<ParsedNumber> {
    let token = tokens.get(index)?;
    if let Some(value) = integer_token(token) {
        return Some(ParsedNumber {
            value,
            next_index: index + 1,
        });
    }
    if let Some(value) = teen_word(token).or_else(|| unit_word(token)) {
        return Some(ParsedNumber {
            value,
            next_index: index + 1,
        });
    }
    let value = tens_word(token)?;
    let mut next_index = index + 1;
    let mut total = value;
    if let Some(next) = tokens.get(next_index).and_then(|next| unit_word(next)) {
        total += next;
        next_index += 1;
    }
    Some(ParsedNumber {
        value: total,
        next_index,
    })
}

fn parse_number_at(tokens: &[&str], index: usize) -> Option<ParsedNumber> {
    let first = parse_under_hundred(tokens, index)?;
    if tokens.get(first.next_index) != Some(&"hundred") {
        return Some(first);
    }

    let mut value = first.value * 100;
    let mut next_index = first.next_index + 1;
    if let Some(remainder) = parse_under_hundred(tokens, next_index) {
        value += remainder.value;
        next_index = remainder.next_index;
    }
    Some(ParsedNumber { value, next_index })
}

fn is_reference_filler(token: &str) -> bool {
    matches!(
        token,
        "book"
            | "of"
            | "the"
            | "page"
            | "pages"
            | "paragraph"
            | "paragraphs"
            | "number"
            | "no"
            | "ellen"
            | "white"
            | "egw"
            | "read"
            | "from"
            | "go"
            | "to"
    )
}

fn parse_next_number(tokens: &[&str], start_index: usize) -> Option<ParsedNumber> {
    let mut index = start_index;
    while index < tokens.len() {
        if let Some(parsed) = parse_number_at(tokens, index) {
            return Some(parsed);
        }
        if !is_reference_filler(tokens[index]) {
            return None;
        }
        index += 1;
    }
    None
}

fn parse_number_after_label(tokens: &[&str], labels: &[&str]) -> Option<ParsedNumber> {
    for (index, token) in tokens.iter().enumerate() {
        if labels.contains(token) {
            if let Some(parsed) = parse_next_number(tokens, index + 1) {
                return Some(parsed);
            }
        }
    }
    None
}

fn parse_egw_page_paragraph(tail: &str) -> Option<(i32, i32)> {
    let tokens: Vec<&str> = tail.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let page = parse_number_after_label(&tokens, &["page", "pages"])?;
    let paragraph = parse_number_after_label(&tokens, &["paragraph", "paragraphs"])?;
    Some((page.value, paragraph.value))
}

fn alias_match_end(text: &str, alias: &str) -> Option<usize> {
    if alias.is_empty() {
        return None;
    }
    for (index, _) in text.match_indices(alias) {
        let before_ok = index == 0 || text.as_bytes().get(index - 1) == Some(&b' ');
        let end = index + alias.len();
        let after_ok = end == text.len() || text.as_bytes().get(end) == Some(&b' ');
        if before_ok && after_ok {
            return Some(end);
        }
    }
    None
}

fn egw_aliases(book: &EgwBook) -> Vec<String> {
    let mut aliases = Vec::new();
    let alias = normalize_reference_text(&book.title);
    if !alias.is_empty() {
        aliases.push(alias.clone());
        if let Some(without_the) = alias.strip_prefix("the ") {
            if !without_the.is_empty() {
                aliases.push(without_the.to_string());
            }
        }
    }
    aliases
}

fn best_egw_alias_match<'a>(
    normalized_text: &str,
    books: &'a [EgwBook],
) -> Vec<(&'a EgwBook, usize, usize)> {
    let mut matches = books
        .iter()
        .flat_map(|book| {
            egw_aliases(book).into_iter().filter_map(move |alias| {
                alias_match_end(normalized_text, &alias).map(|end| (book, end, alias.len()))
            })
        })
        .collect::<Vec<_>>();

    matches.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    matches
}

/// Spoken phrases that attribute what follows to Ellen G. White.
///
/// "white" alone is excluded on purpose: white robes, white as snow and white
/// horses are common sermon imagery, and a bare colour word is not attribution.
const EGW_CUE_PHRASES: [&str; 12] = [
    "ellen white",
    "ellen g white",
    "ellen g writes",
    "ellen writes",
    "sister white",
    "spirit of prophecy",
    // Soniox/Deepgram mishears of "Ellen G. White" / "Ellen White writes"
    "lng writes",
    "lng white",
    "ln g white",
    "l n g white",
    "statement by illinois",
    "statement from ellen",
];

/// True when the window attributes its content to Ellen G. White, either by
/// naming her or by naming one of the imported books.
pub(crate) fn transcript_has_egw_cue(books: &[EgwBook], text: &str) -> bool {
    let normalized = normalize_reference_text(text);
    if normalized.is_empty() {
        return false;
    }
    if EGW_CUE_PHRASES
        .iter()
        .any(|phrase| normalized.contains(phrase))
    {
        return true;
    }
    books.iter().any(|book| {
        egw_aliases(book).into_iter().any(|alias| {
            alias.split_whitespace().count() >= 2 && alias_match_end(&normalized, &alias).is_some()
        })
    })
}

/// How long attribution keeps lowering the run-length bar.
const EGW_CUE_TTL_MS: u64 = 90_000;

fn cue_is_live(now_ms: u64, cue_at_ms: u64) -> bool {
    cue_at_ms > 0 && now_ms.saturating_sub(cue_at_ms) <= EGW_CUE_TTL_MS
}

/// Whether a previously spoken EGW attribution is still in force.
pub(crate) fn egw_cue_is_currently_live(cue_at_ms: &AtomicU64) -> bool {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
    cue_is_live(now_ms, cue_at_ms.load(Ordering::Relaxed))
}

/// Keep only the strongest EGW hit for live emission.
///
/// BM25 nominates several paragraphs; shared-run scoring can pass more than one
/// (e.g. PP p.325 at 88% and Desire of Ages p.327 at 75%). Emitting both lets
/// weaker wrong-book hits thrash the preview. Sort by confidence then rank.
pub(crate) fn retain_best_egw_quote(results: &mut Vec<DetectionResult>) {
    if results.len() <= 1 {
        return;
    }
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.rank_score
                    .partial_cmp(&a.rank_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    results.truncate(1);
}

/// Record a cue if this window carries one, and report whether attribution is
/// currently in force. A quotation typically gets attributed once and then runs
/// for several windows, so the cue outlives the window that carried it.
pub(crate) fn note_and_check_egw_cue(
    books: &[EgwBook],
    text: &str,
    now_ms: u64,
    cue_at_ms: &AtomicU64,
) -> bool {
    if transcript_has_egw_cue(books, text) {
        cue_at_ms.store(now_ms, Ordering::Relaxed);
        return true;
    }
    cue_is_live(now_ms, cue_at_ms.load(Ordering::Relaxed))
}

/// STT confidence below this leaves the transcript too unreliable to present
/// an EGW quote unattended. 0.0 means the provider reported no confidence.
const EGW_LOW_STT_CONFIDENCE: f64 = 0.65;
/// Both mirror the Bible semantic dampening in `run_semantic_detection`.
const EGW_LOW_STT_RANK_SCALE: f64 = 0.85;
const EGW_LOW_STT_CONFIDENCE_CAP: f64 = 0.89;

/// Dampen EGW quote results when the STT confidence for the window was low.
///
/// Mirrors the Bible semantic dampening, and additionally clears `auto_queued`.
///
/// Bible semantic results need no such clause because they can never carry the
/// flag: `DetectionMerger::merge` only marks `auto_queued` for
/// `DetectionSource::DirectReference`. EGW quotes are the one semantic source
/// that sets it directly, from run length, without passing through that
/// eligibility rule — so capping confidence alone would leave a hit reading
/// 0.89 that still presents itself with no operator click.
pub(crate) fn dampen_egw_for_low_stt_confidence(
    results: &mut [DetectionResult],
    stt_confidence: f64,
) {
    if stt_confidence <= 0.0 || stt_confidence >= EGW_LOW_STT_CONFIDENCE {
        return;
    }
    for result in results {
        result.rank_score *= EGW_LOW_STT_RANK_SCALE;
        result.confidence = result.confidence.min(EGW_LOW_STT_CONFIDENCE_CAP);
        result.auto_queued = false;
    }
}

/// Drop EGW quote hits that the scripture in the window already explains.
///
/// EGW paragraphs quote scripture verbatim — The Desire of Ages p.419 par.2
/// carries John 3:16 word for word. Reading that verse aloud therefore shares a
/// long run with the paragraph and fires it at the 0.92 cap: on 2026-08-04 that
/// put 33 Desire of Ages hits in the operator's box while the speaker was
/// simply reading John 3:16.
///
/// The discriminator is which source explains the spoken words better. When the
/// Bible verse matches the transcript at least as well as the paragraph does,
/// the speaker is reading scripture and the paragraph is an echo. A genuine
/// Ellen White quote outruns every verse in the window, so it survives.
///
/// When an EGW attribution cue is live ("Ellen White", "LNG Writes", …), do
/// **not** drop paragraphs: residual scripture from the prior verse in the
/// rolling window would otherwise suppress the real EGW quote for many seconds
/// (live 2026-08-04: Adam/Eve law quote delayed until John 3 reading released).
pub(crate) fn drop_egw_quotes_echoing_scripture(
    egw_results: &mut Vec<DetectionResult>,
    bible_results: &[DetectionResult],
    spoken: &str,
    cue_active: bool,
) {
    if egw_results.is_empty() {
        return;
    }
    if cue_active {
        return;
    }
    let best_verse_run = bible_results
        .iter()
        .filter(|result| result.content_type != "egw")
        .map(|result| longest_shared_content_run(spoken, &result.verse_text).len)
        .max()
        .unwrap_or(0);
    if best_verse_run == 0 {
        return;
    }
    egw_results.retain(|result| {
        let egw_run = longest_shared_content_run(spoken, &result.verse_text).len;
        if egw_run <= best_verse_run {
            log::info!(
                "[DET-EGW-QUOTE] Dropped {} (run={egw_run}) - scripture echo, best verse run={best_verse_run}",
                result.verse_ref
            );
            return false;
        }
        true
    });
}

/// Minimum spoken words before a quote search is worth running.
const EGW_QUOTE_MIN_WORDS: usize = 5;
/// BM25 nominates this many candidates; run-length verification decides.
/// Raised from 1 after live 2026-08-04: a single BM25 nominee often missed the
/// spoken paragraph while residual John 3:16 text still dominated retrieval.
const EGW_QUOTE_CANDIDATES: usize = 5;

/// Detect an EGW paragraph being read aloud in the transcript window.
///
/// BM25 nominates; `longest_shared_content_run` verifies. Every candidate is
/// logged with its run length and cue state — including rejections — so field
/// calibration needs no rebuild.
pub(crate) fn detect_egw_quotes(
    state: &AppState,
    text: &str,
    cue_active: bool,
) -> Vec<DetectionResult> {
    if text.split_whitespace().count() < EGW_QUOTE_MIN_WORDS {
        return Vec::new();
    }
    let Some(db) = state.bible_db.as_ref() else {
        return Vec::new();
    };

    let paragraphs = match db.search_egw_bm25(text, EGW_QUOTE_CANDIDATES) {
        Ok(paragraphs) => paragraphs,
        Err(error) => {
            log::warn!("[DET-EGW-QUOTE] BM25 search failed: {error}");
            return Vec::new();
        }
    };

    paragraphs
        .into_iter()
        .filter_map(|paragraph| {
            let run = longest_shared_content_run(text, &paragraph.text);
            let scored = egw_quote_score(run.len, cue_active);
            log::debug!(
                "[DET-EGW-QUOTE] candidate {} p.{} par.{} run={} cue={cue_active} verdict={}",
                paragraph.book_title,
                paragraph.page,
                paragraph.page_paragraph,
                run.len,
                match scored {
                    Some((confidence, auto_queued)) =>
                        format!("{:.0}% auto_q={auto_queued}", confidence * 100.0),
                    None => "drop".to_string(),
                }
            );
            let (confidence, auto_queued) = scored?;
            let mut result = egw_to_result(paragraph, confidence, text);
            result.source = "semantic".to_string();
            result.rank_score = confidence;
            result.auto_queued = auto_queued;
            result.match_char_start = Some(run.paragraph_byte_start);
            Some(result)
        })
        .collect()
}

/// Detect explicit Ellen G. White paragraph references like
/// `Patriarchs and Prophets page twenty nine paragraph two`.
pub(crate) fn detect_egw_references(state: &AppState, text: &str) -> Vec<DetectionResult> {
    let Some(db) = state.bible_db.as_ref() else {
        return Vec::new();
    };
    let books = match db.list_egw_books() {
        Ok(books) => books,
        Err(error) => {
            log::warn!("[DET-EGW] Failed to load EGW books for direct detection: {error}");
            return Vec::new();
        }
    };
    if books.is_empty() {
        log::debug!("[DET-EGW] No EGW books imported; EGW detection disabled");
        return Vec::new();
    }

    let normalized = normalize_reference_text(text);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for (book, alias_end, _) in best_egw_alias_match(&normalized, &books) {
        let tail = normalized.get(alias_end..).unwrap_or_default().trim();
        let Some((page, paragraph_number)) = parse_egw_page_paragraph(tail) else {
            continue;
        };
        if page <= 0 || paragraph_number <= 0 {
            continue;
        }
        if !seen.insert((book.book_number, page, paragraph_number)) {
            continue;
        }

        match db.get_egw_paragraph_by_page(book.book_number, page, paragraph_number) {
            Ok(Some(paragraph)) => {
                results.push(egw_to_result(paragraph, 0.94, text));
            }
            Ok(None) => {}
            Err(error) => {
                log::warn!(
                    "[DET-EGW] Failed to resolve {} p.{} par.{}: {error}",
                    book.title,
                    page,
                    paragraph_number
                );
            }
        }
    }
    results
}

pub(crate) fn apply_egw_auto_queue(
    results: &mut [DetectionResult],
    merger: &mut rhema_detection::DetectionMerger,
) {
    // Semantic quotes are prequalified by shared-run length (run ≥ 8 + cue).
    // Apply only the configured auto-queue threshold / Manual mode — do NOT
    // push them through merger cooldown. Live 2026-08-04 21:30: continuous
    // partials for the same PP p.322 hit scored auto_q=true on 8 of 88 Found
    // lines; the 2.5s cooldown stripped the rest and starved unattended queue.
    let threshold = merger.auto_queue_threshold();
    for result in results.iter_mut() {
        if result.content_type == "egw" && result.source == "semantic" && result.auto_queued {
            result.auto_queued = result.confidence >= threshold;
        }
    }

    // Direct page/paragraph references still use the full merger policy
    // (threshold + cooldown) so rapid page flips do not flood auto-queue.
    let eligible_indices: Vec<usize> = results
        .iter()
        .enumerate()
        .filter_map(|(index, result)| {
            (result.content_type == "egw" && result.source == "direct").then_some(index)
        })
        .collect();

    if eligible_indices.is_empty() {
        return;
    }

    let candidates: Vec<rhema_detection::Detection> = eligible_indices
        .iter()
        .map(|index| {
            let result = &results[*index];
            rhema_detection::Detection {
                verse_ref: rhema_detection::VerseRef {
                    book_number: result.book_number,
                    book_name: result.book_name.clone(),
                    chapter: result.chapter,
                    verse_start: result.verse,
                    verse_end: None,
                },
                verse_id: None,
                confidence: result.confidence,
                source: rhema_detection::DetectionSource::DirectReference,
                transcript_snippet: result.transcript_snippet.clone(),
                detected_at: 0,
                is_chapter_only: false,
                ..rhema_detection::Detection::default()
            }
        })
        .collect();

    let auto_by_ref: HashMap<(i32, i32, i32), bool> = merger
        .merge(candidates, vec![])
        .into_iter()
        .map(|merged| {
            let verse_ref = merged.detection.verse_ref;
            (
                (
                    verse_ref.book_number,
                    verse_ref.chapter,
                    verse_ref.verse_start,
                ),
                merged.auto_queued,
            )
        })
        .collect();

    for index in eligible_indices {
        let result = &mut results[index];
        result.auto_queued = auto_by_ref
            .get(&(result.book_number, result.chapter, result.verse))
            .copied()
            .unwrap_or(false);
    }
}

#[cfg(test)]
mod low_stt_dampening_tests {
    use super::{
        dampen_egw_for_low_stt_confidence, drop_egw_quotes_echoing_scripture, egw_to_result,
    };
    use crate::commands::detection::DetectionResult;
    use rhema_bible::EgwParagraph;

    fn auto_queued_hit() -> DetectionResult {
        let paragraph = EgwParagraph {
            id: 3,
            book_number: 2,
            book_title: "The Desire of Ages".to_string(),
            chapter: 15,
            chapter_title: "The Shepherd And His Flock".to_string(),
            paragraph: 4,
            page: 480,
            page_paragraph: 1,
            text: "The shepherd does not remain in the fold.".to_string(),
        };
        let mut result = egw_to_result(paragraph, 0.92, "the shepherd does not remain in the fold");
        result.source = "semantic".to_string();
        result.rank_score = 0.92;
        result.auto_queued = true;
        result
    }

    fn egw_hit_with_text(title: &str, text: &str) -> DetectionResult {
        let paragraph = EgwParagraph {
            id: 9,
            book_number: 2,
            book_title: title.to_string(),
            chapter: 12,
            chapter_title: "Nicodemus".to_string(),
            paragraph: 2,
            page: 419,
            page_paragraph: 2,
            text: text.to_string(),
        };
        let mut result = egw_to_result(paragraph, 0.92, text);
        result.source = "semantic".to_string();
        result.rank_score = 0.92;
        result
    }

    fn bible_hit_with_text(reference: &str, text: &str) -> DetectionResult {
        let mut result = egw_hit_with_text("unused", text);
        result.content_type = "bible".to_string();
        result.verse_ref = reference.to_string();
        result.egw_paragraph = None;
        result
    }

    #[test]
    fn scripture_echo_drops_an_egw_paragraph_that_merely_quotes_the_verse() {
        // 2026-08-04: reading John 3:16 aloud fired The Desire of Ages p.419
        // par.2 at 92% (run=14) because that paragraph quotes the verse.
        let spoken = "For God so loved the world that he gave his only begotten Son that whosoever believeth in him should not perish but have everlasting life";
        let mut egw = vec![egw_hit_with_text(
            "The Desire of Ages",
            "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. In this verse the Saviour unfolds the plan of redemption.",
        )];
        let bible = vec![bible_hit_with_text("John 3:16", spoken)];

        drop_egw_quotes_echoing_scripture(&mut egw, &bible, spoken, false);

        assert!(
            egw.is_empty(),
            "a paragraph explained by the spoken verse must not reach the operator"
        );
    }

    #[test]
    fn scripture_echo_skips_drop_when_egw_cue_is_active() {
        let spoken = "For God so loved the world that he gave his only begotten Son";
        let mut egw = vec![egw_hit_with_text(
            "The Desire of Ages",
            "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.",
        )];
        let bible = vec![bible_hit_with_text("John 3:16", spoken)];

        drop_egw_quotes_echoing_scripture(&mut egw, &bible, spoken, true);

        assert_eq!(
            egw.len(),
            1,
            "with an EGW cue, residual scripture in the window must not suppress EGW"
        );
    }

    #[test]
    fn scripture_echo_keeps_a_genuine_egw_quote() {
        // The Great Controversy quote from the same session: no verse in the
        // window explains it, so it must survive.
        let spoken = "Fearful is the issue to which the world is to be brought the powers of earth uniting to war against the commandment of God";
        let mut egw = vec![egw_hit_with_text(
            "The Great Controversy",
            "Fearful is the issue to which the world is to be brought. The powers of earth, uniting to war against the commandments of God, will decree that all shall conform to custom.",
        )];
        let bible = vec![bible_hit_with_text(
            "I Timothy 1:1",
            "Paul, an apostle of Jesus Christ by the commandment of God our Saviour, and Lord Jesus Christ, which is our hope;",
        )];

        drop_egw_quotes_echoing_scripture(&mut egw, &bible, spoken, false);

        assert_eq!(
            egw.len(),
            1,
            "a genuine Ellen White quote outruns every verse in the window"
        );
    }

    #[test]
    fn low_confidence_caps_scores_and_revokes_auto_queue() {
        let mut results = vec![auto_queued_hit()];

        dampen_egw_for_low_stt_confidence(&mut results, 0.5);

        assert!((results[0].confidence - 0.89).abs() < 1e-9);
        assert!((results[0].rank_score - 0.92 * 0.85).abs() < 1e-9);
        assert!(
            !results[0].auto_queued,
            "a garbled window must not present unattended"
        );
    }

    #[test]
    fn unreported_confidence_leaves_the_hit_alone() {
        // 0.0 means the provider gave no confidence, not "no confidence".
        let mut results = vec![auto_queued_hit()];

        dampen_egw_for_low_stt_confidence(&mut results, 0.0);

        assert!((results[0].confidence - 0.92).abs() < 1e-9);
        assert!((results[0].rank_score - 0.92).abs() < 1e-9);
        assert!(results[0].auto_queued);
    }

    #[test]
    fn threshold_boundary_matches_the_bible_block() {
        let mut at_threshold = vec![auto_queued_hit()];
        dampen_egw_for_low_stt_confidence(&mut at_threshold, 0.65);
        assert!(at_threshold[0].auto_queued, "0.65 is not low confidence");

        let mut just_below = vec![auto_queued_hit()];
        dampen_egw_for_low_stt_confidence(&mut just_below, 0.649);
        assert!(!just_below[0].auto_queued);
    }

    #[test]
    fn high_confidence_leaves_the_hit_alone() {
        let mut results = vec![auto_queued_hit()];

        dampen_egw_for_low_stt_confidence(&mut results, 0.95);

        assert!((results[0].confidence - 0.92).abs() < 1e-9);
        assert!(results[0].auto_queued);
    }

    #[test]
    fn the_cap_never_raises_a_weaker_hit() {
        let mut results = vec![auto_queued_hit()];
        results[0].confidence = 0.78;
        results[0].rank_score = 0.78;
        results[0].auto_queued = false;

        dampen_egw_for_low_stt_confidence(&mut results, 0.5);

        assert!(
            (results[0].confidence - 0.78).abs() < 1e-9,
            "cap is a ceiling, not an assignment"
        );
        assert!((results[0].rank_score - 0.78 * 0.85).abs() < 1e-9);
    }
}

#[cfg(test)]
mod auto_queue_policy_tests {
    use super::{apply_egw_auto_queue, egw_to_result};
    use rhema_bible::EgwParagraph;
    use rhema_detection::DetectionMerger;

    fn prequalified_semantic_quote() -> super::DetectionResult {
        let paragraph = EgwParagraph {
            id: 3,
            book_number: 2,
            book_title: "The Desire of Ages".to_string(),
            chapter: 15,
            chapter_title: "The Shepherd And His Flock".to_string(),
            paragraph: 4,
            page: 480,
            page_paragraph: 1,
            text: "The shepherd does not remain in the fold.".to_string(),
        };
        let mut result = egw_to_result(paragraph, 0.92, "quoted text");
        result.source = "semantic".to_string();
        result.auto_queued = true;
        result
    }

    #[test]
    fn manual_mode_revokes_semantic_quote_auto_queue() {
        let mut results = vec![prequalified_semantic_quote()];
        let mut merger = DetectionMerger::new();
        merger.set_auto_queue_threshold(f64::INFINITY);

        apply_egw_auto_queue(&mut results, &mut merger);

        assert!(
            !results[0].auto_queued,
            "manual mode must revoke quote prequalification"
        );
    }

    #[test]
    fn auto_mode_keeps_a_semantic_quote_above_threshold_eligible() {
        let mut results = vec![prequalified_semantic_quote()];
        let mut merger = DetectionMerger::new();
        merger.set_auto_queue_threshold(0.90);

        apply_egw_auto_queue(&mut results, &mut merger);

        assert!(results[0].auto_queued);
    }
}

#[cfg(test)]
mod retain_best_egw_quote_tests {
    use super::{egw_to_result, retain_best_egw_quote};
    use crate::commands::detection::DetectionResult;
    use rhema_bible::EgwParagraph;

    fn hit(page: i32, conf: f64) -> DetectionResult {
        let paragraph = EgwParagraph {
            id: i64::from(page),
            book_number: 1,
            book_title: "Patriarchs and Prophets".into(),
            chapter: 1,
            chapter_title: String::new(),
            paragraph: 1,
            page,
            page_paragraph: 1,
            text: "sample".into(),
        };
        let mut result = egw_to_result(paragraph, conf, "spoken");
        result.source = "semantic".into();
        result.rank_score = conf;
        result
    }

    #[test]
    fn keeps_only_the_highest_confidence_quote() {
        let mut results = vec![hit(327, 0.75), hit(325, 0.88), hit(322, 0.80)];
        retain_best_egw_quote(&mut results);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chapter, 325); // page stored as chapter for EGW
        assert!((results[0].confidence - 0.88).abs() < f64::EPSILON);
    }
}

#[cfg(test)]
mod cue_ttl_tests {
    use super::{cue_is_live, EGW_CUE_TTL_MS};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn cue_is_live_within_the_window() {
        assert!(cue_is_live(1_000, 1_000));
        assert!(cue_is_live(1_000 + EGW_CUE_TTL_MS, 1_000));
    }

    #[test]
    fn cue_expires_after_the_window() {
        assert!(!cue_is_live(1_001 + EGW_CUE_TTL_MS, 1_000));
    }

    #[test]
    fn never_cued_is_never_live() {
        assert!(!cue_is_live(500_000, 0));
    }

    #[test]
    fn clock_going_backwards_does_not_panic_or_extend() {
        // Saturating arithmetic: an earlier `now` reads as still inside the window
        // rather than underflowing.
        assert!(cue_is_live(500, 1_000));
    }

    #[test]
    fn cue_state_is_isolated_between_sessions() {
        let previous_session = AtomicU64::new(1_000);
        let next_session = AtomicU64::new(0);

        assert!(cue_is_live(1_001, previous_session.load(Ordering::Relaxed)));
        assert!(!cue_is_live(1_001, next_session.load(Ordering::Relaxed)));
    }
}

#[cfg(test)]
mod quote_score_tests {
    use rhema_detection::egw_quote_score;

    #[test]
    fn long_run_with_cue_fires_and_auto_queues() {
        let (confidence, auto_queued) = egw_quote_score(8, true).expect("should score");
        assert!((confidence - 0.92).abs() < f64::EPSILON);
        assert!(auto_queued);
    }

    #[test]
    fn long_run_without_cue_fires_but_never_auto_queues() {
        let (confidence, auto_queued) = egw_quote_score(9, false).expect("should score");
        assert!((0.88..=0.92).contains(&confidence));
        assert!(!auto_queued);
    }

    #[test]
    fn fire_band_starts_at_six_regardless_of_cue() {
        for cue in [true, false] {
            let (confidence, auto_queued) = egw_quote_score(6, cue).expect("should score");
            assert!(confidence >= 0.88, "run 6 should fire, got {confidence}");
            assert!(!auto_queued, "run 6 must not auto-queue");
        }
    }

    #[test]
    fn short_run_is_a_hint_only_when_cued() {
        let (confidence, auto_queued) = egw_quote_score(4, true).expect("should score");
        assert!((0.75..0.88).contains(&confidence));
        assert!(!auto_queued);

        assert_eq!(egw_quote_score(4, false), None);
        assert_eq!(egw_quote_score(5, false), None);
    }

    #[test]
    fn three_or_fewer_shared_words_is_always_dropped() {
        for run in 0..=3 {
            assert_eq!(egw_quote_score(run, true), None, "run {run} with cue");
            assert_eq!(egw_quote_score(run, false), None, "run {run} without cue");
        }
    }
}

#[cfg(test)]
mod cue_tests {
    use super::transcript_has_egw_cue;
    use rhema_bible::EgwBook;

    fn books() -> Vec<EgwBook> {
        vec![
            EgwBook {
                id: 1,
                book_number: 1,
                title: "Patriarchs and Prophets".to_string(),
                abbreviation: "PP".to_string(),
                chapter_count: 2,
            },
            EgwBook {
                id: 2,
                book_number: 2,
                title: "The Desire of Ages".to_string(),
                abbreviation: "DA".to_string(),
                chapter_count: 1,
            },
            EgwBook {
                id: 3,
                book_number: 4,
                title: "Education".to_string(),
                abbreviation: "Ed".to_string(),
                chapter_count: 1,
            },
        ]
    }

    #[test]
    fn author_name_is_a_cue() {
        assert!(transcript_has_egw_cue(
            &books(),
            "sister white says it plainly"
        ));
        assert!(transcript_has_egw_cue(
            &books(),
            "Ellen White wrote about this"
        ));
    }

    #[test]
    fn scoped_illinois_stt_substitution_is_an_author_cue() {
        assert!(transcript_has_egw_cue(
            &books(),
            "I am going to read a statement by Illinois"
        ));
    }

    #[test]
    fn full_title_cue_survives_into_a_truncated_quote_window() {
        use std::sync::atomic::AtomicU64;

        let cue_at = AtomicU64::new(0);
        let full = "A statement in Patriarchs and Prophets introduces a quotation";
        let quote_tail = "the human race yet retained much of its early vigor";

        assert!(super::note_and_check_egw_cue(
            &books(),
            full,
            1_000,
            &cue_at
        ));
        assert!(super::note_and_check_egw_cue(
            &books(),
            quote_tail,
            1_001,
            &cue_at
        ));
        assert_eq!(
            rhema_detection::egw_quote_score(8, true),
            Some((0.92, true))
        );
    }

    #[test]
    fn spirit_of_prophecy_is_a_cue() {
        assert!(transcript_has_egw_cue(
            &books(),
            "the spirit of prophecy speaks to this point"
        ));
    }

    #[test]
    fn book_title_is_a_cue() {
        assert!(transcript_has_egw_cue(
            &books(),
            "in the desire of ages we are told"
        ));
        assert!(transcript_has_egw_cue(
            &books(),
            "patriarchs and prophets describes it"
        ));
    }

    #[test]
    fn ordinary_sermon_prose_is_not_a_cue() {
        assert!(!transcript_has_egw_cue(
            &books(),
            "there is rejoicing in heaven over one sinner who repents"
        ));
    }

    #[test]
    fn ambiguous_single_word_book_title_is_not_a_cue() {
        assert!(!transcript_has_egw_cue(
            &books(),
            "Christian education matters to every family"
        ));
    }

    #[test]
    fn white_alone_is_not_a_cue() {
        // "white robes", "white as snow" are common sermon imagery.
        assert!(!transcript_has_egw_cue(
            &books(),
            "their robes were white as snow"
        ));
    }
}

#[cfg(test)]
mod quote_run_tests {
    use rhema_detection::longest_shared_content_run;

    #[test]
    fn verbatim_phrase_runs_the_full_content_length() {
        // Content words (>=4 letters): history great conflict between christ satan = 6
        let spoken = "the history of the great conflict between christ and satan";
        let paragraph =
            "the history of the great conflict between christ and satan began in heaven";
        assert_eq!(longest_shared_content_run(spoken, paragraph).len, 6);
    }

    #[test]
    fn short_words_do_not_break_a_run() {
        // "of"/"the" are dropped before the run is measured, so they cannot split it.
        let spoken = "history of great conflict";
        let paragraph = "history the great conflict";
        assert_eq!(longest_shared_content_run(spoken, paragraph).len, 3);
    }

    #[test]
    fn out_of_order_shared_words_do_not_count_as_a_run() {
        let spoken = "conflict great history";
        let paragraph = "history great conflict";
        assert_eq!(longest_shared_content_run(spoken, paragraph).len, 1);
    }

    #[test]
    fn disjoint_vocabulary_has_no_run() {
        let spoken = "quantum mechanics lecture";
        let paragraph = "history great conflict";
        assert_eq!(longest_shared_content_run(spoken, paragraph).len, 0);
    }

    #[test]
    fn mid_window_interruption_breaks_the_run_instead_of_splicing() {
        // Scaffolding spoken mid-quote must break the run. Callers must pass the
        // raw transcript: on scaffolding-stripped text the two fragments splice
        // into one contiguous run. Gap tolerance may bridge a single inserted
        // token, but a numeric interjection still cannot reach the spliced length.
        let paragraph = "The shepherd does not remain in the fold waiting for the wandering sheep to return of itself, but he goes forth into the wilderness.";
        let interrupted =
            "the shepherd does not remain in the fold verse 12 waiting for the wandering sheep to return";
        let spliced =
            "the shepherd does not remain in the fold waiting for the wandering sheep to return";

        let interrupted_len = longest_shared_content_run(interrupted, paragraph).len;
        let spliced_len = longest_shared_content_run(spliced, paragraph).len;
        assert!(
            spliced_len >= 8,
            "contiguous quote must reach a strong run, got {spliced_len}"
        );
        assert!(
            interrupted_len < spliced_len,
            "scaffolding mid-quote must not match the full spliced run ({interrupted_len} vs {spliced_len})"
        );
    }

    #[test]
    fn empty_inputs_have_no_run() {
        assert_eq!(
            longest_shared_content_run("", "history great conflict").len,
            0
        );
        assert_eq!(
            longest_shared_content_run("history great conflict", "").len,
            0
        );
    }

    #[test]
    fn opposite_polarity_is_not_quote_evidence() {
        let paragraph = "The shepherd does not remain in the fold waiting for the wandering sheep to return of itself, but he goes forth into the wilderness.";
        let opposite = "The shepherd does remain in the fold waiting for the wandering sheep to return of itself, but he goes forth into the wilderness.";

        assert_eq!(longest_shared_content_run(opposite, paragraph).len, 0);
    }

    #[test]
    fn shared_run_reports_where_in_the_paragraph_the_quote_starts() {
        let paragraph = "Balaam loved the wages of unrighteousness. The sin of covetousness \
                         had made him a timeserver. Many flatter themselves that they can depart \
                         from strict integrity for a time, for the sake of some worldly advantage.";
        let window = "many flatter themselves that they can depart from strict integrity";

        let run = longest_shared_content_run(window, paragraph);

        assert!(run.len >= 6, "expected a strong run, got {}", run.len);
        let tail = &paragraph[run.paragraph_byte_start..];
        assert!(
            tail.starts_with("Many flatter themselves"),
            "anchor should land on the quoted sentence, landed on {:?}",
            &tail[..tail.len().min(40)]
        );
    }

    #[test]
    fn shared_run_anchor_is_zero_when_nothing_matches() {
        let run =
            longest_shared_content_run("completely unrelated speech", "Balaam loved the wages.");
        assert_eq!(run.len, 0);
        assert_eq!(run.paragraph_byte_start, 0);
    }
}
