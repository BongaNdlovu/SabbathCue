use std::collections::VecDeque;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::automaton::{BookMatch, BookMatcher};
use super::context::ReferenceContext;
use super::fuzzy;
use super::parser;
use crate::types::{Detection, DetectionSource, VerseRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttVoiceCommand {
    Start,
    Stop,
}

/// Translation command patterns — maps spoken phrases to translation abbreviations.
const TRANSLATION_COMMANDS: &[(&str, &str)] = &[
    // NIV
    ("give me niv", "NIV"),
    ("read in niv", "NIV"),
    ("switch to niv", "NIV"),
    ("in the niv", "NIV"),
    ("can i have it in niv", "NIV"),
    ("can i have that in niv", "NIV"),
    ("show me niv", "NIV"),
    ("new international version", "NIV"),
    ("in new international", "NIV"),
    // ESV
    ("give me esv", "ESV"),
    ("read in esv", "ESV"),
    ("switch to esv", "ESV"),
    ("in the esv", "ESV"),
    ("can i have it in esv", "ESV"),
    ("can i have that in esv", "ESV"),
    ("show me esv", "ESV"),
    ("english standard version", "ESV"),
    ("in english standard", "ESV"),
    // NASB
    ("give me nasb", "NASB"),
    ("read in nasb", "NASB"),
    ("switch to nasb", "NASB"),
    ("in the nasb", "NASB"),
    ("can i have it in nasb", "NASB"),
    ("can i have that in nasb", "NASB"),
    ("show me nasb", "NASB"),
    ("new american standard", "NASB"),
    ("in new american", "NASB"),
    // NKJV
    ("give me nkjv", "NKJV"),
    ("read in nkjv", "NKJV"),
    ("switch to nkjv", "NKJV"),
    ("in the nkjv", "NKJV"),
    ("can i have it in nkjv", "NKJV"),
    ("can i have that in nkjv", "NKJV"),
    ("show me nkjv", "NKJV"),
    ("new king james", "NKJV"),
    ("in new king james", "NKJV"),
    // NLT
    ("give me nlt", "NLT"),
    ("read in nlt", "NLT"),
    ("switch to nlt", "NLT"),
    ("in the nlt", "NLT"),
    ("can i have it in nlt", "NLT"),
    ("can i have that in nlt", "NLT"),
    ("show me nlt", "NLT"),
    ("new living translation", "NLT"),
    ("in new living", "NLT"),
    // KJV
    ("give me kjv", "KJV"),
    ("read in kjv", "KJV"),
    ("switch to kjv", "KJV"),
    ("in the kjv", "KJV"),
    ("can i have it in kjv", "KJV"),
    ("can i have that in kjv", "KJV"),
    ("show me kjv", "KJV"),
    ("king james version", "KJV"),
    ("in king james", "KJV"),
    // AMP
    ("give me amp", "AMP"),
    ("give me amplified", "AMP"),
    ("read in amplified", "AMP"),
    ("switch to amplified", "AMP"),
    ("in the amplified", "AMP"),
    ("can i have it in amplified", "AMP"),
    ("can i have that in amplified", "AMP"),
    ("can i have it in amp", "AMP"),
    ("can i have that in amp", "AMP"),
    ("show me amplified", "AMP"),
    ("show me amp", "AMP"),
    ("amplified bible", "AMP"),
    ("amplified version", "AMP"),
    ("amplified translation", "AMP"),
    ("in amplified version", "AMP"),
    ("in amplified translation", "AMP"),
    // English — maps to the bundled public-domain KJV. Listed after ESV's
    // "english standard version" so that specific phrase still wins.
    ("give me english", "KJV"),
    ("read in english", "KJV"),
    ("switch to english", "KJV"),
    ("back to english", "KJV"),
    ("in english", "KJV"),
    ("can i have it in english", "KJV"),
    ("can i have that in english", "KJV"),
    ("show me english", "KJV"),
    // Note: bare "english version"/"english translation" are intentionally
    // omitted — they would shadow "contemporary english version" (CEV) and
    // "new english translation" (NET). The bare word "english" is handled below.
    // SpaRV (Spanish - Reina-Valera 1909)
    ("give me reina valera", "SpaRV"),
    ("read in reina valera", "SpaRV"),
    ("switch to reina valera", "SpaRV"),
    ("in reina valera", "SpaRV"),
    ("can i have it in reina valera", "SpaRV"),
    ("can i have that in reina valera", "SpaRV"),
    ("show me reina valera", "SpaRV"),
    ("give me spanish", "SpaRV"),
    ("read in spanish", "SpaRV"),
    ("switch to spanish", "SpaRV"),
    ("in spanish", "SpaRV"),
    ("can i have it in spanish", "SpaRV"),
    ("can i have that in spanish", "SpaRV"),
    ("spanish version", "SpaRV"),
    ("spanish translation", "SpaRV"),
    // FreJND (French - J.N. Darby)
    ("give me french", "FreJND"),
    ("read in french", "FreJND"),
    ("switch to french", "FreJND"),
    ("in french", "FreJND"),
    ("can i have it in french", "FreJND"),
    ("can i have that in french", "FreJND"),
    ("show me french", "FreJND"),
    ("french version", "FreJND"),
    ("french translation", "FreJND"),
    ("darby french", "FreJND"),
    // PorBLivre (Portuguese - Biblia Livre)
    ("give me portuguese", "PorBLivre"),
    ("read in portuguese", "PorBLivre"),
    ("switch to portuguese", "PorBLivre"),
    ("in portuguese", "PorBLivre"),
    ("can i have it in portuguese", "PorBLivre"),
    ("can i have that in portuguese", "PorBLivre"),
    ("show me portuguese", "PorBLivre"),
    ("portuguese version", "PorBLivre"),
    ("portuguese translation", "PorBLivre"),
    ("biblia livre", "PorBLivre"),
    // Afr1953 (Afrikaans 1933/1953)
    ("give me afrikaans", "Afr1953"),
    ("read in afrikaans", "Afr1953"),
    ("switch to afrikaans", "Afr1953"),
    ("in afrikaans", "Afr1953"),
    ("can i have it in afrikaans", "Afr1953"),
    ("can i have that in afrikaans", "Afr1953"),
    ("show me afrikaans", "Afr1953"),
    ("afrikaans bybel", "Afr1953"),
    ("afrikaans bible", "Afr1953"),
    ("afrikaans vertaling", "Afr1953"),
    ("1933 bybel", "Afr1953"),
    ("1953 bybel", "Afr1953"),
    // MSG (The Message)
    ("give me message", "MSG"),
    ("give me the message", "MSG"),
    ("read in message", "MSG"),
    ("read in the message", "MSG"),
    ("switch to message", "MSG"),
    ("switch to the message", "MSG"),
    ("in the message", "MSG"),
    ("can i have it in message", "MSG"),
    ("can i have that in message", "MSG"),
    ("can i have it in the message", "MSG"),
    ("can i have that in the message", "MSG"),
    ("show me message", "MSG"),
    ("show me the message", "MSG"),
    ("message version", "MSG"),
    ("message translation", "MSG"),
    ("message bible", "MSG"),
    // HCSB (Holman Christian Standard Bible) - must come before CSB to avoid substring matches
    ("give me hcsb", "HCSB"),
    ("read in hcsb", "HCSB"),
    ("switch to hcsb", "HCSB"),
    ("in the hcsb", "HCSB"),
    ("can i have it in hcsb", "HCSB"),
    ("can i have that in hcsb", "HCSB"),
    ("show me hcsb", "HCSB"),
    ("holman christian standard", "HCSB"),
    ("holman christian", "HCSB"),
    ("in holman christian", "HCSB"),
    // CSB (Christian Standard Bible)
    ("give me csb", "CSB"),
    ("read in csb", "CSB"),
    ("switch to csb", "CSB"),
    ("in the csb", "CSB"),
    ("can i have it in csb", "CSB"),
    ("can i have that in csb", "CSB"),
    ("show me csb", "CSB"),
    ("christian standard bible", "CSB"),
    ("christian standard", "CSB"),
    ("in christian standard", "CSB"),
    // NRSV (New Revised Standard Version) - must come before RSV to avoid substring matches
    ("give me nrsv", "NRSV"),
    ("read in nrsv", "NRSV"),
    ("switch to nrsv", "NRSV"),
    ("in the nrsv", "NRSV"),
    ("can i have it in nrsv", "NRSV"),
    ("can i have that in nrsv", "NRSV"),
    ("show me nrsv", "NRSV"),
    ("new revised standard version", "NRSV"),
    ("new revised standard", "NRSV"),
    ("in new revised standard", "NRSV"),
    // RSV (Revised Standard Version)
    ("give me rsv", "RSV"),
    ("read in rsv", "RSV"),
    ("switch to rsv", "RSV"),
    ("in the rsv", "RSV"),
    ("can i have it in rsv", "RSV"),
    ("can i have that in rsv", "RSV"),
    ("show me rsv", "RSV"),
    ("revised standard version", "RSV"),
    ("revised standard", "RSV"),
    ("in revised standard", "RSV"),
    // NET (New English Translation)
    ("give me net", "NET"),
    ("read in net", "NET"),
    ("switch to net", "NET"),
    ("in the net", "NET"),
    ("can i have it in net", "NET"),
    ("can i have that in net", "NET"),
    ("show me net", "NET"),
    ("new english translation", "NET"),
    ("in new english", "NET"),
    // CEV (Contemporary English Version)
    ("give me cev", "CEV"),
    ("read in cev", "CEV"),
    ("switch to cev", "CEV"),
    ("in the cev", "CEV"),
    ("can i have it in cev", "CEV"),
    ("can i have that in cev", "CEV"),
    ("show me cev", "CEV"),
    ("contemporary english", "CEV"),
    ("contemporary english version", "CEV"),
    ("in contemporary english", "CEV"),
    // GNT/GNB (Good News Translation / Good News Bible)
    ("give me gnt", "GNT"),
    ("give me gnb", "GNT"),
    ("read in gnt", "GNT"),
    ("read in gnb", "GNT"),
    ("switch to gnt", "GNT"),
    ("switch to gnb", "GNT"),
    ("in the gnt", "GNT"),
    ("in the gnb", "GNT"),
    ("can i have it in gnt", "GNT"),
    ("can i have that in gnt", "GNT"),
    ("show me gnt", "GNT"),
    ("show me gnb", "GNT"),
    ("good news", "GNT"),
    ("good news translation", "GNT"),
    ("good news bible", "GNT"),
    ("in good news", "GNT"),
];

const STT_STOP_COMMANDS: &[&str] = &[
    "stop transcribing",
    "stop transcription",
    "stop the transcription",
    "stop listening",
    "stop recording",
];

const STT_START_COMMANDS: &[&str] = &[
    "start transcribing",
    "start transcription",
    "start the transcription",
    "start listening",
    "start recording",
];

/// Maximum chapter count per book (`book_number` 1-66).
/// Used to reject impossible references like "Mark 30:1" (Mark has 16 chapters).
const MAX_CHAPTERS: [i32; 67] = [
    0,   // index 0 unused
    50,  // 1  Genesis
    40,  // 2  Exodus
    27,  // 3  Leviticus
    36,  // 4  Numbers
    34,  // 5  Deuteronomy
    24,  // 6  Joshua
    21,  // 7  Judges
    4,   // 8  Ruth
    31,  // 9  1 Samuel
    24,  // 10 2 Samuel
    22,  // 11 1 Kings
    25,  // 12 2 Kings
    29,  // 13 1 Chronicles
    36,  // 14 2 Chronicles
    10,  // 15 Ezra
    13,  // 16 Nehemiah
    10,  // 17 Esther
    42,  // 18 Job
    150, // 19 Psalms
    31,  // 20 Proverbs
    12,  // 21 Ecclesiastes
    8,   // 22 Song of Solomon
    66,  // 23 Isaiah
    52,  // 24 Jeremiah
    5,   // 25 Lamentations
    48,  // 26 Ezekiel
    12,  // 27 Daniel
    14,  // 28 Hosea
    3,   // 29 Joel
    9,   // 30 Amos
    1,   // 31 Obadiah
    4,   // 32 Jonah
    7,   // 33 Micah
    3,   // 34 Nahum
    3,   // 35 Habakkuk
    3,   // 36 Zephaniah
    2,   // 37 Haggai
    14,  // 38 Zechariah
    4,   // 39 Malachi
    28,  // 40 Matthew
    16,  // 41 Mark
    24,  // 42 Luke
    21,  // 43 John
    28,  // 44 Acts
    16,  // 45 Romans
    16,  // 46 1 Corinthians
    13,  // 47 2 Corinthians
    6,   // 48 Galatians
    6,   // 49 Ephesians
    4,   // 50 Philippians
    4,   // 51 Colossians
    5,   // 52 1 Thessalonians
    3,   // 53 2 Thessalonians
    6,   // 54 1 Timothy
    4,   // 55 2 Timothy
    3,   // 56 Titus
    1,   // 57 Philemon
    13,  // 58 Hebrews
    5,   // 59 James
    5,   // 60 1 Peter
    3,   // 61 2 Peter
    5,   // 62 1 John
    1,   // 63 2 John
    1,   // 64 3 John
    1,   // 65 Jude
    22,  // 66 Revelation
];

/// Check if a book/chapter combination is valid.
fn is_valid_reference(book_number: i32, chapter: i32) -> bool {
    if !(1..=66).contains(&book_number) {
        return false;
    }
    #[expect(clippy::cast_sign_loss, reason = "book_number validated to be 1..=66")]
    let max_ch = MAX_CHAPTERS[book_number as usize];
    chapter >= 1 && chapter <= max_ch
}

/// True when the text immediately following a book match begins with
/// reference-like content: a colon, a chapter/verse number (digit or spoken),
/// or the words "chapter"/"verse". Used to gate abbreviation/alias and fuzzy
/// matches that collide with everyday words ("act", "mic", "pro", "psalm"), so
/// sermon prose does not fabricate references.
fn reference_context_follows(text: &str, after: usize) -> bool {
    let rest = text[after..].trim_start();
    if rest.starts_with(':') {
        return true;
    }
    let Some(first) = rest
        .split(|c: char| c.is_whitespace() || c == ':' || c == '-')
        .find(|token| !token.is_empty())
    else {
        return false;
    };
    let token = first
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_ascii_lowercase();
    if token.is_empty() {
        return false;
    }
    token.chars().all(|c| c.is_ascii_digit())
        || token == "chapter"
        || token == "verse"
        || token == "verses"
        || parser::parse_spoken_number(&token).is_some()
}

/// Filler phrases commonly found in sermon transcripts that confuse detection.
/// These are stripped (case-insensitively) before the text reaches the automaton.
const FILLER_PHRASES: &[&str] = &[
    "please open your bibles to",
    "let us turn to",
    "let's turn to",
    "go to the book of",
    "the book of",
    "book of",
    "if you turn to",
    "if you'll turn to",
    "we will be reading from",
    "we read in",
    "the bible says in",
    "it says in",
    "as we see in",
    "as written in",
    "let's go to",
    "turn in your bibles to",
    "turn in your bible to",
];

/// Common local-STT Bible-reference substitutions. Kept narrow so ordinary
/// sermon language does not turn into false positive Bible references.
const REFERENCE_CORRECTIONS: &[(&str, &str)] = &[
    ("one samuel", "1 Samuel"),
    ("two samuel", "2 Samuel"),
    ("one kings", "1 Kings"),
    ("two kings", "2 Kings"),
    ("one chronicles", "1 Chronicles"),
    ("two chronicles", "2 Chronicles"),
    ("first corinthian", "1 Corinthians"),
    ("one corinthian", "1 Corinthians"),
    ("one corinthians", "1 Corinthians"),
    ("second corinthian", "2 Corinthians"),
    ("two corinthian", "2 Corinthians"),
    ("two corinthians", "2 Corinthians"),
    ("first thessalonian", "1 Thessalonians"),
    ("one thessalonian", "1 Thessalonians"),
    ("one thessalonians", "1 Thessalonians"),
    ("second thessalonian", "2 Thessalonians"),
    ("two thessalonian", "2 Thessalonians"),
    ("two thessalonians", "2 Thessalonians"),
    ("one timothy", "1 Timothy"),
    ("two timothy", "2 Timothy"),
    ("one peter", "1 Peter"),
    ("two peter", "2 Peter"),
    ("one john", "1 John"),
    ("two john", "2 John"),
    ("three john", "3 John"),
    ("revelations", "Revelation"),
    ("song chapter", "Psalms chapter"),
    ("songs chapter", "Psalms chapter"),
    ("song verse", "Psalms verse"),
    ("songs verse", "Psalms verse"),
];

/// Strip common sermon filler phrases from transcript text so they do not
/// confuse the Aho-Corasick automaton or the parser.
///
/// Performs simple case-insensitive removal of each phrase in [`FILLER_PHRASES`],
/// plus a special pattern for "look at" when followed by what looks like a book name
/// (starts with an uppercase letter).
fn clean_transcript(text: &str, strip_english_fillers: bool) -> String {
    let mut result = if strip_english_fillers {
        // English hesitation fillers ("um", "uh") collide with spoken numbers
        // in other language profiles (pt "um" = 1) and can complete pending
        // book-only references into phantom Book 1:1 detections.
        parser::strip_english_filler_words(text)
    } else {
        text.to_string()
    };

    for (from, to) in REFERENCE_CORRECTIONS {
        result = replace_case_insensitive_phrase(&result, from, to);
    }

    // Remove fixed filler phrases (case-insensitive)
    for phrase in FILLER_PHRASES {
        result = replace_case_insensitive_phrase(&result, phrase, "");
    }

    result = strip_case_insensitive_phrase_before_uppercase_word(&result, "look at");

    // Collapse multiple spaces and trim
    let mut prev_space = false;
    let collapsed: String = result
        .chars()
        .filter(|&c| {
            if c == ' ' {
                if prev_space {
                    return false;
                }
                prev_space = true;
            } else {
                prev_space = false;
            }
            true
        })
        .collect();

    collapsed.trim().to_string()
}

fn is_hymn_or_song_number_command(text: &str) -> bool {
    let normalized = text
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect::<String>();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();

    for (index, token) in tokens.iter().enumerate() {
        if !matches!(
            *token,
            "hymn"
                | "hymns"
                | "hymnal"
                | "hymnals"
                | "song"
                | "songs"
                | "lied"
                | "liedere"
                | "liedboek"
                | "liedboeke"
        ) {
            continue;
        }

        let mut number_start = index + 1;
        if matches!(tokens.get(number_start), Some(&("number" | "nommer"))) {
            number_start += 1;
        }

        let Some(number_end) = hymn_command_number_end(&tokens, number_start) else {
            continue;
        };

        if tokens[number_end..]
            .iter()
            .any(|token| matches!(*token, "chapter" | "verse" | "verses"))
        {
            continue;
        }

        return true;
    }

    false
}

/// True when an utterance is a voice command the dedicated command/reading
/// paths already handle: a translation switch, a hymn/song number, or reading
/// navigation. Live semantic paraphrase search skips these so spoken commands
/// don't flood the detections panel with keyword noise.
pub fn is_voice_command_utterance(text: &str) -> bool {
    if is_hymn_or_song_number_command(text) {
        return true;
    }
    let lower = text.to_lowercase();
    if TRANSLATION_COMMANDS
        .iter()
        .any(|(phrase, _)| lower.contains(phrase))
    {
        return true;
    }
    is_navigation_command(&lower)
}

/// Reading-navigation phrases: directional moves ("next"/"previous"/"go back"
/// a verse or chapter) and moves within "the same/this chapter".
fn is_navigation_command(lower: &str) -> bool {
    let directional =
        lower.contains("next") || lower.contains("previous") || lower.contains("go back");
    if directional && (lower.contains("verse") || lower.contains("chapter")) {
        return true;
    }
    (lower.contains("same chapter") || lower.contains("this chapter")) && lower.contains("verse")
}

fn hymn_command_number_end(tokens: &[&str], start: usize) -> Option<usize> {
    let first = tokens.get(start)?;
    if first.chars().all(|ch| ch.is_ascii_digit()) {
        return first
            .parse::<i32>()
            .ok()
            .filter(|value| *value > 0)
            .map(|_| start + 1);
    }

    let mut index = start;
    let mut saw_number = false;
    while let Some(token) = tokens.get(index) {
        if matches!(*token, "and" | "en") && saw_number {
            index += 1;
            continue;
        }

        if parser::parse_spoken_number(token).is_none() {
            break;
        }

        saw_number = true;
        index += 1;
    }

    saw_number.then_some(index)
}

fn replace_case_insensitive_phrase(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(relative_pos) = lower[cursor..].find(from) {
        let pos = cursor + relative_pos;
        let end = pos + from.len();

        if is_word_boundary(text, pos) && is_word_boundary(text, end) {
            result.push_str(&text[cursor..pos]);
            result.push_str(to);
            cursor = end;
        } else if end >= text.len() {
            result.push_str(&text[cursor..]);
            cursor = text.len();
            break;
        } else {
            let next = text[pos..]
                .char_indices()
                .nth(1)
                .map_or(text.len(), |(offset, _)| pos + offset);
            result.push_str(&text[cursor..next]);
            cursor = next;
        }
    }

    if cursor < text.len() {
        result.push_str(&text[cursor..]);
    }

    result
}

fn strip_case_insensitive_phrase_before_uppercase_word(text: &str, phrase: &str) -> String {
    let phrase_lower = phrase.to_ascii_lowercase();
    let mut result = text.to_string();

    while let Some(pos) = result.to_ascii_lowercase().find(&phrase_lower) {
        let after_pos = pos + phrase.len();
        let after = &result[after_pos..];
        let trimmed = after.trim_start();
        let Some(ch) = trimmed.chars().next() else {
            break;
        };

        if !ch.is_ascii_uppercase() {
            break;
        }

        result = format!("{}{}", &result[..pos], &result[after_pos..]);
    }

    result
}

fn is_word_boundary(text: &str, idx: usize) -> bool {
    if idx == 0 || idx >= text.len() {
        return true;
    }
    let before = text[..idx].chars().next_back();
    let after = text[idx..].chars().next();
    !matches!((before, after), (Some(a), Some(b)) if a.is_alphanumeric() && b.is_alphanumeric())
}

/// How long to wait for an incomplete reference to be completed (15 seconds).
/// Preachers often pause between book name and chapter/verse.
const INCOMPLETE_REF_TIMEOUT_MS: u128 = 15_000;

/// Confidence for an explicitly spoken chapter-only citation ("Daniel chapter
/// one"): a direct citation that should go live, sitting just above the 0.90
/// auto-fire threshold but below full chapter:verse references.
const CHAPTER_ONLY_CONFIDENCE: f64 = 0.92;

/// Confidence for a "verse N" / "chapter N verse M" whose book was filled in
/// from mutable reference context rather than repeated by the speaker. Keep it
/// visible to the operator, but below the auto-live threshold: a later citation
/// can make the most recent book differ from the sermon's active passage.
const CONTEXT_RESOLVED_CONFIDENCE: f64 = 0.85;

/// An incomplete reference waiting for verse completion.
#[derive(Debug, Clone)]
struct IncompleteRef {
    verse_ref: VerseRef,
    timestamp: Instant,
    /// When true, the chapter field is a default (1), not explicitly spoken.
    /// Bare numbers should be interpreted as chapter, not verse.
    chapter_is_default: bool,
}

/// Main orchestrator for direct Bible reference detection.
///
/// Uses Aho-Corasick automaton for fast book name matching, then parses
/// chapter:verse patterns (both numeric and spoken forms) and maintains
/// context for resolving partial references.
///
/// Supports incomplete reference handling: when a chapter-only reference
/// is detected (e.g., "Genesis 3"), it's held for up to 5 seconds waiting
/// for a verse completion (e.g., "verse 16"). If no completion arrives,
/// the chapter-only reference is emitted defaulting to verse 1.
/// Phrases that indicate the user wants to go back to a previous verse.
const PREVIOUS_VERSE_PHRASES: &[&str] = &[
    "previous verse",
    "last verse",
    "that verse again",
    "go back to that verse",
    "back to that verse",
    "the same verse",
    "repeat that verse",
];
const SAVE_CONTEXT_PHRASES: &[&str] = &["keep your place", "hold your place"];
const RETURN_CONTEXT_PHRASES: &[&str] = &["back in", "back to", "coming back", "go back"];

fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| text.contains(phrase))
}

