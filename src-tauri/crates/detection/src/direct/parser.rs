use super::automaton::BookMatch;
use crate::types::VerseRef;

/// Result of attempting to extract a continuation from text for an incomplete reference.
#[derive(Debug, Clone, PartialEq)]
pub enum Continuation {
    /// Found both chapter and verse: "chapter 3 verse 22"
    ChapterAndVerse(i32, i32, Option<i32>),
    /// Found chapter only: "chapter 3" (still waiting for verse)
    ChapterOnly(i32),
    /// Found verse only: "verse 22", bare "22"
    VerseOnly(i32, Option<i32>),
}

/// Parse a Bible reference from text given a book match position.
///
/// Looks ahead from the end of the book match for chapter:verse patterns.
pub fn parse_reference(text: &str, book_match: &BookMatch) -> Option<VerseRef> {
    let after = &text[book_match.end..];
    let after_trimmed = after.trim_start();
    let offset = after.len() - after_trimmed.len();
    let _ = offset; // consumed whitespace

    // Tokenize the text after the book name for easier parsing
    let tokens = tokenize(after_trimmed);

    // Pattern: Book name only (no chapter/verse) → chapter-only, held for refinement
    // e.g., "Genesis" on a partial transcript — wait for chapter:verse to follow
    if tokens.is_empty() {
        return Some(VerseRef {
            book_number: book_match.book_number,
            book_name: book_match.book_name.clone(),
            chapter: 1,
            verse_start: 0, // chapter-only — held for refinement by detector
            verse_end: None,
        });
    }

    // Try pattern: chapter:verse or chapter:verse-end
    if let Some(result) = try_colon_pattern(&tokens, book_match) {
        return Some(result);
    }

    // Try pattern: corrections like "verse 5 sorry chapter 3" or "chapter 3 verse 5 sorry verse 7"
    // Handles mid-speech corrections with keywords like "sorry", "rather", "I mean"
    if let Some(result) = try_correction_pattern(&tokens, book_match) {
        return Some(result);
    }

    // Try pattern: "chapter N verse M" (spoken form)
    if let Some(result) = try_chapter_verse_spoken(&tokens, book_match) {
        return Some(result);
    }

    // Try pattern: "verse N" (implies chapter 1)
    // e.g., "Genesis verse 5" → Genesis 1:5
    if let Some(result) = try_verse_only_pattern(&tokens, book_match) {
        return Some(result);
    }

    // Try pattern: number followed by "verse" keyword then number
    // e.g. "32 verse 1"
    if let Some(result) = try_number_verse_pattern(&tokens, book_match) {
        return Some(result);
    }

    // Try pattern: spoken numbers like "thirty two verse one"
    if let Some(result) = try_spoken_numbers(&tokens, book_match) {
        return Some(result);
    }

    // Pattern: dangling chapter keyword after a book name.
    // e.g., "Daniel chapter" should hold Daniel as incomplete so the next
    // segment "1 verse 5" can complete it as Daniel 1:5.
    if is_dangling_chapter_keyword(&tokens) {
        return Some(VerseRef {
            book_number: book_match.book_number,
            book_name: book_match.book_name.clone(),
            chapter: 1,
            verse_start: 0,
            verse_end: None,
        });
    }

    // Try pattern: two consecutive numbers "3 16" → chapter 3 verse 16
    // This handles "John 3 16" where Deepgram transcribes without colon or keywords
    if let Some(result) = try_two_numbers(&tokens, book_match) {
        return Some(result);
    }

    // Try pattern: just a number (chapter only)
    // e.g., "Genesis 3" → Genesis 3:0 (incomplete, waiting for verse)
    if let Some((chapter, _)) = consume_number_at(&tokens, 0) {
        return Some(VerseRef {
            book_number: book_match.book_number,
            book_name: book_match.book_name.clone(),
            chapter,
            verse_start: 0,
            verse_end: None,
        });
    }

    None
}

/// Return both plausible verses when an STT fragment contains a book followed
/// by three unconnected numbers, such as "Matthew 25 45 41".
///
/// With no range or correction word, the detector cannot safely decide whether
/// the second or third number is the intended verse. Both candidates stay
/// available for operator review below the live threshold.
pub(super) fn parse_ambiguous_three_number_reference(
    text: &str,
    book_match: &BookMatch,
) -> Option<[VerseRef; 2]> {
    let tokens = tokenize(text[book_match.end..].trim_start());
    let [Token::Number(chapter), Token::Number(first_verse), Token::Number(alternative_verse), ..] =
        tokens.as_slice()
    else {
        return None;
    };

    let make_reference = |verse_start| VerseRef {
        book_number: book_match.book_number,
        book_name: book_match.book_name.clone(),
        chapter: *chapter,
        verse_start,
        verse_end: None,
    };

    Some([
        make_reference(*first_verse),
        make_reference(*alternative_verse),
    ])
}

fn is_dangling_chapter_keyword(tokens: &[Token]) -> bool {
    tokens
        .last()
        .is_some_and(|token| matches!(token, Token::Word(word) if is_chapter_keyword(word)))
}

/// A token from the text after the book name.
#[derive(Debug, Clone)]
enum Token {
    Word(String),
    Number(i32),
    Colon,
    Dash,
}

fn is_chapter_keyword(word: &str) -> bool {
    matches!(
        word,
        "chapter" | "hoofstuk" | "capitulo" | "capítulo" | "chapitre"
    )
}

fn is_verse_keyword(word: &str) -> bool {
    matches!(
        word,
        "verse" | "verses" | "vers" | "versiculo" | "versículo" | "verso" | "verset" | "versets"
    )
}

fn is_number_connector(word: &str) -> bool {
    matches!(word, "and" | "en" | "y" | "et" | "e")
}

