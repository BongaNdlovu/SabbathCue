use super::books::BOOKS;

/// A fuzzy match of a Bible book name found in text.
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    pub book_name: String,
    pub book_number: i32,
    pub start: usize,
    pub end: usize,
    pub distance: usize,
}

/// Compute the Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    // Use a single-row DP approach for efficiency
    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row: Vec<usize> = vec![0; b_len + 1];

    for (i, a_ch) in a.chars().enumerate() {
        curr_row[0] = i + 1;
        for (j, b_ch) in b.chars().enumerate() {
            let cost = usize::from(a_ch != b_ch);
            curr_row[j + 1] = (prev_row[j] + cost)
                .min(prev_row[j + 1] + 1)
                .min(curr_row[j] + 1);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[b_len]
}

/// Common English words that fall within fuzzy edit distance of a canonical book
/// name but are never how a speaker names the book (e.g. "like" -> Luke,
/// "number" -> Numbers). The fuzzy fallback exists to recover non-word typos
/// (e.g. "Filipians" -> Philippians); real words that merely collide are prose,
/// not references. Correct and abbreviated forms still match via the Aho-Corasick
/// canonical path, so genuine "Numbers chapter 1" is unaffected. Audited against
/// the 66 canonical names; keep sorted.
const FUZZY_NON_BOOK_WORDS: &[&str] = &[
    "ants", "arts", "dude", "duke", "game", "games", "hope", "house", "jade", "jobs", "join",
    "judge", "judged", "june", "kings", "lake", "like", "marks", "much", "name", "names", "noel",
    "number", "numbers", "romance", "rude", "same", "title", "truth", "woman",
];

/// Determine the maximum allowed edit distance for a book name.
/// Short names (≤4 chars like "Mark", "Ruth", "Joel") only allow 1 edit
/// to prevent false positives like "Mara" → "Mark".
fn max_distance_for(name: &str) -> usize {
    if name.len() <= 4 {
        1 // "Mark", "Ruth", "Joel" — only 1 edit allowed
    } else if name.len() <= 8 {
        2
    } else {
        3 // "Philippians", "Deuteronomy" — allow 3 for very long names
    }
}

/// Find Bible book names in text using fuzzy (Levenshtein) matching.
///
/// Each whitespace-delimited token (and each consecutive pair of tokens for
/// multi-word book names like "1 Corinthians" or "Song of Solomon") is
/// compared against all 66 canonical book names. A match is returned when
/// the edit distance is within the allowed threshold.
pub fn fuzzy_find_books(text: &str) -> Vec<FuzzyMatch> {
    let mut matches: Vec<FuzzyMatch> = Vec::new();
    let text_lower = text.to_lowercase();

    // Collect word spans: (start_byte, end_byte, word_lowercase)
    let words: Vec<(usize, usize, String)> = {
        let mut v = Vec::new();
        let mut i = 0;
        let bytes = text_lower.as_bytes();
        while i < bytes.len() {
            // Skip whitespace
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let word = text_lower[start..i].to_string();
            v.push((start, i, word));
        }
        v
    };

    // Try single-word matches and multi-word windows (up to 4 tokens for
    // names like "Song of Solomon").
    for window_size in 1..=4 {
        for wi in 0..words.len() {
            if wi + window_size > words.len() {
                break;
            }
            let span_start = words[wi].0;
            let span_end = words[wi + window_size - 1].1;
            let candidate = &text_lower[span_start..span_end];

            // Skip windows led by a common word that fuzzy-collides with a book
            // (e.g. "like 5" -> Luke, "number 1" -> Numbers). No canonical book
            // name begins with these, so genuine references are unaffected.
            let lead = words[wi].2.trim_matches(|c: char| !c.is_ascii_alphabetic());
            if FUZZY_NON_BOOK_WORDS.contains(&lead) {
                continue;
            }

            for book in BOOKS {
                let book_lower = book.name.to_lowercase();
                let max_dist = max_distance_for(&book_lower);

                // Quick length filter: if lengths differ by more than max_dist, skip
                let len_diff = if candidate.len() > book_lower.len() {
                    candidate.len() - book_lower.len()
                } else {
                    book_lower.len() - candidate.len()
                };
                if len_diff > max_dist {
                    continue;
                }

                let dist = levenshtein(candidate, &book_lower);
                if dist > 0 && dist <= max_dist {
                    // Avoid overlapping with an already-found match at the same position
                    let dominated = matches
                        .iter()
                        .any(|m| m.start == span_start && m.end == span_end && m.distance <= dist);
                    if !dominated {
                        matches.push(FuzzyMatch {
                            book_name: book.name.to_string(),
                            book_number: book.number,
                            start: span_start,
                            end: span_end,
                            distance: dist,
                        });
                    }
                }
            }
        }
    }

    // De-duplicate: for overlapping spans keep the lowest-distance match.
    matches.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.distance.cmp(&b.distance))
    });
    let mut result: Vec<FuzzyMatch> = Vec::new();
    let mut last_end: usize = 0;
    for m in matches {
        if m.start >= last_end {
            last_end = m.end;
            result.push(m);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn test_levenshtein_substitution() {
        // filipians vs philippians: f→ph (1 sub), i→i ok, l→l ok, i→i ok, p→p ok, i→i ok, a→a ok, n→n ok, s→s ok
        // Actually: "filipians" (9 chars) vs "philippians" (11 chars)
        let d = levenshtein("filipians", "philippians");
        assert!(d <= 3, "distance was {d}");
    }

    #[test]
    fn test_fuzzy_filipians() {
        let matches = fuzzy_find_books("in Filipians chapter 4");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].book_name, "Philippians");
    }

    #[test]
    fn test_fuzzy_revelations() {
        // Common misspelling: "Revelations" (extra 's' but that's just 1 edit from "Revelation")
        let matches = fuzzy_find_books("Revelations 21:1");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].book_name, "Revelation");
    }

    #[test]
    fn test_fuzzy_no_false_positive() {
        // A totally unrelated word should not match any book
        let matches = fuzzy_find_books("programming is fun");
        assert!(matches.is_empty());
    }

    #[test]
    fn common_word_number_does_not_match_numbers_book() {
        // "number" is edit-distance 1 from the book "Numbers" but is an ordinary
        // English word — e.g. "who's the number 1" must not fabricate Numbers 1.
        let matches = fuzzy_find_books("who's the number 1 in the case");
        assert!(matches.iter().all(|m| m.book_name != "Numbers"));
    }

    #[test]
    fn common_words_do_not_fuzzy_match_books() {
        // Prose words within edit distance of a canonical name must not fabricate
        // a reference. The collided book is noted for each.
        for (text, book) in [
            ("i would like to read five", "Luke"),
            ("come join us today", "John"),
            ("the truth will set you free", "Ruth"),
            ("so many kings have ruled", "1 Kings"),
            ("we played a fun game", "James"),
            ("she is a wise woman", "Romans"),
            ("there is much to learn", "Micah"),
            ("welcome to the house of the lord", "Hosea"),
            ("the judge made a ruling", "Judges"),
            ("we met back in june", "Jude"),
        ] {
            let matches = fuzzy_find_books(text);
            assert!(
                matches.iter().all(|m| m.book_name != book),
                "{text:?} must not fuzzy-match {book}"
            );
        }
    }

    #[test]
    fn stoplist_ignores_trailing_punctuation() {
        // STT providers such as Deepgram can attach punctuation to tokens.
        let matches = fuzzy_find_books("pick a number, then wait");
        assert!(matches.iter().all(|m| m.book_name != "Numbers"));
    }

    #[test]
    fn genuine_typos_still_recover_book_names() {
        // The stoplist must not neuter non-word typo recovery.
        assert!(fuzzy_find_books("in Filipians chapter 4")
            .iter()
            .any(|m| m.book_name == "Philippians"));
        assert!(fuzzy_find_books("read Hebrws 11")
            .iter()
            .any(|m| m.book_name == "Hebrews"));
        assert!(fuzzy_find_books("Revelations 21 verse 1")
            .iter()
            .any(|m| m.book_name == "Revelation"));
    }

    #[test]
    fn singular_plural_book_variants_still_recover_book_names() {
        // These are plausible spoken/STT variants of plural book names, not
        // generic prose stopwords.
        assert!(fuzzy_find_books("read Hebrew 11")
            .iter()
            .any(|m| m.book_name == "Hebrews"));
        assert!(fuzzy_find_books("turn to Roman 8")
            .iter()
            .any(|m| m.book_name == "Romans"));
    }

    #[test]
    fn test_fuzzy_hebrews_misspelled() {
        let matches = fuzzy_find_books("in Hebrws chapter 11");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].book_name, "Hebrews");
    }
}