fn same_book_chapter(a: &VerseRef, b: &VerseRef) -> bool {
    a.book_number == b.book_number && a.chapter == b.chapter
}

pub struct DirectDetector {
    matcher: BookMatcher,
    /// Active STT language code (e.g. "en", "pt"); controls English filler
    /// stripping, which must stay off for languages where those tokens are
    /// real number words.
    stt_language: String,
    context: ReferenceContext,
    /// Pending incomplete reference waiting for verse completion.
    incomplete: Option<IncompleteRef>,
    /// Recently detected verses for "previous verse" navigation (most recent first).
    recent_detections: VecDeque<VerseRef>,
    saved_contexts: VecDeque<VerseRef>,
}

impl DirectDetector {
    pub fn new() -> Self {
        Self::for_stt_language("en")
    }

    pub fn for_stt_language(language: &str) -> Self {
        DirectDetector {
            matcher: BookMatcher::for_stt_language(language),
            stt_language: language.to_string(),
            context: ReferenceContext::new(),
            incomplete: None,
            recent_detections: VecDeque::with_capacity(5),
            saved_contexts: VecDeque::with_capacity(3),
        }
    }

    /// Rebuild the book matcher when STT language changes.
    pub fn set_stt_language(&mut self, language: &str) {
        self.matcher = BookMatcher::for_stt_language(language);
        self.stt_language = language.to_string();
        self.incomplete = None;
        self.saved_contexts.clear();
    }