fn is_range_connector(word: &str) -> bool {
    matches!(
        word,
        "to" | "through" | "hasta" | "a" | "à" | "al" | "ate" | "até"
    ) || is_number_connector(word)
}

fn is_correction_word(word: &str) -> bool {
    matches!(word, "sorry" | "rather" | "meant" | "mean")
}

/// Tokenize text into words, numbers, colons, and dashes.
fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == ':' {
            tokens.push(Token::Colon);
            chars.next();
            continue;
        }
        if ch == '-' || ch == '\u{2013}' || ch == '\u{2014}' {
            tokens.push(Token::Dash);
            chars.next();
            continue;
        }
        if ch.is_ascii_digit() {
            let mut num_str = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    num_str.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(n) = num_str.parse::<i32>() {
                tokens.push(Token::Number(n));
            }
            continue;
        }
        if ch.is_alphabetic() {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphabetic() {
                    word.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Word(word.to_lowercase()));
            continue;
        }
        // Skip other characters
        chars.next();
    }

    tokens
}

/// Try to parse "N:M" or "N : M" or "N:M-E" patterns.
fn try_colon_pattern(tokens: &[Token], book_match: &BookMatch) -> Option<VerseRef> {
    // Look for: Number Colon Number [Dash Number]
    for i in 0..tokens.len() {
        if let Token::Number(chapter) = &tokens[i] {
            if i + 2 < tokens.len() && matches!(&tokens[i + 1], Token::Colon) {
                if let Token::Number(verse) = &tokens[i + 2] {
                    let mut verse_end = None;
                    if i + 4 < tokens.len() && matches!(&tokens[i + 3], Token::Dash) {
                        if let Token::Number(end) = &tokens[i + 4] {
                            verse_end = Some(*end);
                        }
                    }
                    return Some(VerseRef {
                        book_number: book_match.book_number,
                        book_name: book_match.book_name.clone(),
                        chapter: *chapter,
                        verse_start: *verse,
                        verse_end,
                    });
                }
            }
            // Don't break here; keep looking for a colon pattern
        }
    }
    None
}

/// Try to parse correction patterns where user corrects themselves mid-speech.
/// Examples:
/// - "verse 5 sorry chapter 3" → chapter 3, verse 5
/// - "verse 5 chapter 3 rather" → chapter 3, verse 5
/// - "chapter 3 verse 5 sorry verse 7" → chapter 3, verse 7
/// - "chapter 3 verse 5 I mean chapter 4 verse 7" → chapter 4, verse 7
fn try_correction_pattern(tokens: &[Token], book_match: &BookMatch) -> Option<VerseRef> {
    // Find if there's a correction keyword
    let mut correction_idx = None;
    for (i, token) in tokens.iter().enumerate() {
        if let Token::Word(w) = token {
            if is_correction_word(w) {
                correction_idx = Some(i);
                break;
            }
        }
    }

    let correction_idx = correction_idx?;

    // Parse before correction (initial statement)
    let mut initial_chapter = None;
    let mut initial_verse = None;

    // Look for chapter/verse before correction
    for i in 0..correction_idx {
        if let Token::Word(w) = &tokens[i] {
            if is_chapter_keyword(w) {
                if let Some((ch, _)) = consume_number(tokens, i + 1) {
                    initial_chapter = Some(ch);
                }
            } else if is_verse_keyword(w) {
                if let Some((v, _)) = consume_number(tokens, i + 1) {
                    initial_verse = Some(v);
                }
            }
        }
    }

    // Parse after correction (corrected statement)
    let mut corrected_chapter = None;
    let mut corrected_verse = None;

    // Look for chapter/verse after correction
    for i in (correction_idx + 1)..tokens.len() {
        if let Token::Word(w) = &tokens[i] {
            if is_chapter_keyword(w) {
                if let Some((ch, _)) = consume_number(tokens, i + 1) {
                    corrected_chapter = Some(ch);
                }
            } else if is_verse_keyword(w) {
                if let Some((v, _)) = consume_number(tokens, i + 1) {
                    corrected_verse = Some(v);
                }
            }
        }
    }

    // Apply correction logic:
    // - If something is corrected, use the corrected value
    // - Otherwise, keep the initial value
    let final_chapter = corrected_chapter.or(initial_chapter);
    let final_verse = corrected_verse.or(initial_verse);

    // Must have at least a chapter or verse to be valid
    if final_chapter.is_none() && final_verse.is_none() {
        return None;
    }

    Some(VerseRef {
        book_number: book_match.book_number,
        book_name: book_match.book_name.clone(),
        chapter: final_chapter.unwrap_or(1),
        verse_start: final_verse.unwrap_or(1),
        verse_end: None,
    })
}

/// Try to parse "chapter N verse M" pattern.
/// Handles filler words between chapter and verse:
/// "chapter six we will be reading from verse 10 to verse 16" → 6:10-16
/// Also handles: "let's go to chapter 3 verse 2 to verse 3" → 3:2-3
fn try_chapter_verse_spoken(tokens: &[Token], book_match: &BookMatch) -> Option<VerseRef> {
    for i in 0..tokens.len() {
        if let Token::Word(w) = &tokens[i] {
            if is_chapter_keyword(w) {
                // Next token(s) should be a number (digit or spoken)
                if let Some((chapter, next_idx)) = consume_number(tokens, i + 1) {
                    // Scan forward (up to 15 tokens) looking for "verse" keyword.
                    // Extended from 12 to 15 to handle longer phrases like:
                    // "let's go to chapter 3 verse 2 to verse 3"
                    let scan_limit = (next_idx + 15).min(tokens.len());
                    for j in next_idx..scan_limit {
                        if let Some((verse, verse_next)) = consume_colon_damaged_verse(tokens, j) {
                            let verse_end = scan_verse_end(tokens, verse_next);
                            return Some(VerseRef {
                                book_number: book_match.book_number,
                                book_name: book_match.book_name.clone(),
                                chapter,
                                verse_start: verse,
                                verse_end,
                            });
                        }
                        if let Token::Word(vw) = &tokens[j] {
                            if is_verse_keyword(vw) {
                                if let Some((verse, verse_next)) = consume_number(tokens, j + 1) {
                                    let verse_end = scan_verse_end(tokens, verse_next);
                                    return Some(VerseRef {
                                        book_number: book_match.book_number,
                                        book_name: book_match.book_name.clone(),
                                        chapter,
                                        verse_start: verse,
                                        verse_end,
                                    });
                                }
                            }
                        }
                    }
                    // No verse keyword found — chapter-only, held for refinement
                    // e.g., "Genesis chapter 3" → Genesis 3:0 (incomplete)
                    return Some(VerseRef {
                        book_number: book_match.book_number,
                        book_name: book_match.book_name.clone(),
                        chapter,
                        verse_start: 0,
                        verse_end: None,
                    });
                }
                // "chapter" keyword found but no number follows
                // (e.g., "Romans chapter verse 8"). Do NOT return here — keep
                // scanning so a later "chapter N" / "verse N" can still match.
            }
        }
    }
    None
}