    /// Recent detections for context tracking.
    pub fn recent_detections(&self) -> &VecDeque<VerseRef> {
        &self.recent_detections
    }

    /// Check if the transcript contains a translation switching command.
    /// Returns the translation abbreviation if found (e.g., "NIV", "ESV").
    ///
    /// Matches both full phrases ("new international version") and bare
    /// abbreviations ("NIV", "AMP") as standalone words.
    pub fn detect_translation_command(&self, text: &str) -> Option<String> {
        let lower = text.to_lowercase();

        // First check full phrases (higher confidence)
        for (pattern, abbrev) in TRANSLATION_COMMANDS {
            if lower.contains(pattern) {
                log::info!("[DET-DIRECT] Translation command detected: {abbrev}");
                return Some(abbrev.to_string());
            }
        }

        // Then check bare abbreviations as standalone words
        // Split into words and check each against known abbreviations
        let words: Vec<&str> = lower
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .collect();

        for word in &words {
            let matched = match *word {
                "niv" => Some("NIV"),
                "esv" => Some("ESV"),
                "nasb" => Some("NASB"),
                "nkjv" | "njkv" => Some("NKJV"),
                "nlt" => Some("NLT"),
                "kjv" | "english" => Some("KJV"),
                "amp" | "amplified" => Some("AMP"),
                "sparv" | "spanish" => Some("SpaRV"),
                "frejnd" | "french" => Some("FreJND"),
                "porblivre" | "portuguese" => Some("PorBLivre"),
                "msg" => Some("MSG"),
                "csb" => Some("CSB"),
                "hcsb" => Some("HCSB"),
                "rsv" => Some("RSV"),
                "nrsv" => Some("NRSV"),
                "net" => Some("NET"),
                "cev" => Some("CEV"),
                "gnt" | "gnb" => Some("GNT"),
                _ => None,
            };
            if let Some(abbrev) = matched {
                log::info!("[DET-DIRECT] Translation abbreviation detected: {abbrev}");
                return Some(abbrev.to_string());
            }
        }

        None
    }

    /// Check for STT lifecycle voice commands.
    ///
    /// Stop can be acted on by the running transcription pipeline. Start is
    /// only detectable while a pipeline is already listening; waking from a
    /// fully stopped state requires a separate always-listening command path.
    pub fn detect_stt_voice_command(&self, text: &str) -> Option<SttVoiceCommand> {
        let lower = text.to_lowercase();

        if STT_STOP_COMMANDS
            .iter()
            .any(|pattern| lower.contains(pattern))
        {
            log::info!("[DET-DIRECT] STT stop voice command detected");
            return Some(SttVoiceCommand::Stop);
        }

        if STT_START_COMMANDS
            .iter()
            .any(|pattern| lower.contains(pattern))
        {
            log::info!("[DET-DIRECT] STT start voice command detected");
            return Some(SttVoiceCommand::Start);
        }

        None
    }