/// Scan for a verse range ending after the verse number.
/// Handles: "to verse 16", "through 18", "- 20", "to 16", "and 16"
fn scan_verse_end(tokens: &[Token], start: usize) -> Option<i32> {
    if start >= tokens.len() {
        return None;
    }
    // Check for dash: "10-16"
    if matches!(&tokens[start], Token::Dash) {
        if let Some((end, _)) = consume_number(tokens, start + 1) {
            return Some(end);
        }
    }
    // Check for range connectors or same-chapter "and"
    if let Token::Word(tw) = &tokens[start] {
        if is_range_connector(tw) {
            let next = start + 1;
            if next < tokens.len() {
                // "to verse 16" pattern
                if let Token::Word(vw) = &tokens[next] {
                    if is_verse_keyword(vw) && !is_number_connector(tw) {
                        if let Some((end, _)) = consume_number(tokens, next + 1) {
                            return Some(end);
                        }
                    }
                }
                // "to 16" pattern (no "verse" keyword)
                if let Some((end, _)) = consume_number(tokens, next) {
                    return Some(end);
                }
            }
        }
    }
    None
}

fn consume_colon_damaged_verse(tokens: &[Token], start: usize) -> Option<(i32, usize)> {
    if !matches!(tokens.get(start), Some(Token::Colon)) {
        return None;
    }

    let mut next = start + 1;
    if matches!(tokens.get(next), Some(Token::Word(word)) if word == "es") {
        next += 1;
    }

    consume_number(tokens, next)
}

fn corrected_number_after(tokens: &[Token], start: usize) -> Option<(i32, usize)> {
    let (_, next) = consume_number(tokens, start)?;

    let Some(Token::Word(word)) = tokens.get(next) else {
        return consume_number(tokens, start);
    };
    if !is_correction_word(word) {
        return consume_number(tokens, start);
    }

    consume_number(tokens, next + 1).or_else(|| consume_number(tokens, start))
}