    /// Detect Bible references in the given transcript text.
    ///
    /// Returns a list of Detection objects for each reference found.
    #[expect(
        clippy::too_many_lines,
        reason = "reference detection keeps ordered fallback logic in one pass"
    )]
    pub fn detect(&mut self, text: &str) -> Vec<Detection> {
        if is_hymn_or_song_number_command(text) {
            return Vec::new();
        }

        // Step 0: Clean filler phrases from the transcript
        let cleaned = clean_transcript(text, self.stt_language == "en");
        let text = &cleaned;
        let lower_text = text.to_lowercase();

        let mut detections = Vec::new();

        // Step 0b: Check for "previous verse" / "last verse" navigation commands
        if let Some(prev_detection) = self.check_previous_verse_command(text) {
            detections.push(prev_detection);
            return detections;
        }

        // Step 0c: Check if there's a pending incomplete reference.
        // Try to complete it with chapter/verse continuation, or expire on timeout.
        if let Some(ref incomplete) = self.incomplete.clone() {
            let elapsed = incomplete.timestamp.elapsed().as_millis();
            if elapsed > INCOMPLETE_REF_TIMEOUT_MS {
                // Timeout: clean up pending state (EDGE-02).
                self.incomplete = None;
            } else if !self
                .matcher
                .find_books(text)
                .iter()
                .any(|book_match| text[..book_match.start].trim().is_empty())
            {
                if let Some(cont) =
                    parser::try_extract_continuation(text, incomplete.chapter_is_default)
                {
                    match cont {
                        parser::Continuation::ChapterAndVerse(ch, v, verse_end) => {
                            let mut completed = incomplete.verse_ref.clone();
                            completed.chapter = ch;
                            completed.verse_start = v;
                            completed.verse_end = verse_end;
                            if is_valid_reference(completed.book_number, completed.chapter) {
                                detections.push(self.make_direct_detection(
                                    &completed,
                                    compute_confidence(&completed, &completed),
                                    text,
                                    0,
                                    text.len(),
                                ));
                                self.push_recent(&completed);
                                self.context.update(&completed);
                            }
                            self.incomplete = None;
                            return detections;
                        }
                        parser::Continuation::VerseOnly(v, verse_end) => {
                            let mut completed = incomplete.verse_ref.clone();
                            completed.verse_start = v;
                            completed.verse_end = verse_end;
                            if is_valid_reference(completed.book_number, completed.chapter) {
                                detections.push(self.make_direct_detection(
                                    &completed,
                                    compute_confidence(&completed, &completed),
                                    text,
                                    0,
                                    text.len(),
                                ));
                                self.push_recent(&completed);
                                self.context.update(&completed);
                            }
                            self.incomplete = None;
                            return detections;
                        }
                        parser::Continuation::ChapterOnly(ch) => {
                            // Update chapter, reset timeout, keep waiting for verse.
                            let mut updated = incomplete.verse_ref.clone();
                            updated.chapter = ch;
                            self.incomplete = Some(IncompleteRef {
                                verse_ref: updated.clone(),
                                timestamp: Instant::now(),
                                chapter_is_default: false,
                            });
                            self.context.update(&updated);
                            // Fall through to book matcher (text may also contain a new book)
                        }
                    }
                }
            }
        }

        // Step 1: Find all book name matches using Aho-Corasick
        let book_matches = self.matcher.find_books(text);

        // Step 1b: If the automaton found nothing, try fuzzy matching as fallback
        let fuzzy_matches: Vec<BookMatch>;
        let effective_matches: &[BookMatch] = if book_matches.is_empty() {
            fuzzy_matches = fuzzy::fuzzy_find_books(text)
                .into_iter()
                .map(|fm| BookMatch {
                    book_number: fm.book_number,
                    book_name: fm.book_name,
                    start: fm.start,
                    end: fm.end,
                })
                .collect();
            &fuzzy_matches
        } else {
            &book_matches
        };

        if effective_matches.is_empty() {
            if let Some(saved_detection) = self.check_saved_context_return(text) {
                detections.push(saved_detection);
                return detections;
            }
        }

        // Step 2 & 3: Parse references and resolve context
        for book_match in effective_matches {
            // Abbreviation/alias and fuzzy matches (e.g. "act", "mic", "pro",
            // "gal", "psalm") collide with everyday words. Honor them only when an
            // explicit chapter/verse reference immediately follows. A full
            // canonical book name is always trusted — it is how a book is spoken.
            let matched_text = text[book_match.start..book_match.end].to_ascii_lowercase();
            let is_canonical_name = matched_text == book_match.book_name.to_ascii_lowercase();
            if !is_canonical_name && !reference_context_follows(text, book_match.end) {
                continue;
            }
            if let Some(verse_ref) = parser::parse_reference(text, book_match) {
                // Resolve any partial references using context
                let mut resolved = self.context.resolve(&verse_ref);

                // Skip if we couldn't resolve to a meaningful reference
                if resolved.book_number == 0 || resolved.chapter == 0 {
                    self.context.update(&verse_ref);
                    continue;
                }

                // Skip impossible references (e.g., "Mark 30:1" — Mark has 16 chapters)
                if resolved.chapter > 0
                    && !is_valid_reference(resolved.book_number, resolved.chapter)
                {
                    continue;
                }

                // Chapter-only: immediately surface verse 1 for live navigation,
                // but keep the incomplete reference so a following "verse N"
                // can refine it without losing cross-segment behavior.
                if resolved.verse_start == 0 {
                    // Detect if chapter was explicitly spoken or defaulted.
                    let after_book = text[book_match.end..].trim();
                    let after_book_lower = after_book.to_lowercase();
                    let starts_with_verse_keyword = after_book_lower.starts_with("verse");
                    let mut has_explicit_chapter = after_book
                        .starts_with(|c: char| c.is_ascii_digit())
                        || matches!(
                            parser::try_extract_continuation(after_book, true),
                            Some(
                                parser::Continuation::ChapterAndVerse(..)
                                    | parser::Continuation::ChapterOnly(_)
                            )
                        );

                    // A bare re-mention of a book (no chapter spoken) must not
                    // clobber a chapter already established for the same book.
                    // e.g. "Philippians chapter 4 … this book philippians …
                    // verse 3" must stay in chapter 4, not reset to 1.
                    if !has_explicit_chapter {
                        if let Some(prev) = self.incomplete.as_ref() {
                            if !prev.chapter_is_default
                                && prev.verse_ref.book_number == resolved.book_number
                            {
                                resolved.chapter = prev.verse_ref.chapter;
                                has_explicit_chapter = true;
                            }
                        }
                    }
                    self.incomplete = Some(IncompleteRef {
                        verse_ref: resolved.clone(),
                        timestamp: Instant::now(),
                        chapter_is_default: !has_explicit_chapter && !starts_with_verse_keyword,
                    });
                    self.context.update(&resolved);
                    self.save_context_if_requested(&lower_text, &resolved);

                    // Only surface a chapter-only detection when the chapter is
                    // actually known (spoken or inherited). A lone book name with
                    // no chapter — often a mis-heard keyterm like "Esther" — is
                    // held for refinement but not emitted or auto-staged.
                    if has_explicit_chapter {
                        let mut chapter_start = resolved.clone();
                        chapter_start.verse_start = 1;
                        let snippet = extract_snippet(text, book_match.start, book_match.end);
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "timestamp millis won't exceed u64 for centuries"
                        )]
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;

                        detections.push(Detection {
                            verse_ref: chapter_start.clone(),
                            verse_id: None,
                            confidence: CHAPTER_ONLY_CONFIDENCE,
                            source: DetectionSource::DirectReference,
                            transcript_snippet: snippet,
                            detected_at: now,
                            is_chapter_only: true,
                        });
                        self.push_recent(&chapter_start);
                    }
                    continue;
                }

                // Full reference — also clear any pending incomplete
                self.incomplete = None;

                let confidence = compute_confidence(&resolved, &verse_ref);
                let snippet = extract_snippet(text, book_match.start, book_match.end);

                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "timestamp millis won't exceed u64 for centuries"
                )]
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let detection = Detection {
                    verse_ref: resolved.clone(),
                    verse_id: None,
                    confidence,
                    source: DetectionSource::DirectReference,
                    transcript_snippet: snippet,
                    detected_at: now,
                    is_chapter_only: false,
                };

                // Track in recent detections for "previous verse" support
                self.push_recent(&resolved);

                detections.push(detection);
                self.context.update(&resolved);
                self.save_context_if_requested(&lower_text, &resolved);
            }
        }

        // Fallback: an explicit "verse N" / "chapter N verse M" citation with
        // no (usable) book name in the fragment — resolve the book from recent
        // context as a conservative operator-visible candidate.
        if detections.is_empty() && self.incomplete.is_none() {
            if let Some(context_detection) = self.try_context_resolved_reference(text) {
                detections.push(context_detection);
            }
        }

        detections
    }

    /// Check if text contains a "previous verse" / "last verse" command.
    fn check_previous_verse_command(&self, text: &str) -> Option<Detection> {
        let lower = text.to_lowercase();
        for phrase in PREVIOUS_VERSE_PHRASES {
            if lower.contains(phrase) {
                if let Some(prev_ref) = self.recent_detections.front() {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "timestamp millis won't exceed u64 for centuries"
                    )]
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    return Some(Detection {
                        verse_ref: prev_ref.clone(),
                        verse_id: None,
                        confidence: 0.92,
                        source: DetectionSource::DirectReference,
                        transcript_snippet: text.to_string(),
                        detected_at: now,
                        is_chapter_only: false,
                    });
                }
            }
        }
        None
    }

    /// Push a verse ref to the recent detections queue (max 5).
    fn push_recent(&mut self, verse_ref: &VerseRef) {
        // Don't push duplicates of the most recent
        if let Some(front) = self.recent_detections.front() {
            if front.book_number == verse_ref.book_number
                && front.chapter == verse_ref.chapter
                && front.verse_start == verse_ref.verse_start
            {
                return;
            }
        }
        self.recent_detections.push_front(verse_ref.clone());
        if self.recent_detections.len() > 5 {
            self.recent_detections.pop_back();
        }
    }

    fn save_context_if_requested(&mut self, lower_text: &str, verse_ref: &VerseRef) {
        if !contains_any(lower_text, SAVE_CONTEXT_PHRASES) || verse_ref.book_number <= 0 {
            return;
        }

        let mut saved = verse_ref.clone();
        if saved.verse_start == 0 {
            saved.verse_start = 1;
        }
        if self
            .saved_contexts
            .front()
            .is_some_and(|front| same_book_chapter(front, &saved))
        {
            return;
        }
        self.saved_contexts.push_front(saved);
        if self.saved_contexts.len() > 3 {
            self.saved_contexts.pop_back();
        }
    }

    fn check_saved_context_return(&mut self, text: &str) -> Option<Detection> {
        let lower = text.to_lowercase();
        if !contains_any(&lower, RETURN_CONTEXT_PHRASES) {
            return None;
        }

        let mut restored = self.saved_contexts.front()?.clone();
        let mut is_chapter_only = false;
        match parser::try_extract_continuation(text, false)? {
            parser::Continuation::ChapterAndVerse(chapter, verse, verse_end) => {
                restored.chapter = chapter;
                restored.verse_start = verse;
                restored.verse_end = verse_end;
            }
            parser::Continuation::ChapterOnly(chapter) => {
                restored.chapter = chapter;
                restored.verse_start = 1;
                restored.verse_end = None;
                is_chapter_only = true;
            }
            parser::Continuation::VerseOnly(verse, verse_end) => {
                restored.verse_start = verse;
                restored.verse_end = verse_end;
            }
        }
        if !is_valid_reference(restored.book_number, restored.chapter) {
            return None;
        }

        if is_chapter_only {
            self.incomplete = Some(IncompleteRef {
                verse_ref: restored.clone(),
                timestamp: Instant::now(),
                chapter_is_default: false,
            });
        } else {
            self.incomplete = None;
        }
        self.context.update(&restored);
        self.push_recent(&restored);

        let mut detection = self.make_direct_detection(&restored, 0.92, text, 0, text.len());
        detection.is_chapter_only = is_chapter_only;
        Some(detection)
    }

    /// Resolve an explicit spoken "verse N" / "chapter N verse M" that carries
    /// no book name using recent reference context. Preachers routinely cite
    /// this way minutes after last naming the book, but the inferred book can
    /// be stale, so the candidate remains below the auto-fire threshold.
    fn try_context_resolved_reference(&mut self, text: &str) -> Option<Detection> {
        let continuation = parser::try_extract_standalone_reference(text)?;
        let partial = match continuation {
            parser::Continuation::ChapterAndVerse(chapter, verse, verse_end) => VerseRef {
                book_number: 0,
                book_name: String::new(),
                chapter,
                verse_start: verse,
                verse_end,
            },
            parser::Continuation::VerseOnly(verse, verse_end) => VerseRef {
                book_number: 0,
                book_name: String::new(),
                chapter: 0,
                verse_start: verse,
                verse_end,
            },
            // A keyword-less chapter switch is reading-mode navigation, not a
            // citation — leave it to the reading-mode tracker.
            parser::Continuation::ChapterOnly(_) => return None,
        };

        let resolved = self.context.resolve(&partial);
        if resolved.book_number == 0 || resolved.chapter == 0 || resolved.verse_start == 0 {
            return None;
        }
        if !is_valid_reference(resolved.book_number, resolved.chapter) {
            return None;
        }

        let detection =
            self.make_direct_detection(&resolved, CONTEXT_RESOLVED_CONFIDENCE, text, 0, text.len());
        self.push_recent(&resolved);
        self.context.update(&resolved);
        Some(detection)
    }

    /// Build a Detection from a resolved `VerseRef`.
    #[expect(
        clippy::unused_self,
        reason = "method kept on self for future extensibility"
    )]
    fn make_direct_detection(
        &self,
        verse_ref: &VerseRef,
        confidence: f64,
        text: &str,
        start: usize,
        end: usize,
    ) -> Detection {
        let snippet = extract_snippet(text, start, end.min(text.len()));
        #[expect(
            clippy::cast_possible_truncation,
            reason = "timestamp millis won't exceed u64 for centuries"
        )]
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Detection {
            verse_ref: verse_ref.clone(),
            verse_id: None,
            confidence,
            source: DetectionSource::DirectReference,
            transcript_snippet: snippet,
            detected_at: now,
            is_chapter_only: false,
        }
    }
}

impl Default for DirectDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a confidence score for the detection.
/// Full explicit references (book + chapter + verse) get 1.0.
/// References missing some parts get lower scores.
fn compute_confidence(_resolved: &VerseRef, original: &VerseRef) -> f64 {
    let mut confidence: f64 = 0.90;

    // Bonus for having explicit chapter
    if original.chapter > 0 {
        confidence += 0.04;
    }

    // Bonus for having explicit verse
    if original.verse_start > 0 {
        confidence += 0.04;
    }

    // Bonus for having explicit book
    if original.book_number > 0 {
        confidence += 0.02;
    }

    confidence.min(1.0_f64)
}

/// Extract a snippet of text around the reference for context.
fn extract_snippet(text: &str, start: usize, end: usize) -> String {
    let start = floor_char_boundary(text, start.min(text.len()));
    let end = floor_char_boundary(text, end.min(text.len()));
    let snippet_start = floor_char_boundary(text, start.saturating_sub(30));
    let snippet_end = if end.saturating_add(30) < text.len() {
        ceil_char_boundary(text, end + 30)
    } else {
        text.len()
    };

    // Adjust to word boundaries
    let snippet_start = text[snippet_start..start]
        .rfind(' ')
        .map_or(snippet_start, |p| snippet_start + p + 1);

    let snippet_end = text[end..snippet_end].find(' ').map_or(snippet_end, |p| {
        // Find the end of the relevant portion (after a few more words)
        let after_space = end + p + 1;
        text[after_space..snippet_end]
            .find(' ')
            .map_or(snippet_end, |p2| after_space + p2)
    });

    text[snippet_start..snippet_end].to_string()
}