/// Try to parse "verse N" pattern (just verse keyword followed by number, implies chapter 1).
/// e.g., "Genesis verse 5" → Genesis 1:5
fn try_verse_only_pattern(tokens: &[Token], book_match: &BookMatch) -> Option<VerseRef> {
    for i in 0..tokens.len() {
        if let Token::Word(w) = &tokens[i] {
            if is_verse_keyword(w) {
                // Check if this is NOT preceded by a resolved chapter number.
                // Need to check for:
                // 1. Direct number before verse: "3 verse 1" or "Romans 8 and verse 28"
                // 2. Spoken number before verse: "thirty two verse one"
                // 3. "chapter N" where N is present (bare "chapter" alone does not count)
                // A bare "chapter" without a following number does NOT block verse-only
                // parsing (e.g. "Romans chapter verse 8" → Romans 1:8).
                let has_chapter_before = if i > 0 {
                    let mut found = false;
                    for j in 0..i {
                        match &tokens[j] {
                            Token::Number(_) => {
                                found = true;
                                break;
                            }
                            Token::Word(w) if parse_spoken_number(w).is_some() => {
                                found = true;
                                break;
                            }
                            Token::Word(w)
                                if is_chapter_keyword(w)
                                    && consume_number(tokens, j + 1).is_some() =>
                            {
                                found = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                    found
                } else {
                    false
                };

                if !has_chapter_before {
                    if let Some((verse, _)) = consume_number(tokens, i + 1) {
                        return Some(VerseRef {
                            book_number: book_match.book_number,
                            book_name: book_match.book_name.clone(),
                            chapter: 1,
                            verse_start: verse,
                            verse_end: None,
                        });
                    }
                    // "verse" keyword found but no number follows.
                    // Hold the reference so the next segment can provide the verse number.
                    return Some(VerseRef {
                        book_number: book_match.book_number,
                        book_name: book_match.book_name.clone(),
                        chapter: 1,
                        verse_start: 0,
                        verse_end: None,
                    });
                }
            }
        }
    }
    None
}

/// Try to parse "N verse M" pattern (number followed by "verse" keyword).
/// Also scans forward for "verse" with filler words: "6 and we read verse 10"
fn try_number_verse_pattern(tokens: &[Token], book_match: &BookMatch) -> Option<VerseRef> {
    for i in 0..tokens.len() {
        if let Some((chapter, next_idx)) = consume_number_at(tokens, i) {
            // Scan forward for "verse" keyword (allow filler)
            let scan_limit = (next_idx + 10).min(tokens.len());
            for j in next_idx..scan_limit {
                if let Token::Word(w) = &tokens[j] {
                    if is_verse_keyword(w) {
                        if let Some((verse, verse_next)) = consume_number(tokens, j + 1) {
                            let verse_end = scan_verse_end(tokens, verse_next);
                            return Some(VerseRef {
                                book_number: book_match.book_number,
                                book_name: book_match.book_name.clone(),
                                chapter,
                                verse_start: verse,
                                verse_end,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

/// Try to parse spoken number sequences like "thirty two verse one".
fn try_spoken_numbers(tokens: &[Token], book_match: &BookMatch) -> Option<VerseRef> {
    // Try to consume a spoken number at position 0, then look for "verse" keyword
    if let Some((chapter, next_idx)) = consume_number(tokens, 0) {
        if next_idx < tokens.len() {
            if let Token::Word(w) = &tokens[next_idx] {
                if is_verse_keyword(w) {
                    if let Some((verse, verse_next)) = consume_number(tokens, next_idx + 1) {
                        let mut verse_end = None;
                        if verse_next < tokens.len() {
                            if matches!(&tokens[verse_next], Token::Dash) {
                                if let Some((end, _)) = consume_number(tokens, verse_next + 1) {
                                    verse_end = Some(end);
                                }
                            }
                            if let Token::Word(tw) = &tokens[verse_next] {
                                if tw == "through" || tw == "to" {
                                    if let Some((end, _)) = consume_number(tokens, verse_next + 1) {
                                        verse_end = Some(end);
                                    }
                                }
                            }
                        }
                        return Some(VerseRef {
                            book_number: book_match.book_number,
                            book_name: book_match.book_name.clone(),
                            chapter,
                            verse_start: verse,
                            verse_end,
                        });
                    }
                }
            }
        }
    }
    None
}

/// Try to parse two consecutive numbers "N M" as chapter and verse.
/// Handles: "3 16", "119 105", and also spoken: "three sixteen"
fn try_two_numbers(tokens: &[Token], book_match: &BookMatch) -> Option<VerseRef> {
    if let Some((chapter, next_idx)) = consume_number_at(tokens, 0) {
        if chapter > 0 {
            if let Some((verse, verse_next)) = consume_number_at(tokens, next_idx) {
                if verse > 0 {
                    return Some(VerseRef {
                        book_number: book_match.book_number,
                        book_name: book_match.book_name.clone(),
                        chapter,
                        verse_start: verse,
                        verse_end: scan_verse_end(tokens, verse_next),
                    });
                }
            }
        }
    }
    None
}

/// Try to consume a number at the given token position.
/// Returns (number, `next_token_index`) if successful.
/// Handles both digit tokens and spoken number words (including compounds like "thirty two").
fn consume_number(tokens: &[Token], start: usize) -> Option<(i32, usize)> {
    if start >= tokens.len() {
        return None;
    }
    consume_number_at(tokens, start)
}

/// Consume a number starting at position `start`.
/// Handles compound spoken numbers like "thirty two", "one hundred fifty".
fn consume_number_at(tokens: &[Token], start: usize) -> Option<(i32, usize)> {
    if start >= tokens.len() {
        return None;
    }

    // If it's a digit number, return it directly
    if let Token::Number(n) = &tokens[start] {
        return Some((*n, start + 1));
    }

    // Try to parse spoken number words
    if let Token::Word(w) = &tokens[start] {
        if let Some(n) = parse_spoken_number(w) {
            // Check if this is "hundred" — if so, look for more
            if w == "hundred" || w == "honderd" {
                // Shouldn't start with "hundred" alone without context
                return Some((n, start + 1));
            }

            // If n >= 100, it's already compound (e.g., won't happen with single words)
            // Afrikaans inverted form: "drie en twintig" → 23
            if (1..=19).contains(&n) && start + 2 < tokens.len() {
                if let Token::Word(connector) = &tokens[start + 1] {
                    if is_number_connector(connector) {
                        if let Token::Word(tens_w) = &tokens[start + 2] {
                            if let Some(tens) = parse_spoken_number(tens_w) {
                                if (20..=90).contains(&tens) && tens % 10 == 0 {
                                    return Some((tens + n, start + 3));
                                }
                            }
                        }
                    }
                }
            }

            // If n is a tens value (20, 30, ..., 90), look for a ones digit next
            if n >= 20 && n % 10 == 0 && start + 1 < tokens.len() {
                let ones_idx = if matches!(
                    tokens.get(start + 1),
                    Some(Token::Word(next_w)) if is_number_connector(next_w)
                ) {
                    start + 2
                } else {
                    start + 1
                };
                if let Some(Token::Word(next_w)) = tokens.get(ones_idx) {
                    if let Some(ones) = parse_spoken_number(next_w) {
                        if (1..=9).contains(&ones) {
                            let combined = n + ones;
                            // Check for "hundred" after tens+ones
                            if ones_idx + 1 < tokens.len() {
                                if let Token::Word(hw) = &tokens[ones_idx + 1] {
                                    if hw == "hundred" || hw == "honderd" {
                                        // e.g., "one hundred" — but we're at "thirty two hundred"?
                                        // This is unusual, so skip
                                        return Some((combined, ones_idx + 1));
                                    }
                                }
                            }
                            return Some((combined, ones_idx + 1));
                        }
                    }
                }
            }

            // Check if next word is "hundred"
            if (1..=9).contains(&n) && start + 1 < tokens.len() {
                if let Token::Word(next_w) = &tokens[start + 1] {
                    if next_w == "hundred" || next_w == "honderd" {
                        let base = n * 100;
                        // Look for more after "hundred"
                        if start + 2 < tokens.len() {
                            if let Token::Word(w2) = &tokens[start + 2] {
                                // Skip optional "and"
                                let skip = usize::from(is_number_connector(w2));
                                if let Some((rest, rest_idx)) =
                                    consume_number_at(tokens, start + 2 + skip)
                                {
                                    if rest < 100 {
                                        return Some((base + rest, rest_idx));
                                    }
                                }
                            }
                            if let Token::Number(n2) = &tokens[start + 2] {
                                if *n2 < 100 {
                                    return Some((base + n2, start + 3));
                                }
                            }
                        }
                        return Some((base, start + 2));
                    }
                }
            }

            return Some((n, start + 1));
        }
    }

    None
}

/// English hesitation fillers that collide with spoken numbers in other STT
/// language profiles ("um"/"uma" is Portuguese for one, "un"/"una" Spanish and
/// French). In English speech these are noise; left in place they complete
/// pending book-only references into phantom `Book 1:1` detections (the
/// 2026-07-07 "in the book of Jeremiah" incident).
const ENGLISH_FILLER_WORDS: &[&str] = &["um", "uhm", "umm", "uh", "erm", "hmm", "mhm"];

/// Whether a token (already stripped to alphabetic characters) is an English
/// hesitation filler.
pub fn is_english_filler_word(word: &str) -> bool {
    ENGLISH_FILLER_WORDS.contains(&word.to_ascii_lowercase().as_str())
}

/// Remove standalone English hesitation fillers ("um", "uh", ...) from a
/// transcript. Only whole tokens are removed (a token's alphabetic core must
/// equal a filler word), so words like "number" or "drum" are untouched.
pub fn strip_english_filler_words(text: &str) -> String {
    text.split_whitespace()
        .filter(|token| {
            let core: String = token.chars().filter(|c| c.is_alphabetic()).collect();
            !is_english_filler_word(&core)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert a spoken number word to an integer.
/// Supports common spoken number words used by the STT language profiles.
pub fn parse_spoken_number(word: &str) -> Option<i32> {
    match word.to_lowercase().as_str() {
        "zero" | "nul" | "cero" | "zéro" => Some(0),
        "one" | "een" | "uno" | "un" | "una" | "um" | "uma" => Some(1),
        "two" | "twee" | "dos" | "deux" | "dois" | "duas" => Some(2),
        "three" | "drie" | "tres" | "três" | "trois" => Some(3),
        "four" | "vier" | "cuatro" | "quatre" | "quatro" => Some(4),
        "five" | "vyf" | "cinco" | "cinq" => Some(5),
        "six" | "ses" | "seis" => Some(6),
        "seven" | "sewe" | "siete" | "sept" | "sete" => Some(7),
        "eight" | "agt" | "ocho" | "huit" | "oito" => Some(8),
        "nine" | "nege" | "nueve" | "neuf" | "nove" => Some(9),
        "ten" | "tien" | "diez" | "dix" | "dez" => Some(10),
        "eleven" | "elf" | "once" | "onze" => Some(11),
        "twelve" | "twaalf" | "doce" | "douze" => Some(12),
        "thirteen" | "dertien" | "trece" | "treize" | "treze" => Some(13),
        "fourteen" | "veertien" | "catorce" | "quatorze" => Some(14),
        "fifteen" | "vyftien" | "quince" | "quinze" => Some(15),
        "sixteen" | "sestien" | "dieciseis" | "dieciséis" | "seize" | "dezesseis" => Some(16),
        "seventeen" | "sewentien" | "diecisiete" | "dezessete" => Some(17),
        "eighteen" | "agtien" | "dieciocho" | "dezoito" => Some(18),
        "nineteen" | "negentien" | "diecinueve" | "dezenove" => Some(19),
        "twenty" | "twintig" | "veinte" | "vingt" | "vinte" => Some(20),
        "thirty" | "dertig" | "treinta" | "trente" | "trinta" => Some(30),
        "forty" | "veertig" | "cuarenta" | "quarante" | "quarenta" => Some(40),
        "fifty" | "vyftig" | "cincuenta" | "cinquante" | "cinquenta" => Some(50),
        "sixty" | "sestig" | "sesenta" | "soixante" => Some(60),
        "seventy" | "sewentig" | "setenta" | "soixantedix" => Some(70),
        "eighty" | "tagtig" | "ochenta" | "oitenta" => Some(80),
        "ninety" | "negentig" | "noventa" => Some(90),
        "hundred" | "honderd" | "cien" | "ciento" | "cent" | "cem" | "cento" => Some(100),
        _ => None,
    }
}

/// Try to extract a chapter and/or verse continuation from text that follows
/// an incomplete reference (book-only or book+chapter).
///
/// Used when the previous STT segment detected a book name (possibly with
/// chapter) and the current segment may contain the chapter/verse info.
///
/// The `is_book_only` flag indicates the incomplete ref has a defaulted chapter (1).
/// When true, a bare number at start is interpreted as chapter, not verse.
pub fn try_extract_continuation(text: &str, is_book_only: bool) -> Option<Continuation> {
    let lower = text.to_lowercase();
    let trimmed = lower.trim();
    let tokens = tokenize(trimmed);

    if tokens.is_empty() {
        return None;
    }

    // In split speech, Deepgram can produce fragments like "7 verse"
    // before the actual verse number arrives. Do not treat the leading
    // number as a verse in that case; wait for the next segment.
    if !is_book_only && starts_with_dangling_number_verse(&tokens) {
        return None;
    }

    // A pending book-only fragment followed by "N verse M" supplies both
    // missing values. Parse the leading number as the chapter before the
    // generic "verse M" scan can classify the fragment as verse-only.
    if is_book_only {
        if let Some((chapter, next_idx)) = consume_number_at(&tokens, 0) {
            if matches!(tokens.get(next_idx), Some(Token::Word(word)) if is_verse_keyword(word)) {
                if let Some((verse, verse_next)) = corrected_number_after(&tokens, next_idx + 1) {
                    return Some(Continuation::ChapterAndVerse(
                        chapter,
                        verse,
                        scan_verse_end(&tokens, verse_next),
                    ));
                }
            }
        }
    }

    // Pattern 1: "chapter N [... verse M]"
    if let Some(continuation) = find_chapter_keyword_pattern(&tokens) {
        return Some(continuation);
    }

    // Pattern 2: "verse N" / "verses N" anywhere in text
    if let Some(continuation) = find_verse_keyword_pattern(&tokens) {
        return Some(continuation);
    }

    // Pattern 3: Bare number at start
    if let Some((num, _)) = consume_number_at(&tokens, 0) {
        if num > 0 && num <= 176 {
            if is_book_only {
                // After book-only (e.g., "Acts"), bare "3" = chapter
                return Some(Continuation::ChapterOnly(num));
            }
            // After book+chapter (e.g., "Acts 3"), bare "22" = verse
            return Some(Continuation::VerseOnly(num, None));
        }
    }

    None
}

/// Scan for "chapter N [... verse M]" anywhere in the tokens.
fn find_chapter_keyword_pattern(tokens: &[Token]) -> Option<Continuation> {
    for i in 0..tokens.len() {
        if let Token::Word(w) = &tokens[i] {
            if is_chapter_keyword(w) {
                if let Some((chapter, next_idx)) = consume_number(tokens, i + 1) {
                    if chapter <= 0 {
                        continue;
                    }
                    // Scan forward for "verse" keyword (up to 15 tokens)
                    let scan_limit = (next_idx + 15).min(tokens.len());
                    for j in next_idx..scan_limit {
                        if let Some((verse, verse_next)) = consume_colon_damaged_verse(tokens, j) {
                            if verse > 0 && verse <= 176 {
                                return Some(Continuation::ChapterAndVerse(
                                    chapter,
                                    verse,
                                    scan_verse_end(tokens, verse_next),
                                ));
                            }
                        }
                        if let Token::Word(vw) = &tokens[j] {
                            if is_verse_keyword(vw) {
                                if let Some((verse, verse_next)) =
                                    corrected_number_after(tokens, j + 1)
                                {
                                    if verse > 0 && verse <= 176 {
                                        return Some(Continuation::ChapterAndVerse(
                                            chapter,
                                            verse,
                                            scan_verse_end(tokens, verse_next),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    // No verse found — chapter only
                    return Some(Continuation::ChapterOnly(chapter));
                }
            }
        }
    }
    None
}

/// Scan for "verse N" / "verses N" anywhere in the tokens.
fn find_verse_keyword_pattern(tokens: &[Token]) -> Option<Continuation> {
    for i in preferred_verse_indices(tokens) {
        if let Token::Word(w) = &tokens[i] {
            if is_verse_keyword(w) {
                if let Some((verse, verse_next)) = corrected_number_after(tokens, i + 1) {
                    if verse > 0 && verse <= 176 {
                        return Some(Continuation::VerseOnly(
                            verse,
                            scan_verse_end(tokens, verse_next),
                        ));
                    }
                }
            }
        }
    }
    None
}

/// Extract an explicit "chapter N [verse M]" / "verse N" reference from text
/// that contains no book name at all (e.g. "Chapter 2, verse 37, the Bible
/// says…" or "Read verse 2" long after the book was last spoken).
///
/// Unlike `try_extract_continuation`, bare numbers do NOT count: with no
/// pending book reference, the spoken keyword is the only evidence that the
/// numbers are a citation rather than ordinary prose ("this is around 605,
/// 606 BC" must stay silent).
pub fn try_extract_standalone_reference(text: &str) -> Option<Continuation> {
    let lower = text.to_lowercase();
    let tokens = tokenize(lower.trim());
    if tokens.is_empty() {
        return None;
    }

    if let Some(continuation) = find_chapter_keyword_pattern(&tokens) {
        return Some(continuation);
    }
    find_verse_keyword_pattern(&tokens)
}

fn preferred_verse_indices(tokens: &[Token]) -> Vec<usize> {
    let verse_indices: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            matches!(token, Token::Word(word) if is_verse_keyword(word)).then_some(index)
        })
        .collect();

    if verse_indices.len() <= 1 {
        return verse_indices;
    }

    let first = verse_indices[0];
    let last = *verse_indices.last().unwrap_or(&0);
    if tokens[first..last].iter().any(|token| {
        matches!(token, Token::Word(word) if matches!(word.as_str(), "but" | "instead" | "rather"))
    }) {
        return vec![last];
    }

    verse_indices
}

fn starts_with_dangling_number_verse(tokens: &[Token]) -> bool {
    let Some((_number, next_idx)) = consume_number_at(tokens, 0) else {
        return false;
    };

    let Some(Token::Word(word)) = tokens.get(next_idx) else {
        return false;
    };

    (is_verse_keyword(word)) && consume_number(tokens, next_idx + 1).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct::automaton::BookMatch;

    fn make_book_match(name: &str, number: i32, end: usize) -> BookMatch {
        BookMatch {
            book_number: number,
            book_name: name.to_string(),
            start: 0,
            end,
        }
    }

    #[test]
    fn test_colon_reference() {
        let bm = make_book_match("John", 43, 4);
        let text = "John 3:16 says something";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 16);
        assert!(result.verse_end.is_none());
    }

    #[test]
    fn test_colon_range() {
        let bm = make_book_match("Romans", 45, 6);
        let text = "Romans 8:28-30 is powerful";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 8);
        assert_eq!(result.verse_start, 28);
        assert_eq!(result.verse_end, Some(30));
    }

    #[test]
    fn test_spoken_chapter_verse() {
        let bm = make_book_match("Psalms", 19, 5);
        let text = "Psalm thirty two verse one now says";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 32);
        assert_eq!(result.verse_start, 1);
    }

    #[test]
    fn test_chapter_only() {
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis 3 is about the fall";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 0);
    }

    #[test]
    fn test_chapter_verse_keywords() {
        let bm = make_book_match("Isaiah", 23, 6);
        let text = "Isaiah chapter 53 verse 5";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 53);
        assert_eq!(result.verse_start, 5);
    }

    #[test]
    fn test_two_numbers_space_separated() {
        let bm = make_book_match("John", 43, 4);
        let text = "John 3 16 for God so loved";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 16);
    }

    #[test]
    fn test_two_numbers_genesis() {
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis 1 1 in the beginning";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 1);
    }

    #[test]
    fn test_two_numbers_large() {
        let bm = make_book_match("Psalms", 19, 5);
        let text = "Psalm 119 105 thy word is a lamp";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 119);
        assert_eq!(result.verse_start, 105);
    }

    #[test]
    fn test_spoken_number_parser() {
        assert_eq!(parse_spoken_number("one"), Some(1));
        assert_eq!(parse_spoken_number("twenty"), Some(20));
        assert_eq!(parse_spoken_number("thirty"), Some(30));
        assert_eq!(parse_spoken_number("hundred"), Some(100));
        assert_eq!(parse_spoken_number("dog"), None);
    }

    #[test]
    fn test_chapter_verse_with_filler_words() {
        let bm = make_book_match("Ephesians", 49, 10);
        let text = "Ephesians chapter six we will be reading from verse 10 to verse 16";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 6);
        assert_eq!(result.verse_start, 10);
        assert_eq!(result.verse_end, Some(16));
    }

    #[test]
    fn test_chapter_verse_with_and_filler() {
        let bm = make_book_match("John", 43, 4);
        let text = "John chapter three and I want us to look at verse sixteen";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 16);
    }

    #[test]
    fn test_chapter_verse_range_to() {
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis chapter one verse one to verse five";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 1);
        assert_eq!(result.verse_end, Some(5));
    }

    #[test]
    fn transcript_same_chapter_and_range_is_preserved() {
        let bm = make_book_match("John", 43, 4);
        let text = "John 12 verse 32 and 33";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 12);
        assert_eq!(result.verse_start, 32);
        assert_eq!(result.verse_end, Some(33));
    }

    #[test]
    fn transcript_damaged_colon_es_range_is_preserved() {
        let bm = make_book_match("Numbers", 4, 7);
        let text = "Numbers chapter 21:es 4-9";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 21);
        assert_eq!(result.verse_start, 4);
        assert_eq!(result.verse_end, Some(9));
    }

    #[test]
    fn test_number_verse_with_filler() {
        let bm = make_book_match("Romans", 45, 6);
        let text = "Romans 8 and let's look at verse 28";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 8);
        assert_eq!(result.verse_start, 28);
    }

    #[test]
    fn test_lets_go_to_with_range() {
        // Issue: "let's go to Genesis 3 verse 2 to verse 3"
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis let's go to chapter 3 verse 2 to verse 3";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 2);
        assert_eq!(result.verse_end, Some(3));
    }

    #[test]
    fn test_genesis_without_chapter_keyword() {
        // Direct pattern: "Genesis 3 verse 2 to verse 3"
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis 3 verse 2 to verse 3";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 2);
        assert_eq!(result.verse_end, Some(3));
    }

    #[test]
    fn test_book_chapter_verse_with_long_tail() {
        let bm = make_book_match("Exodus", 2, 6);
        let text = "Exodus 20 verse 4 keeping the sabbath holy";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 20);
        assert_eq!(result.verse_start, 4);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_book_only_is_chapter_only() {
        // Pattern: Just book name → chapter-only (held for refinement)
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 0); // chapter-only
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_book_chapter_is_chapter_only() {
        // Pattern: "Genesis chapter 3" → chapter-only (held for refinement)
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis chapter 3";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 0); // chapter-only
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_book_number_incomplete() {
        // Pattern: "Genesis 5" → Genesis 5:0 (incomplete, waiting for verse)
        // This allows continuation like "verse 16" to complete it to Genesis 5:16
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis 5";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 5);
        assert_eq!(result.verse_start, 0);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_book_verse_defaults_to_chapter_1() {
        // Pattern: "Genesis verse 10" → Genesis 1:10
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis verse 10";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 10);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_john_only_is_chapter_only() {
        // Pattern: "John" → chapter-only (held for refinement)
        let bm = make_book_match("John", 43, 4);
        let text = "John";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 0); // chapter-only
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_romans_chapter_8_is_chapter_only() {
        // Pattern: "Romans chapter 8" → chapter-only (held for refinement)
        let bm = make_book_match("Romans", 45, 6);
        let text = "Romans chapter 8";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 8);
        assert_eq!(result.verse_start, 0); // chapter-only
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_psalms_verse_23_defaults_to_chapter_1() {
        // Pattern: "Psalms verse 23" → Psalms 1:23
        let bm = make_book_match("Psalms", 19, 6);
        let text = "Psalms verse 23";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 23);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_correction_verse_then_chapter() {
        // Pattern: "Genesis verse 5 sorry chapter 3" → Genesis 3:5
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis verse 5 sorry chapter 3";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 5);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_correction_verse_then_chapter_rather() {
        // Pattern: "Genesis verse 5 chapter 3 rather" → Genesis 3:5
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis verse 5 chapter 3 rather";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 5);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_correction_full_reference() {
        // Pattern: "Genesis chapter 3 verse 5 sorry chapter 3 verse 7" → Genesis 3:7
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis chapter 3 verse 5 sorry chapter 3 verse 7";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 7);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_correction_chapter_change() {
        // Pattern: "Genesis chapter 3 verse 5 I mean chapter 4 verse 7" → Genesis 4:7
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis chapter 3 verse 5 I mean chapter 4 verse 7";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 4);
        assert_eq!(result.verse_start, 7);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_correction_verse_only() {
        // Pattern: "John chapter 3 verse 16 sorry verse 17" → John 3:17
        let bm = make_book_match("John", 43, 4);
        let text = "John chapter 3 verse 16 sorry verse 17";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 17);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_correction_chapter_only() {
        // Pattern: "Romans chapter 8 sorry chapter 12" → Romans 12:1
        let bm = make_book_match("Romans", 45, 6);
        let text = "Romans chapter 8 sorry chapter 12";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 12);
        assert_eq!(result.verse_start, 1);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_incomplete_chapter_keyword() {
        // Pattern: "Genesis chapter" (incomplete, no number) → hold Genesis
        // so a later segment like "1 verse 5" can complete the reference.
        let bm = make_book_match("Genesis", 1, 7);
        let text = "Genesis chapter";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 0);
    }

    #[test]
    fn test_chapter_keyword_then_verse_number() {
        // Pattern: "Romans chapter verse 8" → Romans 1:8 (not Romans 1:0).
        let bm = make_book_match("Romans", 45, 6);
        let text = "Romans chapter verse 8";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 8);
        assert_eq!(result.verse_end, None);
    }

    #[test]
    fn test_incomplete_verse_keyword() {
        // Pattern: "John verse" (incomplete, no number) holds John 1.
        let bm = make_book_match("John", 43, 4);
        let text = "John verse";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.verse_start, 0);
        assert_eq!(result.verse_end, None);
    }

    // ========== Continuation Extraction Tests ==========

    #[test]
    fn test_continuation_chapter_and_verse() {
        assert_eq!(
            try_extract_continuation("chapter 3 verse 22", false),
            Some(Continuation::ChapterAndVerse(3, 22, None))
        );
        assert_eq!(
            try_extract_continuation("chapter three and I'm reading from verse twenty two", false),
            Some(Continuation::ChapterAndVerse(3, 22, None))
        );
    }

    #[test]
    fn test_continuation_chapter_only() {
        assert_eq!(
            try_extract_continuation("chapter three, and I'm reading from verse", false),
            Some(Continuation::ChapterOnly(3))
        );
        assert_eq!(
            try_extract_continuation("chapter 5", false),
            Some(Continuation::ChapterOnly(5))
        );
    }

    #[test]
    fn test_continuation_verse_anywhere() {
        assert_eq!(
            try_extract_continuation("and I'm reading from verse 22", false),
            Some(Continuation::VerseOnly(22, None))
        );
        assert_eq!(
            try_extract_continuation("verse sixteen", false),
            Some(Continuation::VerseOnly(16, None))
        );
    }

    #[test]
    fn test_continuation_bare_number_book_only() {
        // After book-only detection, bare number = chapter
        assert_eq!(
            try_extract_continuation("3", true),
            Some(Continuation::ChapterOnly(3))
        );
        assert_eq!(
            try_extract_continuation("three", true),
            Some(Continuation::ChapterOnly(3))
        );
    }

    #[test]
    fn test_continuation_bare_number_with_chapter() {
        // After book+chapter detection, bare number = verse
        assert_eq!(
            try_extract_continuation("22", false),
            Some(Continuation::VerseOnly(22, None))
        );
        assert_eq!(
            try_extract_continuation("22. Acts three for Moses", false),
            Some(Continuation::VerseOnly(22, None))
        );
    }

    #[test]
    fn test_continuation_dangling_number_verse_waits_for_number() {
        assert_eq!(try_extract_continuation("7 verse", false), None);
        assert_eq!(try_extract_continuation("seven verse", false), None);
    }

    #[test]
    fn test_continuation_dangling_number_verse_after_book_only_sets_chapter() {
        assert_eq!(
            try_extract_continuation("3 verse", true),
            Some(Continuation::ChapterOnly(3))
        );
    }

    #[test]
    fn test_continuation_number_verse_number_after_book_only_sets_both() {
        assert_eq!(
            try_extract_continuation("4 verse 6", true),
            Some(Continuation::ChapterAndVerse(4, 6, None))
        );
    }

    #[test]
    fn test_continuation_number_verse_number_uses_following_verse() {
        assert_eq!(
            try_extract_continuation("7 verse 9", false),
            Some(Continuation::VerseOnly(9, None))
        );
    }

    #[test]
    fn transcript_continuation_correction_prefers_corrected_verse() {
        assert_eq!(
            try_extract_continuation("then verse 21, sorry 22", false),
            Some(Continuation::VerseOnly(22, None))
        );
    }

    #[test]
    fn transcript_continuation_damaged_colon_es_range_is_preserved() {
        assert_eq!(
            try_extract_continuation("chapter 21:es 4-9", true),
            Some(Continuation::ChapterAndVerse(21, 4, Some(9)))
        );
    }

    #[test]
    fn test_continuation_no_match() {
        assert_eq!(try_extract_continuation("the weather is nice", false), None);
        assert_eq!(
            try_extract_continuation("something unrelated here", false),
            None
        );
    }

    #[test]
    fn afrikaans_johannes_3_vers_16() {
        let bm = make_book_match("Johannes", 43, 8);
        let text = "Johannes 3 vers 16";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 16);
    }

    #[test]
    fn afrikaans_johannes_drie_sestien() {
        let bm = make_book_match("Johannes", 43, 8);
        let text = "Johannes drie sestien";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 3);
        assert_eq!(result.verse_start, 16);
    }

    #[test]
    fn afrikaans_psalm_drie_en_twintig() {
        let bm = make_book_match("Psalms", 19, 5);
        let text = "Psalm drie en twintig";
        let result = parse_reference(text, &bm).unwrap();
        assert_eq!(result.chapter, 23);
        assert_eq!(result.verse_start, 0);
    }

    #[test]
    fn afrikaans_number_words() {
        assert_eq!(parse_spoken_number("drie"), Some(3));
        assert_eq!(parse_spoken_number("sestien"), Some(16));
        assert_eq!(parse_spoken_number("twintig"), Some(20));
    }
}