fn floor_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(text: &str, mut idx: usize) -> usize {
    idx = idx.min(text.len());
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lone_book_name_without_chapter_is_not_emitted() {
        // A mis-heard bare book name (no chapter spoken) must not surface as
        // Book 1:1 (the "Esther" false positive from Deepgram keyterm bias).
        let mut detector = DirectDetector::new();
        let results = detector.detect("the judgment is possibly good news esther");
        assert!(results.is_empty());
    }

    #[test]
    fn suppressed_bare_book_still_completes_with_a_later_chapter_verse() {
        // Suppressing the bare-book emission must not lose the held reference:
        // a following "chapter N verse M" still completes it.
        let mut detector = DirectDetector::new();
        assert!(detector.detect("let us turn to esther").is_empty());
        let results = detector.detect("chapter 4 verse 14");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Esther");
        assert_eq!(results[0].verse_ref.chapter, 4);
        assert_eq!(results[0].verse_ref.verse_start, 14);
    }

    #[test]
    fn dangling_verse_keyword_waits_for_number_instead_of_emitting_verse_one() {
        let mut detector = DirectDetector::new();

        assert!(detector.detect("John verse").is_empty());
        assert!(detector.incomplete.is_some());

        let results = detector.detect("five");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "John");
        assert_eq!(results[0].verse_ref.chapter, 1);
        assert_eq!(results[0].verse_ref.verse_start, 5);
    }

    #[test]
    fn bare_book_rementions_keep_the_spoken_chapter() {
        // A bare re-mention of a book must not reset a chapter already spoken
        // for it: "Philippians chapter 4 … this book philippians … verse 3"
        // must resolve to 4:3, not 1:3.
        let mut detector = DirectDetector::new();
        detector.detect("turn to philippians chapter 4 what is in this book");
        detector.detect("what is contained in this book philippians");
        let results = detector.detect("and we're going to read verse 3");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Philippians");
        assert_eq!(results[0].verse_ref.chapter, 4);
        assert_eq!(results[0].verse_ref.verse_start, 3);
    }

    #[test]
    fn judgment_books_sermon_references_stay_on_spoken_passages() {
        let mut detector = DirectDetector::new();
        let mut refs = Vec::new();

        for segment in [
            "in daniel it says the court was seated the king james says the judgment was set",
            "what we notice in verse 10 is that at the same time that judgment begins",
            "let's turn to revelation chapter 20 and we're going to read verse 12",
            "the books were opened and another book was opened which is the book of life",
            "so we see there again that there are at least 2 books right",
            "so far and then it says and another book so that is now we have at least 3 books",
            "let's now turn a few chapters back to chapter 13 revelation chapter 13 and we're going to read verse 8",
            "what's the name of this book the full name of the book of life the lamb's book of life",
            "now what we're going to see if we turn to philippians chapter 4",
            "what is contained in this book philippians 4 and we're going to read verse 3",
        ] {
            refs.extend(
                detector
                    .detect(segment)
                    .into_iter()
                    .map(|detection| {
                        format!(
                            "{} {}:{}",
                            detection.verse_ref.book_name,
                            detection.verse_ref.chapter,
                            detection.verse_ref.verse_start
                        )
                    }),
            );
        }

        assert!(refs.contains(&"Revelation 20:12".to_string()));
        assert!(refs.contains(&"Revelation 13:8".to_string()));
        assert!(refs.contains(&"Philippians 4:3".to_string()));
        assert!(!refs.contains(&"Revelation 20:2".to_string()));
        assert!(!refs.contains(&"Revelation 20:3".to_string()));
        assert!(!refs.contains(&"Revelation 1:1".to_string()));
        assert!(!refs.contains(&"Revelation 3:1".to_string()));
        assert!(!refs.contains(&"Philippians 4:13".to_string()));
    }

    #[test]
    fn revelation_thirteen_book_of_life_sermon_does_not_become_isaiah() {
        let mut detector = DirectDetector::new();
        let mut refs = Vec::new();

        for segment in [
            "revelation chapter 13 and we're going to read verse 8",
            "now this verse is translated differently depending on which version of the bible that you're using",
            "but we won't focus on that for this morning",
            "it says all who dwell on the earth will worship him that is the beast the sea beast",
            "whose names have not been written in the book of life of the lamb slain from the foundation of the world",
            "what I want us to get from this verse is the full name of this verse of this book rather",
            "the full name of this book is the book of life of the lamb",
            "say that differently the lamb's book of life",
            "okay this lamb we are told was slain from the foundation of the world",
        ] {
            refs.extend(
                detector
                    .detect(segment)
                    .into_iter()
                    .map(|detection| {
                        format!(
                            "{} {}:{} {:.2}",
                            detection.verse_ref.book_name,
                            detection.verse_ref.chapter,
                            detection.verse_ref.verse_start,
                            detection.confidence
                        )
                    }),
            );
        }

        assert!(refs.iter().any(|r| r.starts_with("Revelation 13:8")));
        assert!(
            refs.iter().all(|r| !r.starts_with("Isaiah ")),
            "unexpected refs: {refs:?}"
        );
    }

    #[test]
    fn common_word_is_does_not_fabricate_isaiah() {
        // "Is" was registered as an Isaiah alias, so the everyday word "is"
        // matched Isaiah (case-insensitive, at word boundaries) and fabricated
        // references like "Isaiah 13:8" from prose such as
        // "this verse is ... chapter 13 verse 8". Isaiah must only match when
        // the book name is actually spoken.
        let mut detector = DirectDetector::new();
        let results = detector.detect("this is the verse we will read in chapter 13 verse 8");
        assert!(
            results.iter().all(|d| d.verse_ref.book_name != "Isaiah"),
            "unexpected Isaiah detection: {results:?}"
        );
    }

    #[test]
    fn word_like_abbreviations_require_adjacent_reference_context() {
        // "act"/"mic"/"pro" collide with everyday words. In prose they must not
        // fabricate a reference even when a chapter/verse appears later in the
        // sentence (the forward scan used to reach it).
        let mut detector = DirectDetector::new();
        assert!(detector
            .detect("we will act on this in chapter 3 verse 5")
            .iter()
            .all(|d| d.verse_ref.book_name != "Acts"));

        let mut detector = DirectDetector::new();
        assert!(detector
            .detect("check the mic before chapter 5 verse 2")
            .iter()
            .all(|d| d.verse_ref.book_name != "Micah"));

        // A real abbreviated reference (number adjacent) still resolves.
        let mut detector = DirectDetector::new();
        let results = detector.detect("Mic 5 verse 2");
        assert!(
            results.iter().any(|d| d.verse_ref.book_name == "Micah"
                && d.verse_ref.chapter == 5
                && d.verse_ref.verse_start == 2),
            "expected Micah 5:2, got {results:?}"
        );
    }

    #[test]
    fn spoken_psalm_alias_still_resolves_with_adjacent_number() {
        // "Psalm" (singular) is a non-canonical alias but the common spoken form;
        // it must still resolve when the chapter follows immediately.
        let mut detector = DirectDetector::new();
        let results = detector.detect("David in Psalm thirty two verse one now says");
        assert!(
            results.iter().any(|d| d.verse_ref.book_name == "Psalms"
                && d.verse_ref.chapter == 32
                && d.verse_ref.verse_start == 1),
            "expected Psalms 32:1, got {results:?}"
        );
    }

    #[test]
    fn test_basic_reference() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Jesus said in John 3:16 that God loved the world");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "John");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 16);
    }

    #[test]
    fn test_spoken_reference() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("David in Psalm thirty two verse one now says");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "Psalms");
        assert_eq!(results[0].verse_ref.chapter, 32);
        assert_eq!(results[0].verse_ref.verse_start, 1);
    }

    #[test]
    fn test_verse_range() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Let's read Romans 8:28-30 together");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "Romans");
        assert_eq!(results[0].verse_ref.chapter, 8);
        assert_eq!(results[0].verse_ref.verse_start, 28);
        assert_eq!(results[0].verse_ref.verse_end, Some(30));
    }

    #[test]
    fn test_numbered_book() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Paul wrote in 1 Corinthians 13:4 about love");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "1 Corinthians");
        assert_eq!(results[0].verse_ref.chapter, 13);
        assert_eq!(results[0].verse_ref.verse_start, 4);
    }

    #[test]
    fn test_chapter_only_held_as_incomplete() {
        // Chapter-only references are NOT emitted — just held as incomplete for refinement
        let mut detector = DirectDetector::new();
        let results = detector.detect("Genesis 3 is about the fall of man");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Genesis");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 1);
        assert!(results[0].is_chapter_only);
        assert!(detector.incomplete.is_some()); // Held for refinement
        let inc = detector.incomplete.as_ref().unwrap();
        assert_eq!(inc.verse_ref.book_name, "Genesis");
        assert_eq!(inc.verse_ref.chapter, 3);
    }

    #[test]
    fn test_chapter_only_no_duplicate_on_repeat() {
        // Same book+chapter in a subsequent call — still held, no emission
        let mut detector = DirectDetector::new();
        let results = detector.detect("Genesis 3");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);
        assert!(detector.incomplete.is_some());

        // Same text again — still held
        let results = detector.detect("Genesis 3");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);
        assert!(detector.incomplete.is_some());
    }

    #[test]
    fn test_incomplete_ref_completed_by_verse() {
        // Chapter-only held, then refined by verse continuation
        let mut detector = DirectDetector::new();
        // First: chapter-only — held as incomplete, not emitted
        let results = detector.detect("Genesis 3");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);
        assert!(detector.incomplete.is_some());

        // Second: verse continuation — refines the detection
        let results = detector.detect("verse 15");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Genesis");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 15);
        assert!(!results[0].is_chapter_only);
        assert!(detector.incomplete.is_none());
    }

    #[test]
    fn test_new_book_supersedes_incomplete() {
        // EDGE-01: a new book/chapter replaces the pending incomplete cleanly
        let mut detector = DirectDetector::new();
        let results = detector.detect("Genesis 3");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);
        assert!(detector.incomplete.is_some());

        // Different book — supersedes Genesis 3
        let results = detector.detect("let's look at John 1");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);
        // Incomplete now tracks John 1, not Genesis 3
        let inc = detector.incomplete.as_ref().unwrap();
        assert_eq!(inc.verse_ref.book_name, "John");
    }

    #[test]
    fn test_abandoned_partial_no_stale_state() {
        // EDGE-02: after timeout, incomplete is cleaned up without re-emission
        let mut detector = DirectDetector::new();
        let results = detector.detect("Genesis 3");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);
        assert!(detector.incomplete.is_some());

        // Simulate timeout by replacing with an expired timestamp (exceeds 15s)
        detector.incomplete = Some(IncompleteRef {
            verse_ref: detector.incomplete.as_ref().unwrap().verse_ref.clone(),
            timestamp: Instant::now()
                .checked_sub(std::time::Duration::from_secs(20))
                .unwrap(),
            chapter_is_default: detector.incomplete.as_ref().unwrap().chapter_is_default,
        });

        // Next detect call should clean up without emitting
        let results = detector.detect("something unrelated");
        assert!(results.is_empty());
        assert!(detector.incomplete.is_none());
    }

    #[test]
    fn test_previous_verse_command() {
        let mut detector = DirectDetector::new();
        // First detect a verse
        let results = detector.detect("John 3:16");
        assert!(!results.is_empty());

        // Then ask for "previous verse"
        let results = detector.detect("can you show me the last verse");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "John");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 16);
    }

    #[test]
    fn test_previous_verse_no_history() {
        let mut detector = DirectDetector::new();
        // No previous detection — should return empty
        let results = detector.detect("go back to that verse");
        assert!(results.is_empty());
    }

    #[test]
    fn test_no_reference() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("The weather is nice today");
        assert!(results.is_empty());
    }

    #[test]
    fn test_spoken_chapter_verse_keywords() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Isaiah chapter fifty three verse five");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "Isaiah");
        assert_eq!(results[0].verse_ref.chapter, 53);
        assert_eq!(results[0].verse_ref.verse_start, 5);
    }

    #[test]
    fn test_required_explicit_reference_phrases() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("Genesis 1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Genesis");
        assert_eq!(results[0].verse_ref.chapter, 1);
        assert_eq!(results[0].verse_ref.verse_start, 1);
        assert!(results[0].is_chapter_only);

        let mut detector = DirectDetector::new();
        let results = detector.detect("John chapter 1 verse 1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "John");
        assert_eq!(results[0].verse_ref.chapter, 1);
        assert_eq!(results[0].verse_ref.verse_start, 1);

        let mut detector = DirectDetector::new();
        let results = detector.detect("Acts chapter 1 verse 1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Acts");
        assert_eq!(results[0].verse_ref.chapter, 1);
        assert_eq!(results[0].verse_ref.verse_start, 1);
    }

    #[test]
    fn test_impossible_reference_does_not_leave_stale_continuation_state() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("Mark 30:1");
        assert!(results.is_empty());

        let results = detector.detect("verse two");
        assert!(
            results.is_empty(),
            "invalid explicit refs must not seed a pending reference"
        );
        assert!(detector.incomplete.is_none());
        assert!(detector.recent_detections().is_empty());
    }

    #[test]
    fn test_multiple_references() {
        let mut detector = DirectDetector::new();
        let results =
            detector.detect("Compare John 3:16 with Romans 5:8 for understanding God's love");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].verse_ref.book_name, "John");
        assert_eq!(results[1].verse_ref.book_name, "Romans");
    }

    #[test]
    fn test_confidence_range() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("John 3:16");
        assert!(!results.is_empty());
        assert!(results[0].confidence >= 0.90);
        assert!(results[0].confidence <= 1.0);
    }

    #[test]
    fn test_detection_source() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("John 3:16");
        assert!(!results.is_empty());
        assert!(matches!(
            results[0].source,
            DetectionSource::DirectReference
        ));
    }

    #[test]
    fn test_clean_transcript() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Please open your bibles to Ephesians chapter 6 verse 10");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "Ephesians");
    }

    #[test]
    fn test_clean_transcript_lets_turn_to() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Let's turn to Romans 8:28 and read together");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "Romans");
        assert_eq!(results[0].verse_ref.chapter, 8);
        assert_eq!(results[0].verse_ref.verse_start, 28);
    }

    #[test]
    fn test_clean_transcript_the_bible_says_in() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("The bible says in John 3:16 that God loved the world");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "John");
    }

    #[test]
    fn test_clean_transcript_look_at() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Now look at Genesis 1:1 for the beginning");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "Genesis");
    }

    #[test]
    fn test_reference_corrections_numbered_books() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("one john chapter two verse three");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "1 John");
        assert_eq!(results[0].verse_ref.chapter, 2);
        assert_eq!(results[0].verse_ref.verse_start, 3);

        let mut detector = DirectDetector::new();
        let results = detector.detect("second corinthian chapter five verse seventeen");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "2 Corinthians");
        assert_eq!(results[0].verse_ref.chapter, 5);
        assert_eq!(results[0].verse_ref.verse_start, 17);
    }

    #[test]
    fn test_reference_corrections_psalm_chapter_mishears() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("songs chapter twenty three verse one");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Psalms");
        assert_eq!(results[0].verse_ref.chapter, 23);
        assert_eq!(results[0].verse_ref.verse_start, 1);
    }

    #[test]
    fn voice_command_utterance_matches_hymn_translation_and_navigation() {
        // Commands the dedicated command/reading paths already handle — live
        // semantic paraphrase search must skip these so it does not flood the
        // detections panel with keyword noise.
        assert!(is_voice_command_utterance("Hymn number 46"));
        assert!(is_voice_command_utterance(
            "I need the new living translation."
        ));
        assert!(is_voice_command_utterance("King James Version"));
        assert!(is_voice_command_utterance("let's go to the next verse"));
        assert!(is_voice_command_utterance("go back to the previous verse"));
        assert!(is_voice_command_utterance("in the same chapter verse 17"));
    }

    #[test]
    fn voice_command_utterance_ignores_prose_and_bare_references() {
        // Sermon prose must remain eligible for semantic paraphrase detection.
        assert!(!is_voice_command_utterance(
            "For God so loved the world that he gave his only begotten son"
        ));
        // A bare scripture reference is a reference, not a voice command; the
        // reference path (not this predicate) is responsible for it.
        assert!(!is_voice_command_utterance("Genesis chapter 3 verse 15"));
    }

    #[test]
    fn hymn_number_commands_do_not_become_bible_references() {
        let mut detector = DirectDetector::new();

        assert!(detector.detect("hymn 12").is_empty());
        assert!(detector.detect("Adventist hymnal 100").is_empty());
        assert!(detector
            .detect("Seventh-day Adventist hymnal one hundred")
            .is_empty());
        assert!(detector.detect("song two hundred fifty one").is_empty());
        assert!(detector.detect("please open song number 251").is_empty());
        assert!(detector.detect("lied 12").is_empty());
        assert!(detector.detect("Adventiste liedboek 100").is_empty());
        assert!(detector
            .detect("Sewendedag Adventiste lied nommer een honderd")
            .is_empty());
        assert!(detector.detect("lied drie en twintig").is_empty());
    }

    #[test]
    fn active_stt_provider_reference_variants_stay_direct_accurate() {
        let cases = [
            ("vosk", "john chapter three verse sixteen"),
            ("deepgram", "John 3:16"),
            ("deepgram", "John three sixteen"),
        ];

        for (provider, transcript) in cases {
            let mut detector = DirectDetector::new();
            let results = detector.detect(transcript);
            assert_eq!(
                results.len(),
                1,
                "{provider} transcript should produce one direct reference"
            );
            assert_eq!(results[0].verse_ref.book_name, "John", "{provider}");
            assert_eq!(results[0].verse_ref.chapter, 3, "{provider}");
            assert_eq!(results[0].verse_ref.verse_start, 16, "{provider}");
            assert!(
                matches!(results[0].source, DetectionSource::DirectReference),
                "{provider} transcript should stay on the direct path"
            );
        }
    }

    #[test]
    fn hymn_number_guard_preserves_scripture_song_references() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("Song of Solomon 2:1");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Song of Solomon");
        assert_eq!(results[0].verse_ref.chapter, 2);
        assert_eq!(results[0].verse_ref.verse_start, 1);
    }

    #[test]
    fn hymn_number_guard_preserves_psalm_song_chapter_mishears() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("song chapter twenty three verse one");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Psalms");
        assert_eq!(results[0].verse_ref.chapter, 23);
        assert_eq!(results[0].verse_ref.verse_start, 1);
    }

    #[test]
    fn test_reference_corrections_do_not_replace_inside_words() {
        assert_eq!(
            replace_case_insensitive_phrase("someone johnny said one john", "one john", "1 John"),
            "someone johnny said 1 John"
        );
    }

    #[test]
    fn test_fuzzy_fallback_filipians() {
        let mut detector = DirectDetector::new();
        let results = detector.detect("Filipians chapter 4 verse 13");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.book_name, "Philippians");
        assert_eq!(results[0].verse_ref.chapter, 4);
        assert_eq!(results[0].verse_ref.verse_start, 13);
    }

    // ========== Translation Command Detection Tests ==========

    #[test]
    fn test_translation_command_basic_niv() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("give me niv"),
            Some("NIV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("read in niv"),
            Some("NIV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("switch to niv"),
            Some("NIV".to_string())
        );
    }

    #[test]
    fn test_translation_command_natural_language() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("can i have it in amplified"),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("can i have that in amplified version"),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("can i have it in esv"),
            Some("ESV".to_string())
        );
    }

    #[test]
    fn test_translation_command_full_names() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("new international version"),
            Some("NIV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("king james version"),
            Some("KJV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("english standard version"),
            Some("ESV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("amplified bible"),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("new living translation"),
            Some("NLT".to_string())
        );
    }

    #[test]
    fn test_translation_command_ignores_king_james_narration() {
        let detector = DirectDetector::new();
        // "the king james says ..." is narration about the KJV reading, not a
        // command to switch translation. It must not flip the active version.
        assert_eq!(
            detector.detect_translation_command(
                "the court was seated the king james says the judgment was set"
            ),
            None
        );
    }

    #[test]
    fn test_translation_command_bare_abbreviations() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("niv"),
            Some("NIV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("esv"),
            Some("ESV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("kjv"),
            Some("KJV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("amp"),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("nasb"),
            Some("NASB".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("nkjv"),
            Some("NKJV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("nlt"),
            Some("NLT".to_string())
        );
    }

    #[test]
    fn test_translation_command_in_sentence() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("show me genesis 3:16 in the amplified"),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("read john 3:16 in niv"),
            Some("NIV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("i want to read that in the message"),
            Some("MSG".to_string())
        );
    }

    #[test]
    fn translation_command_accepts_i_want_nkjv_and_nlt_phrases() {
        let detector = DirectDetector::new();

        assert_eq!(
            detector.detect_translation_command("I want the NKJV"),
            Some("NKJV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("I want the NJKV"),
            Some("NKJV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("I want the NLT"),
            Some("NLT".to_string())
        );
    }

    #[test]
    fn test_translation_command_message_bible() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("give me the message"),
            Some("MSG".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("read in the message"),
            Some("MSG".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("switch to message"),
            Some("MSG".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("message version"),
            Some("MSG".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("message bible"),
            Some("MSG".to_string())
        );
    }

    #[test]
    fn test_translation_command_csb_hcsb() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("give me csb"),
            Some("CSB".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("christian standard bible"),
            Some("CSB".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("give me hcsb"),
            Some("HCSB".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("holman christian standard"),
            Some("HCSB".to_string())
        );
    }

    #[test]
    fn test_translation_command_revised_standard() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("give me rsv"),
            Some("RSV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("revised standard version"),
            Some("RSV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("give me nrsv"),
            Some("NRSV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("new revised standard"),
            Some("NRSV".to_string())
        );
    }

    #[test]
    fn test_translation_command_good_news() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("give me gnt"),
            Some("GNT".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("give me gnb"),
            Some("GNT".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("good news translation"),
            Some("GNT".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("good news bible"),
            Some("GNT".to_string())
        );
    }

    #[test]
    fn test_translation_command_net_cev() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("give me net"),
            Some("NET".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("new english translation"),
            Some("NET".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("give me cev"),
            Some("CEV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("contemporary english version"),
            Some("CEV".to_string())
        );
    }

    #[test]
    fn test_translation_command_non_english() {
        let detector = DirectDetector::new();
        // Spanish
        assert_eq!(
            detector.detect_translation_command("give me spanish"),
            Some("SpaRV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("read in reina valera"),
            Some("SpaRV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("in spanish"),
            Some("SpaRV".to_string())
        );

        // French
        assert_eq!(
            detector.detect_translation_command("give me french"),
            Some("FreJND".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("read in french"),
            Some("FreJND".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("darby french"),
            Some("FreJND".to_string())
        );

        // Portuguese
        assert_eq!(
            detector.detect_translation_command("give me portuguese"),
            Some("PorBLivre".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("biblia livre"),
            Some("PorBLivre".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("in portuguese"),
            Some("PorBLivre".to_string())
        );
    }

    #[test]
    fn test_translation_command_english_maps_to_kjv() {
        let detector = DirectDetector::new();
        for phrase in [
            "read in english",
            "switch to english",
            "back to english",
            "give me english",
            "in english",
            "english version",
            "english", // bare word
        ] {
            assert_eq!(
                detector.detect_translation_command(phrase),
                Some("KJV".to_string()),
                "phrase: {phrase}"
            );
        }
    }

    #[test]
    fn test_translation_command_english_does_not_shadow_esv() {
        // The specific "english standard version" phrase must still resolve to
        // ESV even though a generic English -> KJV command now exists.
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("read in english standard version"),
            Some("ESV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("can i have it in esv"),
            Some("ESV".to_string())
        );
    }

    #[test]
    fn test_translation_command_spanish_english_round_trip() {
        // A pastor switches to Spanish mid-sermon, then asks to come back to
        // English — both directions must be recognized by voice command.
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("read in spanish"),
            Some("SpaRV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("switch to english"),
            Some("KJV".to_string())
        );
        // The explicit-version path back to English still works too.
        assert_eq!(
            detector.detect_translation_command("switch to kjv"),
            Some("KJV".to_string())
        );
    }

    #[test]
    fn test_translation_command_case_insensitive() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("GIVE ME NIV"),
            Some("NIV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("Give Me Amplified"),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("CAN I HAVE IT IN ESV"),
            Some("ESV".to_string())
        );
    }

    #[test]
    fn test_translation_command_show_me_variations() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("show me niv"),
            Some("NIV".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("show me amplified"),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector.detect_translation_command("show me the message"),
            Some("MSG".to_string())
        );
    }

    #[test]
    fn test_translation_command_no_match() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_translation_command("genesis 3 verse 16"),
            None
        );
        assert_eq!(
            detector.detect_translation_command("the weather is nice"),
            None
        );
        assert_eq!(
            detector.detect_translation_command("tell me about the bible"),
            None
        );
    }

    #[test]
    fn test_translation_command_partial_match() {
        let detector = DirectDetector::new();
        // Should match even with extra words
        assert_eq!(
            detector.detect_translation_command(
                "i would like to read that in amplified version please"
            ),
            Some("AMP".to_string())
        );
        assert_eq!(
            detector
                .detect_translation_command("could you show me that verse in the niv translation"),
            Some("NIV".to_string())
        );
    }

    #[test]
    fn test_stt_voice_command_stop() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_stt_voice_command("please stop transcribing"),
            Some(SttVoiceCommand::Stop)
        );
        assert_eq!(
            detector.detect_stt_voice_command("stop listening now"),
            Some(SttVoiceCommand::Stop)
        );
    }

    #[test]
    fn test_stt_voice_command_start() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_stt_voice_command("start transcribing"),
            Some(SttVoiceCommand::Start)
        );
        assert_eq!(
            detector.detect_stt_voice_command("please start listening"),
            Some(SttVoiceCommand::Start)
        );
    }

    #[test]
    fn test_stt_voice_command_no_match() {
        let detector = DirectDetector::new();
        assert_eq!(
            detector.detect_stt_voice_command("let us keep listening to the sermon"),
            None
        );
    }

    // ========== Cross-Segment Detection Tests ==========

    #[test]
    fn test_cross_segment_acts_3_22() {
        // The exact bug scenario from logs:
        // "...Acts" → "chapter three..." → "22..."
        let mut detector = DirectDetector::new();

        // Segment 1: Book-only "Acts" — held for refinement, not emitted.
        let results = detector.detect("God had put in his mouth. Acts");
        assert!(results.is_empty());
        assert!(detector.incomplete.is_some());
        let inc = detector.incomplete.as_ref().unwrap();
        assert_eq!(inc.verse_ref.book_name, "Acts");
        assert!(inc.chapter_is_default);

        // Segment 2: Chapter continuation
        let results = detector.detect("chapter three, and I'm reading from verse");
        assert!(results.is_empty());
        assert!(detector.incomplete.is_some());
        let inc = detector.incomplete.as_ref().unwrap();
        assert_eq!(inc.verse_ref.chapter, 3);
        assert!(!inc.chapter_is_default);

        // Segment 3: Verse completion via bare number
        let results = detector.detect("22. Acts three, for Moses truly");
        let result = results.iter().find(|r| !r.is_chapter_only).unwrap();
        assert_eq!(result.verse_ref.book_name, "Acts");
        assert_eq!(result.verse_ref.chapter, 3);
        assert_eq!(result.verse_ref.verse_start, 22);
    }

    #[test]
    fn test_cross_segment_chapter_and_verse_combined() {
        // Book-only → "chapter 3 verse 22" in one segment
        let mut detector = DirectDetector::new();

        let results = detector.detect("let's read Acts");
        assert!(results.is_empty()); // bare book held for refinement, not emitted

        let results = detector.detect("chapter 3 verse 22");
        let result = results.iter().find(|r| !r.is_chapter_only).unwrap();
        assert_eq!(result.verse_ref.book_name, "Acts");
        assert_eq!(result.verse_ref.chapter, 3);
        assert_eq!(result.verse_ref.verse_start, 22);
    }

    #[test]
    fn test_cross_segment_dangling_chapter_keyword_then_number_verse() {
        // Regression from live Vosk transcript:
        // "daniel chapter" → "1 verse 5" must stay on Daniel, not fall to Genesis.
        let mut detector = DirectDetector::new();

        let results = detector.detect("daniel chapter");
        assert!(results.is_empty());
        assert!(detector.incomplete.is_some());

        let results = detector.detect("1 verse 5");
        assert_eq!(results.len(), 1);
        assert!(!results[0].is_chapter_only);
        assert_eq!(results[0].verse_ref.book_name, "Daniel");
        assert_eq!(results[0].verse_ref.chapter, 1);
        assert_eq!(results[0].verse_ref.verse_start, 5);
    }

    #[test]
    fn transcript_philippians_dangling_chapter_then_four_verse_six() {
        let mut detector = DirectDetector::new();

        let first = detector.detect("okay and then let's go to philippians chapter");
        assert!(first.is_empty());

        let results = detector.detect("4 verse 6");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Philippians");
        assert_eq!(results[0].verse_ref.chapter, 4);
        assert_eq!(results[0].verse_ref.verse_start, 6);
    }

    #[test]
    fn test_cross_segment_daniel_7_dangling_number_verse_waits_for_actual_verse() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("Daniel 7");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);
        assert_eq!(results[0].verse_ref.book_name, "Daniel");
        assert_eq!(results[0].verse_ref.chapter, 7);
        assert_eq!(results[0].verse_ref.verse_start, 1);
        assert!(detector.incomplete.is_some());

        let results = detector.detect("7 verse");
        assert!(results.is_empty());
        assert!(detector.incomplete.is_some());

        let results = detector.detect("9");
        assert_eq!(results.len(), 1);
        assert!(!results[0].is_chapter_only);
        assert_eq!(results[0].verse_ref.book_name, "Daniel");
        assert_eq!(results[0].verse_ref.chapter, 7);
        assert_eq!(results[0].verse_ref.verse_start, 9);
    }

    #[test]
    fn test_bare_number_as_chapter_after_book_only() {
        // "Acts" → "3" → "22"
        let mut detector = DirectDetector::new();

        let results = detector.detect("turn to Acts");
        assert!(results.is_empty()); // bare book held for refinement, not emitted
        assert!(detector.incomplete.as_ref().unwrap().chapter_is_default);

        // Bare "3" = chapter (because book-only)
        let results = detector.detect("3");
        assert!(results.is_empty());
        let inc = detector.incomplete.as_ref().unwrap();
        assert_eq!(inc.verse_ref.chapter, 3);

        // Bare "22" = verse (chapter already set)
        let results = detector.detect("22");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 22);
    }

    #[test]
    fn test_verse_keyword_anywhere_in_text() {
        // "Genesis 3" → "and I'm reading from verse 15"
        let mut detector = DirectDetector::new();

        let results = detector.detect("Genesis 3");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);

        let results = detector.detect("and I'm reading from verse 15");
        assert!(!results.is_empty());
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 15);
    }

    #[test]
    fn transcript_revelation_context_prefers_read_verse_after_skipped_verse() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("turn with me in your Bibles to Revelation chapter 14");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);

        let results = detector
            .detect("we won't read verse six, but verse seven, the message of the first angel");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Revelation");
        assert_eq!(results[0].verse_ref.chapter, 14);
        assert_eq!(results[0].verse_ref.verse_start, 7);
    }

    #[test]
    fn transcript_numbers_context_handles_damaged_range() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("do you remember the story in the book of Numbers");
        assert!(results.is_empty());
        assert!(detector.incomplete.is_some());

        let results = detector.detect("chapter 21:es 4-9");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Numbers");
        assert_eq!(results[0].verse_ref.chapter, 21);
        assert_eq!(results[0].verse_ref.verse_start, 4);
        assert_eq!(results[0].verse_ref.verse_end, Some(9));
    }

    #[test]
    fn transcript_same_chapter_and_range_survives_detection() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("John 12 verse 32 and 33");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "John");
        assert_eq!(results[0].verse_ref.chapter, 12);
        assert_eq!(results[0].verse_ref.verse_start, 32);
        assert_eq!(results[0].verse_ref.verse_end, Some(33));
    }

    #[test]
    fn transcript_verse_only_correction_prefers_corrected_verse() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("Romans 4");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);

        let results = detector.detect("then verse 21, sorry 22");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Romans");
        assert_eq!(results[0].verse_ref.chapter, 4);
        assert_eq!(results[0].verse_ref.verse_start, 22);
    }

    #[test]
    fn transcript_keep_your_place_restores_saved_chapter() {
        let mut detector = DirectDetector::new();

        let results = detector.detect("keep your place in John 3");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_chapter_only);

        let results = detector.detect("John 12 verse 32 and 33");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.chapter, 12);

        let results = detector.detect("so back in chapter 3 verse 15");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "John");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 15);
    }

    #[test]
    fn afrikaans_detects_johannes_3_vers_16() {
        let mut detector = DirectDetector::for_stt_language("af");
        let results = detector.detect("Johannes 3 vers 16");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Johannes");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 16);
    }

    #[test]
    fn afrikaans_detects_deuteronomium_16_vers_18() {
        let mut detector = DirectDetector::for_stt_language("af");
        let results = detector.detect("Deuteronomium 16 vers 18");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Deuteronomium");
        assert_eq!(results[0].verse_ref.chapter, 16);
        assert_eq!(results[0].verse_ref.verse_start, 18);
    }

    #[test]
    fn afrikaans_detects_matteus_20_vers_25() {
        let mut detector = DirectDetector::for_stt_language("af");
        let results = detector.detect("Matteus 20 vers 25");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Matteus");
        assert_eq!(results[0].verse_ref.chapter, 20);
        assert_eq!(results[0].verse_ref.verse_start, 25);
    }

    #[test]
    fn afrikaans_translation_command() {
        let detector = DirectDetector::for_stt_language("af");
        assert_eq!(
            detector.detect_translation_command("switch to afrikaans please"),
            Some("Afr1953".into())
        );
    }

    #[test]
    fn spanish_detects_juan_3_versiculo_16() {
        let mut detector = DirectDetector::for_stt_language("es");
        let results = detector.detect("Juan 3 versiculo 16");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Juan");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 16);
    }

    #[test]
    fn french_detects_jean_chapitre_3_verset_16() {
        let mut detector = DirectDetector::for_stt_language("fr");
        let results = detector.detect("Jean chapitre 3 verset 16");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Jean");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 16);
    }

    #[test]
    fn portuguese_detects_joao_3_versiculo_16() {
        let mut detector = DirectDetector::for_stt_language("pt");
        let results = detector.detect("João 3 versiculo 16");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verse_ref.book_name, "Joao");
        assert_eq!(results[0].verse_ref.chapter, 3);
        assert_eq!(results[0].verse_ref.verse_start, 16);
    }
}
