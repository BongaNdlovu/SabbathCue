use std::collections::HashSet;
use std::time::Instant;

use serde::Serialize;

use crate::direct::books::BOOKS;
use crate::direct::parser::parse_spoken_number;

/// Timeout: pause reading mode after 3 minutes of no verse matches.
/// Context is maintained for ~3 minutes. Verses stay loaded for re-activation.
const READING_MODE_TIMEOUT_MS: u128 = 180_000;

/// Minimum word overlap ratio to consider a transcript matching a verse.
const MIN_WORD_OVERLAP: f64 = 0.40;

/// A verse loaded for reading mode tracking.
#[derive(Debug, Clone)]
struct LoadedVerse {
    verse_number: i32,
    text: String,
    /// Pre-computed lowercase word set for fast matching.
    words: HashSet<String>,
    word_count: usize,
}

/// Result when reading mode advances to a new verse.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadingAdvance {
    pub book_number: i32,
    pub book_name: String,
    pub chapter: i32,
    pub verse: i32,
    pub verse_text: String,
    pub reference: String,
    pub confidence: f64,
}

/// Signal that reading mode wants to change to a different chapter.
/// The caller must load the new chapter from the database and call `start()`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterChange {
    pub book_number: i32,
    pub book_name: String,
    pub new_chapter: i32,
    /// Optional verse number to start at (e.g., "chapter 3 verse 5" → verse 5)
    /// If None, starts at verse 1
    pub start_verse: Option<i32>,
    /// Whether the caller should emit the starting verse to the UI immediately.
    ///
    /// Bare chapter navigation ("chapter 13") updates reading context but should
    /// not push verse 1 into preview/live before the actual verse is spoken.
    pub emit_start_verse: bool,
}

/// Context for interpreting bare numbers after "chapter" commands.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BareNumberContext {
    /// No special context - bare numbers are verses in current chapter
    None,
    /// After "Genesis chapter" - next bare number is a chapter number
    ExpectingChapter,
    /// After navigating to a chapter - next bare number is a verse in that chapter
    ExpectingVerse,
}

/// Tracks the current reading position and matches transcripts against
/// expected verse text to auto-advance through a passage.
///
/// Activated when direct detection catches a verse reference. Pre-loads
/// the remaining verses in the chapter. On each transcript, compares
/// word overlap against the current and next verse to detect advancement.
pub struct ReadingMode {
    active: bool,
    book_number: i32,
    book_name: String,
    chapter: i32,
    /// Index into `verses` for the current verse being read.
    current_index: usize,
    /// All verses from the starting verse to end of chapter.
    verses: Vec<LoadedVerse>,
    /// Last time a verse match was found.
    last_match_time: Instant,
    /// Accumulated transcript text since last advance (for multi-fragment matching).
    accumulated_text: String,
    /// Context for interpreting the next bare number
    bare_number_context: BareNumberContext,
    /// Book+chapter of the current run of consecutive strong out-of-scope semantic hits.
    out_of_scope_streak_scope: Option<(i32, i32)>,
    /// Length of that run; reset whenever a verse in scope matches.
    out_of_scope_streak: u32,
}

impl ReadingMode {
    /// Create an inactive reading mode instance.
    pub fn new() -> Self {
        Self {
            active: false,
            book_number: 0,
            book_name: String::new(),
            chapter: 0,
            current_index: 0,
            verses: Vec::new(),
            last_match_time: Instant::now(),
            accumulated_text: String::new(),
            bare_number_context: BareNumberContext::None,
            out_of_scope_streak_scope: None,
            out_of_scope_streak: 0,
        }
    }

    /// Record a strong out-of-scope semantic hit and return the streak length.
    /// Consecutive hits on the same book+chapter grow the streak; a hit on a
    /// different out-of-scope chapter restarts it. Any in-scope verse match
    /// resets it to zero.
    pub fn note_out_of_scope_hit(&mut self, book_number: i32, chapter: i32) -> u32 {
        let scope = (book_number, chapter);
        if self.out_of_scope_streak_scope == Some(scope) {
            self.out_of_scope_streak += 1;
        } else {
            self.out_of_scope_streak_scope = Some(scope);
            self.out_of_scope_streak = 1;
        }
        self.out_of_scope_streak
    }

    fn reset_out_of_scope_streak(&mut self) {
        self.out_of_scope_streak_scope = None;
        self.out_of_scope_streak = 0;
    }

    /// Activate reading mode starting from the given verse.
    ///
    /// `verses` should be `(verse_number, verse_text)` pairs for all verses
    /// from the starting verse to the end of the chapter.
    pub fn start(
        &mut self,
        book_number: i32,
        book_name: &str,
        chapter: i32,
        start_verse: i32,
        verses: Vec<(i32, String)>,
    ) {
        // Load ALL chapter verses so "verse N" can navigate backward too.
        let loaded: Vec<LoadedVerse> = verses
            .into_iter()
            .map(|(v, text)| {
                let words = text_to_word_set(&text);
                let word_count = words.len();
                LoadedVerse {
                    verse_number: v,
                    text,
                    words,
                    word_count,
                }
            })
            .collect();

        if loaded.is_empty() {
            log::warn!("[READING] No verses loaded for {book_name} {chapter}:{start_verse}");
            return;
        }

        // Position cursor at start_verse; fall back to first verse.
        let start_index = loaded
            .iter()
            .position(|v| v.verse_number == start_verse)
            .unwrap_or(0);

        log::info!(
            "[READING] Started: {book_name} {chapter}:{start_verse} ({} verses loaded)",
            loaded.len()
        );

        self.active = true;
        self.book_number = book_number;
        self.book_name = book_name.to_string();
        self.chapter = chapter;
        self.current_index = start_index;
        self.verses = loaded;
        self.last_match_time = Instant::now();
        self.accumulated_text.clear();
        self.reset_out_of_scope_streak();
    }

    /// Check if reading mode is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Check if verses are still loaded (paused but resumable).
    pub fn has_verses(&self) -> bool {
        !self.verses.is_empty()
    }

    /// Resume from the current position (re-activate after pause/toggle).
    pub fn resume(&mut self) {
        if !self.verses.is_empty() {
            self.active = true;
            self.last_match_time = Instant::now();
            self.reset_out_of_scope_streak();
            let verse = self
                .verses
                .get(self.current_index)
                .map_or(0, |v| v.verse_number);
            log::info!(
                "[READING] Resumed at: {} {}:{verse}",
                self.book_name,
                self.chapter
            );
        }
    }

    /// Pause reading mode without clearing verses (resumable), e.g. when the
    /// speaker has clearly moved to a different passage before the timeout.
    pub fn pause(&mut self) {
        if self.active {
            log::info!(
                "[READING] Paused: {} {} (verses retained)",
                self.book_name,
                self.chapter
            );
        }
        self.active = false;
        // Drop the pre-pause transcript: the speaker has moved elsewhere, so
        // letting it merge with post-resume speech can fake a verse overlap.
        self.accumulated_text.clear();
    }

    /// Seconds since the last verse match (or since activation if none yet).
    pub fn seconds_since_last_match(&self) -> u64 {
        self.last_match_time.elapsed().as_secs()
    }

    /// Get the book number being tracked.
    pub fn current_book(&self) -> i32 {
        self.book_number
    }

    /// Get the chapter being tracked.
    pub fn current_chapter(&self) -> i32 {
        self.chapter
    }

    /// Get the book name being tracked.
    pub fn current_book_name(&self) -> &str {
        &self.book_name
    }

    /// Get the current verse number being tracked.
    pub fn current_verse(&self) -> Option<i32> {
        if self.active {
            self.verses.get(self.current_index).map(|v| v.verse_number)
        } else {
            None
        }
    }

    /// Set the bare number context (for chapter-only detections).
    /// Call this when a "book chapter" pattern is detected without a verse number.
    pub fn set_expecting_chapter(&mut self) {
        self.bare_number_context = BareNumberContext::ExpectingChapter;
        log::info!("[READING] Context set to ExpectingChapter");
    }

    /// Fully deactivate reading mode and clear all loaded verses.
    /// Called when the user turns the toggle OFF.
    pub fn deactivate(&mut self) {
        if self.active || !self.verses.is_empty() {
            log::info!("[READING] Deactivated (verses cleared)");
        }
        self.active = false;
        self.verses.clear();
        self.accumulated_text.clear();
    }

    /// Check if the transcript contains a chapter navigation command.
    ///
    /// Recognises "chapter N", "next chapter", and "previous chapter" anywhere
    /// in the text. Returns `Some(ChapterChange)` when a different chapter is
    /// requested, `None` otherwise.
    ///
    /// Also sets `bare_number_context` when "chapter" keyword is detected without a number.
    #[expect(
        clippy::too_many_lines,
        reason = "chapter command parsing is intentionally kept together for readability"
    )]
    pub fn check_chapter_command(&mut self, text: &str) -> Option<ChapterChange> {
        if self.verses.is_empty() {
            return None;
        }

        // The command grammar here is English-only, so English hesitation
        // fillers are always noise — and "um" would otherwise parse as the
        // number 1 (Portuguese) and fake chapter/verse navigation.
        let lower = crate::direct::parser::strip_english_filler_words(&text.to_lowercase());
        let trimmed = lower.trim();

        // Check if text contains "chapter" keyword without a number following it
        // This sets context for the next bare number to be interpreted as chapter
        // BUT exclude "next chapter" and "previous chapter" commands
        if !trimmed.contains("next")
            && !trimmed.contains("previous")
            && !trimmed.contains("verse")
            && ((trimmed.contains("chapter") && trimmed.ends_with("chapter"))
                || trimmed.ends_with("chapter?")
                || trimmed.ends_with("chapter."))
        {
            log::info!("[READING] Detected 'chapter' keyword without number - expecting chapter number next");
            self.bare_number_context = BareNumberContext::ExpectingChapter;
            return None; // No navigation yet, just set context
        }

        // Check for bare number(s) when expecting chapter or verse
        if self.bare_number_context != BareNumberContext::None {
            if let Some((first_num, rest)) = parse_number_and_rest(trimmed) {
                let rest_trimmed = rest.trim();

                match self.bare_number_context {
                    BareNumberContext::ExpectingChapter => {
                        // Check if there's a second number (e.g., "5 2" → chapter 5 verse 2)
                        if let Some((second_num, rest_after_second)) =
                            parse_number_and_rest(rest_trimmed)
                        {
                            if rest_after_second.trim().is_empty() {
                                log::info!("[READING] Two numbers '{first_num} {second_num}' interpreted as chapter:verse (context: ExpectingChapter)");
                                self.bare_number_context = BareNumberContext::None;
                                return Some(ChapterChange {
                                    book_number: self.book_number,
                                    book_name: self.book_name.clone(),
                                    new_chapter: first_num,
                                    start_verse: Some(second_num),
                                    emit_start_verse: true,
                                });
                            }
                        }
                        // Single number - just chapter
                        else if rest_trimmed.is_empty() {
                            log::info!("[READING] Bare number {first_num} interpreted as chapter (context: ExpectingChapter)");
                            self.bare_number_context = BareNumberContext::ExpectingVerse;
                            return Some(ChapterChange {
                                book_number: self.book_number,
                                book_name: self.book_name.clone(),
                                new_chapter: first_num,
                                start_verse: Some(1),
                                emit_start_verse: false,
                            });
                        }
                    }
                    BareNumberContext::ExpectingVerse => {
                        // Single number only when expecting verse
                        if rest_trimmed.is_empty() {
                            log::info!("[READING] Bare number {first_num} interpreted as verse (context: ExpectingVerse)");
                            self.bare_number_context = BareNumberContext::None;
                            // Return None to let check_verse_number_reference handle it
                            return None;
                        }
                    }
                    BareNumberContext::None => {}
                }
            }
        }

        // "next chapter"
        if trimmed == "next chapter" || trimmed == "next chapter." {
            let new_chapter = self.chapter + 1;
            log::info!("[READING] 'Next chapter' command detected → chapter {new_chapter}");
            return Some(ChapterChange {
                book_number: self.book_number,
                book_name: self.book_name.clone(),
                new_chapter,
                start_verse: None,
                emit_start_verse: true,
            });
        }

        // "previous chapter"
        if trimmed == "previous chapter" || trimmed == "previous chapter." {
            if self.chapter > 1 {
                let new_chapter = self.chapter - 1;
                log::info!("[READING] 'Previous chapter' command detected → chapter {new_chapter}");
                return Some(ChapterChange {
                    book_number: self.book_number,
                    book_name: self.book_name.clone(),
                    new_chapter,
                    start_verse: None,
                    emit_start_verse: true,
                });
            }
            return None;
        }

        if (trimmed.contains("same chapter") || trimmed.contains("this chapter"))
            && trimmed.contains("verse ")
        {
            if let Some(verse) = extract_verse_number(trimmed) {
                log::info!("[READING] Same chapter verse command detected: verse {verse}");
                self.bare_number_context = BareNumberContext::None;
                return Some(ChapterChange {
                    book_number: self.book_number,
                    book_name: self.book_name.clone(),
                    new_chapter: self.chapter,
                    start_verse: Some(verse),
                    emit_start_verse: true,
                });
            }
        }

        // Leaving the current chapter requires the current book name in the
        // same utterance. Bare "chapter 4 verse 8", "4 verse 8", or a leading
        // "one … verse 15" must not retarget reading. A different book is a
        // new citation for the direct detector, not in-book navigation.
        if utterance_names_other_book(trimmed, self.book_number) {
            return None;
        }
        let navigation_text = suffix_after_current_book(trimmed, self.book_number)?;

        // "chapter N" or "chapter N verse M" or "N verse M" after the book name
        log::debug!("[READING] Attempting to extract chapter and verse from: {navigation_text:?}");
        let (chapter_num, verse_num) = extract_chapter_and_verse(navigation_text)?;
        log::debug!("[READING] Extracted: chapter={chapter_num}, verse={verse_num:?}");

        // Ignore same-chapter references that restate or trail the current
        // cursor. STT partial/final windows often replay the original
        // "chapter N verse M" intro after reading mode has already advanced.
        if chapter_num == self.chapter {
            if let Some(verse) = verse_num {
                let current_verse = self.current_verse();
                let is_explicit_backward_navigation =
                    trimmed.contains("go back") || trimmed.contains("back to");
                if current_verse == Some(verse)
                    || (current_verse.is_some_and(|current| verse < current)
                        && !is_explicit_backward_navigation)
                {
                    log::debug!("[READING] Ignoring current chapter {chapter_num} verse {verse}");
                    self.bare_number_context = BareNumberContext::None;
                    return None;
                }
            } else {
                log::debug!("[READING] Ignoring same chapter {chapter_num} without specific verse");
                return None;
            }
        }

        if let Some(verse) = verse_num {
            log::info!(
                "[READING] Chapter change command detected: chapter {chapter_num} verse {verse}"
            );
            // Reset context after full chapter:verse navigation
            self.bare_number_context = BareNumberContext::None;
        } else {
            log::info!("[READING] Chapter change command detected: chapter {chapter_num}");
            // After navigating to a chapter, expect verse next
            self.bare_number_context = BareNumberContext::ExpectingVerse;
        }

        Some(ChapterChange {
            book_number: self.book_number,
            book_name: self.book_name.clone(),
            new_chapter: chapter_num,
            start_verse: verse_num,
            emit_start_verse: verse_num.is_some(),
        })
    }

    /// Process a transcript fragment and check if the reader has advanced.
    ///
    /// Returns `Some(ReadingAdvance)` if the reader has moved to a new verse.
    /// Returns `None` if still on the current verse or no match found.
    ///
    /// Automatically deactivates after timeout.
    pub fn check_transcript(&mut self, text: &str) -> Option<ReadingAdvance> {
        if !self.active || self.verses.is_empty() {
            return None;
        }

        // Check timeout — but don't clear verses, just pause.
        // This allows "verse N" references to re-activate.
        if self.last_match_time.elapsed().as_millis() > READING_MODE_TIMEOUT_MS && self.active {
            log::info!("[READING] Timeout — pausing (toggle still on, verses retained)");
            self.pause();
        }

        // Check for explicit verse number references like "verse three", "verse 4".
        // This works even when paused (timed out) — it re-activates reading mode.
        if !self.verses.is_empty() {
            if let Some(advance) = self.check_verse_number_reference(text) {
                self.active = true; // Re-activate if paused
                return Some(advance);
            }
        }

        if !self.active {
            return None;
        }

        // Accumulate text for multi-fragment matching
        if !self.accumulated_text.is_empty() {
            self.accumulated_text.push(' ');
        }
        self.accumulated_text.push_str(text);

        // [DIAG] Watch accumulated_text growth — only cleared on advance_to,
        // so if matches never trigger, this grows until timeout pauses us.
        if self.accumulated_text.len() > 4096 {
            log::warn!(
                "[READING] accumulated_text large: {} bytes (no advance since last clear)",
                self.accumulated_text.len(),
            );
        }

        let transcript_words = text_to_word_set(&self.accumulated_text);

        // Check current verse
        if let Some(current) = self.verses.get(self.current_index) {
            let overlap = word_overlap(&transcript_words, &current.words, current.word_count);
            if overlap >= MIN_WORD_OVERLAP {
                // Matched current verse — now check if we should advance to next
                let next_idx = self.current_index + 1;
                if next_idx < self.verses.len() {
                    let next = &self.verses[next_idx];
                    let next_overlap =
                        word_overlap(&transcript_words, &next.words, next.word_count);

                    // If transcript also matches next verse, advance
                    if next_overlap >= MIN_WORD_OVERLAP {
                        return self.advance_to(next_idx);
                    }
                }

                // Still on current verse: reset the match timer and clear the
                // buffer. Otherwise minutes of commentary that re-mentions this
                // verse's words keep accumulating and eventually cross the next
                // verse's overlap threshold, auto-advancing to a verse that was
                // never read.
                self.last_match_time = Instant::now();
                self.accumulated_text.clear();
                self.reset_out_of_scope_streak();
                return None;
            }
        }

        // Check next verse (speaker may have moved ahead without us catching current)
        let next_idx = self.current_index + 1;
        if next_idx < self.verses.len() {
            let next = &self.verses[next_idx];
            let overlap = word_overlap(&transcript_words, &next.words, next.word_count);
            if overlap >= MIN_WORD_OVERLAP {
                return self.advance_to(next_idx);
            }
        }

        // Check verse after next (speaker may have skipped one)
        let skip_idx = self.current_index + 2;
        if skip_idx < self.verses.len() {
            let skip = &self.verses[skip_idx];
            let overlap = word_overlap(&transcript_words, &skip.words, skip.word_count);
            if overlap >= MIN_WORD_OVERLAP {
                return self.advance_to(skip_idx);
            }
        }

        None
    }

    /// Check if the transcript contains a verse navigation command:
    /// - "verse three", "verse 4" → jump to that verse
    /// - "next" / "next verse" / "let's go to the next verse" → advance by 1
    /// - "previous verse" / "go back" / "let's go to the previous verse" → go back by 1
    /// - Bare numbers when context is `ExpectingVerse`
    fn check_verse_number_reference(&mut self, text: &str) -> Option<ReadingAdvance> {
        // Strip English fillers first: a stray "um." would otherwise parse as
        // the bare number 1 and jump the reading cursor back to verse 1.
        let normalized =
            crate::direct::parser::strip_english_filler_words(&normalize_command_text(text));
        let trimmed = normalized.as_str();

        // If we're expecting a verse number and this is a bare number, interpret it as verse
        if self.bare_number_context == BareNumberContext::ExpectingVerse {
            if let Some((num, rest)) = parse_number_and_rest(trimmed) {
                if rest.trim().is_empty() {
                    log::info!("[READING] Bare number {num} interpreted as verse (context: ExpectingVerse)");
                    self.bare_number_context = BareNumberContext::None;
                    // Navigate to this verse
                    for (idx, v) in self.verses.iter().enumerate() {
                        if v.verse_number == num {
                            return self.advance_to(idx);
                        }
                    }
                    log::warn!("[READING] Verse {num} not found in loaded verses");
                    return None;
                }
            }
        }

        if is_next_verse_command(trimmed) {
            let next_idx = self.current_index + 1;
            if next_idx < self.verses.len() {
                log::info!("[READING] 'Next' command detected");
                return self.advance_to(next_idx);
            }
            return None;
        }

        if is_previous_verse_command(trimmed) {
            if self.current_index > 0 {
                let prev_idx = self.current_index - 1;
                log::info!("[READING] 'Previous' command detected");
                return self.advance_to(prev_idx);
            }
            return None;
        }

        // Strip Deepgram stutters: "verse verse four" → "verse four"
        let cleaned = trimmed
            .replace("verse verse ", "verse ")
            .replace("verses verses ", "verses ");

        // Try to extract a verse number from patterns like "verse N", "verse N."
        let verse_num = extract_verse_number(&cleaned)?;

        // Find this verse number in our loaded verses (allow forward AND backward)
        for (idx, v) in self.verses.iter().enumerate() {
            if v.verse_number == verse_num {
                log::info!("[READING] Verse number reference detected: verse {verse_num}");
                return self.advance_to(idx);
            }
        }

        None
    }

    /// Advance to a new verse index and return the advance event.
    fn advance_to(&mut self, index: usize) -> Option<ReadingAdvance> {
        let verse = self.verses.get(index)?;
        let verse_number = verse.verse_number;
        let verse_text = verse.text.clone();

        self.current_index = index;
        self.last_match_time = Instant::now();
        self.accumulated_text.clear();
        self.reset_out_of_scope_streak();

        let reference = format!("{} {}:{verse_number}", self.book_name, self.chapter);
        log::info!("[READING] Advanced to: {reference}");

        Some(ReadingAdvance {
            book_number: self.book_number,
            book_name: self.book_name.clone(),
            chapter: self.chapter,
            verse: verse_number,
            verse_text,
            reference,
            confidence: 1.0, // We'll refine this later
        })
    }
}

impl Default for ReadingMode {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_command_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut previous_was_space = true;

    for ch in text.to_lowercase().chars() {
        if ch.is_alphanumeric() {
            normalized.push(ch);
            previous_was_space = false;
        } else if ch != '\'' && !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    normalized.trim().to_string()
}

fn contains_command_phrase(command: &str, phrase: &str) -> bool {
    command == phrase
        || command.starts_with(&format!("{phrase} "))
        || command.ends_with(&format!(" {phrase}"))
        || command.contains(&format!(" {phrase} "))
}

fn is_next_verse_command(command: &str) -> bool {
    matches!(
        command,
        "next" | "next verse" | "please next verse" | "next verse please"
    ) || [
        "go to next verse",
        "go to the next verse",
        "lets go to next verse",
        "lets go to the next verse",
        "let us go to next verse",
        "let us go to the next verse",
        "advance to next verse",
        "advance to the next verse",
        "move to next verse",
        "move to the next verse",
        "bring up next verse",
        "bring up the next verse",
    ]
    .iter()
    .any(|phrase| contains_command_phrase(command, phrase))
}

fn is_previous_verse_command(command: &str) -> bool {
    matches!(
        command,
        "previous"
            | "previous verse"
            | "prev verse"
            | "please previous verse"
            | "previous verse please"
            | "go back"
            | "go back please"
    ) || [
        "go to previous verse",
        "go to the previous verse",
        "lets go to previous verse",
        "lets go to the previous verse",
        "let us go to previous verse",
        "let us go to the previous verse",
        "go back a verse",
        "go back one verse",
        "back one verse",
        "back a verse",
        "move to previous verse",
        "move to the previous verse",
        "bring up previous verse",
        "bring up the previous verse",
    ]
    .iter()
    .any(|phrase| contains_command_phrase(command, phrase))
}

/// True when a transcript already reads as a complete verse-navigation
/// command ("let's go to the next verse", "go back to the previous verse"),
/// so the STT router can dispatch it on a *partial* transcript instead of
/// waiting out the endpointing pause before the final.
///
/// Deliberately stricter than [`crate::is_voice_command_utterance`]: the
/// utterance must contain the word "verse", so bare directionals ("next",
/// "go back") — common prefixes of ordinary speech — never fire early.
/// Chapter navigation stays on the final path for the same reason.
pub fn is_complete_verse_navigation_command(text: &str) -> bool {
    let normalized =
        crate::direct::parser::strip_english_filler_words(&normalize_command_text(text));
    let trimmed = normalized.as_str();
    if !trimmed.contains("verse") {
        return false;
    }
    is_next_verse_command(trimmed)
        || is_previous_verse_command(trimmed)
        || crate::direct::detector::PREVIOUS_VERSE_PHRASES
            .iter()
            .any(|phrase| contains_command_phrase(trimmed, phrase))
}

/// Extract a verse number from text containing "verse N" anywhere.
///
/// Matches phrases like "let's go to verse five", "give me verse 4",
/// "verse three", or whole-utterance bare SINGLE numbers like "3." or "five".
///
/// Returns None for two-number patterns like "5 2" or "five two" which
/// should be handled as "chapter 5 verse 2" by `check_chapter_command`.
fn extract_verse_number(text: &str) -> Option<i32> {
    // Find "verse N" or "verses N" anywhere in the text
    for keyword in &["verse ", "verses "] {
        let mut search_start = 0;
        while let Some(relative_pos) = text[search_start..].find(keyword) {
            let pos = search_start + relative_pos;
            if !has_reference_context_before_verse(text, pos) {
                let rest = &text[pos + keyword.len()..];
                if let Some(verse) = parse_number_token(rest) {
                    return Some(verse);
                }
            }
            search_start = pos + keyword.len();
        }
    }

    // Keep "first N" as a prefix-only match
    if let Some(rest) = text.strip_prefix("first ") {
        return parse_number_token(rest);
    }

    // Check for two-number pattern: "5 2" or "five two" → should be chapter:verse
    // Don't treat this as a bare verse number
    if let Some((first_num, rest_after_first)) = parse_number_and_rest(text) {
        let rest_trimmed = rest_after_first.trim_start();
        // If there's another number after the first, this is likely "chapter verse"
        if let Some((_second_num, _)) = parse_number_and_rest(rest_trimmed) {
            log::debug!(
                "[READING] Found two numbers '{} {}...' - deferring to chapter command handler",
                first_num,
                rest_trimmed.split_whitespace().next().unwrap_or("")
            );
            return None; // Let check_chapter_command handle it
        }
        if is_only_command_punctuation(rest_trimmed) {
            return Some(first_num);
        }
    }

    None
}

fn is_only_command_punctuation(text: &str) -> bool {
    text.trim()
        .chars()
        .all(|ch| matches!(ch, '.' | ',' | '?' | '!'))
}

fn has_reference_context_before_verse(text: &str, verse_pos: usize) -> bool {
    let prefix = text[..verse_pos].trim();
    !prefix.is_empty()
        && prefix
            .split_whitespace()
            .rev()
            .take(3)
            .any(|token| parse_number_token(token).is_some())
}

/// Parse a number (digit or spoken word) from the start of `text`.
fn parse_number_token(text: &str) -> Option<i32> {
    let trimmed = text.trim_end_matches(['.', ',', '?', '!']);
    // Try digit
    let token: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .collect();
    if let Ok(n) = token.parse::<i32>() {
        if n > 0 && n <= 176 {
            return Some(n);
        }
    }
    // Try spoken number
    let word: String = trimmed.chars().take_while(|c| c.is_alphabetic()).collect();
    if !word.is_empty() {
        if let Some(n) = parse_spoken_number(&word) {
            if n > 0 && n <= 176 {
                return Some(n);
            }
        }
    }
    None
}

/// Extract both chapter and optional verse from patterns like:
/// - "chapter 3" → (3, None)
/// - "chapter 3 verse 5" → (3, Some(5))
/// - "3 verse 5" → (3, Some(5))
fn book_needle_end(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let haystack_lower = haystack.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut last_end = None;
    let mut search_from = 0;
    while search_from < haystack_lower.len() {
        let Some(rel) = haystack_lower[search_from..].find(&needle_lower) else {
            break;
        };
        let start = search_from + rel;
        let end = start + needle_lower.len();
        let before_ok = start == 0 || !haystack_lower.as_bytes()[start - 1].is_ascii_alphanumeric();
        let after_ok =
            end == haystack_lower.len() || !haystack_lower.as_bytes()[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            last_end = Some(end);
        }
        search_from = start + 1;
    }
    last_end
}

fn last_end_for_book(text: &str, book_number: i32) -> Option<usize> {
    let book = BOOKS
        .iter()
        .find(|candidate| candidate.number == book_number)?;
    let mut last = book_needle_end(text, book.name);
    for alias in book.aliases {
        if let Some(end) = book_needle_end(text, alias) {
            last = Some(last.map_or(end, |existing| existing.max(end)));
        }
    }
    last
}

fn utterance_names_other_book(text: &str, current_book_number: i32) -> bool {
    BOOKS.iter().any(|book| {
        book.number != current_book_number && last_end_for_book(text, book.number).is_some()
    })
}

fn suffix_after_current_book(text: &str, current_book_number: i32) -> Option<&str> {
    let end = last_end_for_book(text, current_book_number)?;
    Some(text.get(end..).unwrap_or(""))
}

fn extract_chapter_and_verse(text: &str) -> Option<(i32, Option<i32>)> {
    // First try to find "chapter N"
    if let Some(pos) = text.find("chapter ") {
        let rest = &text[pos + "chapter ".len()..];
        log::debug!("[EXTRACT] Found 'chapter', rest: {rest:?}");
        if let Some((chapter, rest_after_chapter)) = parse_number_and_rest(rest) {
            log::debug!(
                "[EXTRACT] Parsed chapter={chapter}, rest_after_chapter: {rest_after_chapter:?}"
            );
            // Now check if there's "verse M" after the chapter
            if let Some(verse_pos) = rest_after_chapter.find("verse ") {
                let verse_rest = &rest_after_chapter[verse_pos + "verse ".len()..];
                log::debug!(
                    "[EXTRACT] Found 'verse' at pos {verse_pos}, verse_rest: {verse_rest:?}"
                );
                if let Some((verse, _)) = parse_number_and_rest(verse_rest) {
                    log::debug!("[EXTRACT] Parsed verse={verse}");
                    return Some((chapter, Some(verse)));
                }
                log::debug!("[EXTRACT] Failed to parse verse number from: {verse_rest:?}");
            } else {
                log::debug!("[EXTRACT] No 'verse ' keyword found in: {rest_after_chapter:?}");
            }
            return Some((chapter, None));
        }
    }

    // Also try pattern without "chapter" keyword: "3 verse 5"
    if let Some((chapter, rest_after_number)) = parse_number_and_rest(text) {
        log::debug!(
            "[EXTRACT] Number-first pattern: chapter={chapter}, rest: {rest_after_number:?}"
        );
        if let Some(verse_pos) = rest_after_number.find("verse ") {
            let verse_rest = &rest_after_number[verse_pos + "verse ".len()..];
            if let Some((verse, _)) = parse_number_and_rest(verse_rest) {
                return Some((chapter, Some(verse)));
            }
        }
    }

    None
}

/// Parse a number at the start of text and return both the number and remaining text
fn parse_number_and_rest(text: &str) -> Option<(i32, &str)> {
    let trimmed = text.trim_start();

    // Try digit number first
    if let Ok(captures) = trimmed
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse::<i32>()
    {
        let consumed_len = trimmed.chars().take_while(char::is_ascii_digit).count();
        return Some((captures, &trimmed[consumed_len..]));
    }

    // Try spoken number
    let first_word = trimmed.split_whitespace().next()?;
    // Strip punctuation from the word before parsing as spoken number
    let clean_word: String = first_word.chars().filter(|c| c.is_alphabetic()).collect();
    if let Some(num) = parse_spoken_number(&clean_word) {
        let rest = &trimmed[first_word.len()..];

        // Check for compound spoken numbers like "thirty two"
        if num >= 20 && num % 10 == 0 {
            let rest_trimmed = rest.trim_start();
            if let Some(second_word) = rest_trimmed.split_whitespace().next() {
                let clean_second: String =
                    second_word.chars().filter(|c| c.is_alphabetic()).collect();
                if let Some(ones) = parse_spoken_number(&clean_second) {
                    if (1..=9).contains(&ones) {
                        let combined = num + ones;
                        let rest_after_second = &rest_trimmed[second_word.len()..];
                        return Some((combined, rest_after_second));
                    }
                }
            }
        }

        return Some((num, rest));
    }

    None
}

/// Convert text to a set of lowercase words (stripped of punctuation).
fn text_to_word_set(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .map(|w| {
            w.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .collect::<String>()
        })
        .filter(|w| w.len() >= 2) // Skip single-char words
        .collect()
}

/// Calculate what fraction of `verse_words` appear in `transcript_words`.
fn word_overlap(
    transcript_words: &HashSet<String>,
    verse_words: &HashSet<String>,
    verse_word_count: usize,
) -> f64 {
    if verse_word_count == 0 {
        return 0.0;
    }
    let matches = verse_words.intersection(transcript_words).count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "word counts are small enough for f64 precision"
    )]
    {
        matches as f64 / verse_word_count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_verse_navigation_command_matches_next_and_previous_phrases() {
        assert!(is_complete_verse_navigation_command(
            "Let's go to the next verse"
        ));
        assert!(is_complete_verse_navigation_command("next verse"));
        assert!(is_complete_verse_navigation_command(
            "Go back to the previous verse."
        ));
        assert!(is_complete_verse_navigation_command(
            "previous verse please"
        ));
        assert!(is_complete_verse_navigation_command("last verse again"));
    }

    #[test]
    fn complete_verse_navigation_command_rejects_prose_references_and_bare_directionals() {
        // Narration about a verse is not a command.
        assert!(!is_complete_verse_navigation_command(
            "the next verse explains the same idea"
        ));
        // References belong to the reference fast-path, not the command path.
        assert!(!is_complete_verse_navigation_command(
            "genesis chapter 3 verse 15"
        ));
        // Bare directionals stay final-only: as partials they are prefixes of
        // ordinary speech ("next Sunday...", "go back with me to...").
        assert!(!is_complete_verse_navigation_command("next"));
        assert!(!is_complete_verse_navigation_command("go back"));
        assert!(!is_complete_verse_navigation_command("next chapter"));
    }

    fn sample_verses() -> Vec<(i32, String)> {
        vec![
            (28, "For it seemed good to the Holy Ghost, and to us, to lay upon you no greater burden than these necessary things;".to_string()),
            (29, "That ye abstain from meats offered to idols, and from blood, and from things strangled, and from fornication: from which if ye keep yourselves, ye shall do well. Fare ye well.".to_string()),
            (30, "So when they were dismissed, they came to Antioch: and when they had gathered the multitude together, they delivered the epistle:".to_string()),
            (31, "Which when they had read, they rejoiced for the consolation.".to_string()),
        ]
    }

    #[test]
    fn test_starts_inactive() {
        let rm = ReadingMode::new();
        assert!(!rm.is_active());
        assert!(rm.current_verse().is_none());
    }

    #[test]
    fn test_start_activates() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 28, sample_verses());
        assert!(rm.is_active());
        assert_eq!(rm.current_verse(), Some(28));
    }

    #[test]
    fn test_advance_on_next_verse_match() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 28, sample_verses());

        // Feed text matching verse 28
        let r = rm.check_transcript("it seemed good to the Holy Ghost and to us to lay upon you no greater burden than these necessary things");
        // Still on verse 28 — no advance yet
        assert!(r.is_none());

        // Feed text matching verse 29
        let r = rm.check_transcript("that ye abstain from meats offered to idols and from blood and from things strangled and from fornication");
        assert!(r.is_some());
        let advance = r.unwrap();
        assert_eq!(advance.verse, 29);
        assert_eq!(advance.reference, "Acts 15:29");
    }

    #[test]
    fn commentary_repeating_current_verse_does_not_auto_advance() {
        let mut rm = ReadingMode::new();
        let verses = vec![
            (1, "alpha beta gamma delta".to_string()),
            (
                2,
                "one two three four five six seven eight nine ten".to_string(),
            ),
        ];
        rm.start(1, "Test", 1, 1, verses);

        // Reads verse 1 and drops two stray verse-2 words — stays on verse 1.
        let r = rm.check_transcript("alpha beta gamma delta one two");
        assert!(r.is_none());

        // More commentary with two more verse-2 words. The earlier "one two"
        // must not linger and combine with "three four" to cross verse 2's 40%
        // overlap and falsely advance.
        let r = rm.check_transcript("three four");
        assert!(r.is_none());
        assert_eq!(rm.current_verse(), Some(1));
    }

    #[test]
    fn test_pause_retains_verses_for_resume() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 28, sample_verses());
        assert!(rm.is_active());
        rm.pause();
        assert!(!rm.is_active());
        assert!(rm.has_verses());
    }

    #[test]
    fn pause_clears_accumulated_text_so_stale_words_cannot_fake_a_match() {
        let mut rm = ReadingMode::new();
        let verses = vec![
            (1, "alpha beta gamma delta".to_string()),
            (
                2,
                "one two three four five six seven eight nine ten".to_string(),
            ),
        ];
        rm.start(1, "Test", 1, 1, verses);

        // Partial verse-2 words accumulate without matching anything.
        assert!(rm.check_transcript("one two three").is_none());

        // Speaker moves to a different passage — reading scope pauses.
        rm.pause();
        rm.resume();

        // Post-resume fragment alone is below verse 2's overlap threshold;
        // it must not combine with the pre-pause "one two three" to advance.
        assert!(rm.check_transcript("four five").is_none());
        assert_eq!(rm.current_verse(), Some(1));
    }

    #[test]
    fn out_of_scope_streak_counts_same_chapter_and_restarts_on_scope_change() {
        let mut rm = ReadingMode::new();
        rm.start(44, "Acts", 15, 28, sample_verses());

        assert_eq!(rm.note_out_of_scope_hit(19, 23), 1);
        assert_eq!(rm.note_out_of_scope_hit(19, 23), 2);
        assert_eq!(rm.note_out_of_scope_hit(19, 23), 3);
        // A hit on the same book but a different chapter restarts the run.
        assert_eq!(rm.note_out_of_scope_hit(19, 24), 1);
        // A hit on a different book also restarts the run.
        assert_eq!(rm.note_out_of_scope_hit(23, 53), 1);
    }

    #[test]
    fn out_of_scope_streak_resets_when_a_scoped_verse_matches() {
        let mut rm = ReadingMode::new();
        let verses = vec![
            (
                1,
                "for it seemed good to the holy ghost and to us".to_string(),
            ),
            (
                2,
                "one two three four five six seven eight nine ten".to_string(),
            ),
        ];
        rm.start(44, "Acts", 15, 1, verses);

        assert_eq!(rm.note_out_of_scope_hit(19, 23), 1);
        assert_eq!(rm.note_out_of_scope_hit(19, 23), 2);

        // Speaker is still reading the anchored chapter — verse 1 matches.
        rm.check_transcript("for it seemed good to the holy ghost and to us");

        // Echo streak starts over.
        assert_eq!(rm.note_out_of_scope_hit(19, 23), 1);
    }

    #[test]
    fn test_deactivate() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 28, sample_verses());
        assert!(rm.is_active());
        rm.deactivate();
        assert!(!rm.is_active());
    }

    #[test]
    fn test_no_match_returns_none() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 28, sample_verses());

        let r = rm.check_transcript("the weather is nice today and I like coffee");
        assert!(r.is_none());
    }

    #[test]
    fn test_word_overlap_function() {
        let transcript = text_to_word_set("for it seemed good to the holy ghost");
        let verse = text_to_word_set("For it seemed good to the Holy Ghost, and to us, to lay upon you no greater burden than these necessary things;");
        let count = verse.len();
        let overlap = word_overlap(&transcript, &verse, count);
        assert!(overlap > 0.3); // At least some overlap
    }

    // --- Bug fix: extract_verse_number with prefix phrases ---

    #[test]
    fn test_extract_verse_number_with_prefix() {
        assert_eq!(extract_verse_number("let's go to verse five"), Some(5));
        assert_eq!(extract_verse_number("give me verse nine"), Some(9));
        assert_eq!(extract_verse_number("go to verse 4"), Some(4));
        assert_eq!(extract_verse_number("let's go to verse twenty"), Some(20));
    }

    #[test]
    fn test_extract_verse_number_direct() {
        assert_eq!(extract_verse_number("verse eight"), Some(8));
        assert_eq!(extract_verse_number("verse eight,"), Some(8));
        assert_eq!(extract_verse_number("verse 3"), Some(3));
        assert_eq!(extract_verse_number("verse three."), Some(3));
    }

    #[test]
    fn test_extract_verse_number_bare() {
        assert_eq!(extract_verse_number("3."), Some(3));
        assert_eq!(extract_verse_number("12"), Some(12));
        assert_eq!(extract_verse_number("2 books right"), None);
        assert_eq!(
            extract_verse_number("3 books a third book was opened"),
            None
        );
    }

    #[test]
    fn bare_um_filler_does_not_jump_to_verse_one() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(24, "Jeremiah", 29, 7, verses);

        // A lone hesitation must not parse as the bare number 1.
        assert!(rm.check_transcript("um.").is_none());
        assert_eq!(rm.current_verse(), Some(7));

        // Nor may it satisfy an ExpectingChapter/ExpectingVerse context.
        let _ = rm.check_chapter_command("chapter");
        assert_eq!(rm.check_chapter_command("um"), None);
    }

    #[test]
    fn verse_command_with_um_filler_still_navigates() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(24, "Jeremiah", 29, 7, verses);

        let advance = rm
            .check_transcript("verse um nine")
            .expect("filler inside verse command should still navigate");
        assert_eq!(advance.verse, 9);
    }

    #[test]
    fn test_extract_verse_number_no_match() {
        assert_eq!(extract_verse_number("hello world"), None);
        assert_eq!(extract_verse_number("the weather is nice"), None);
    }

    #[test]
    fn extract_verse_number_ignores_complete_reference_context() {
        assert_eq!(extract_verse_number("genesis 3 verse 9"), None);
        assert_eq!(extract_verse_number("chapter 3 verse 9"), None);
    }

    #[test]
    fn reading_mode_does_not_apply_external_reference_to_current_context() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> = (1..=10)
            .map(|i| (i, format!("sample passage words number {i}")))
            .collect();
        rm.start(43, "John", 1, 1, verses);

        let result = rm.check_transcript("Genesis 3 verse 9");

        assert_eq!(result, None);
        assert_eq!(rm.current_verse(), Some(1));
    }

    #[test]
    fn later_verse_command_wins_after_full_reference_in_same_fragment() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> = (1..=22)
            .map(|i| (i, format!("alpha beta gamma delta marker{i}")))
            .collect();
        rm.start(66, "Revelation", 1, 9, verses);

        let transcript = concat!(
            "Revelation chapter 1 verse 9 I John who also am your brother and companion ",
            "in tribulation was on the island called Patmos. Verse 10 I was in the spirit ",
            "on the Lord's day."
        );
        let advance = rm.check_transcript(transcript);

        assert_eq!(advance.map(|result| result.verse), Some(10));
    }

    #[test]
    fn reading_mode_ignores_book_count_numbers_after_revelation_reference() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> = (1..=22)
            .map(|i| (i, format!("Revelation 20 verse {i}.")))
            .collect();
        rm.start(66, "Revelation", 20, 12, verses);

        let two_books =
            rm.check_transcript("so we see there again that there are at least 2 books right");
        let three_books = rm.check_transcript(
            "so far and then it says and another book so that is now we have at least 3 books",
        );

        assert_eq!(two_books, None);
        assert_eq!(three_books, None);
        assert_eq!(rm.current_verse(), Some(12));
    }

    // --- Bug fix: all chapter verses loaded for backward navigation ---

    #[test]
    fn test_backward_verse_navigation() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> = (1..=10)
            .map(|i| (i, format!("Verse {i} text content here.")))
            .collect();

        rm.start(1, "Genesis", 5, 6, verses);
        assert_eq!(rm.current_verse(), Some(6));

        // Navigate backward to verse 3
        let r = rm.check_transcript("verse three");
        assert!(r.is_some());
        assert_eq!(r.unwrap().verse, 3);
    }

    #[test]
    fn next_verse_command_accepts_natural_go_to_phrase() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 29, sample_verses());

        let advance = rm
            .check_transcript("let's go to the next verse")
            .expect("natural next verse command should advance");

        assert_eq!(advance.verse, 30);
    }

    #[test]
    fn previous_verse_command_accepts_natural_go_to_phrase() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 29, sample_verses());

        let advance = rm
            .check_transcript("lets go to the previous verse")
            .expect("natural previous verse command should go back");

        assert_eq!(advance.verse, 28);
    }

    #[test]
    fn next_verse_command_does_not_fire_for_plain_sermon_sentence() {
        let mut rm = ReadingMode::new();

        rm.start(44, "Acts", 15, 29, sample_verses());

        let result = rm.check_transcript("the next verse explains the same idea");

        assert_eq!(result, None);
    }

    #[test]
    fn test_start_positions_cursor_at_start_verse() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=31).map(|i| (i, format!("Verse {i} text."))).collect();

        rm.start(44, "Acts", 15, 28, verses);
        assert_eq!(rm.current_verse(), Some(28));
    }

    // --- Bug fix: chapter navigation ---

    #[test]
    fn test_chapter_command_detected() {
        let mut rm = ReadingMode::new();
        rm.start(
            1,
            "Genesis",
            5,
            1,
            vec![(1, "In the beginning.".to_string())],
        );

        let result = rm.check_chapter_command("let's go to Genesis chapter seven");
        assert_eq!(
            result,
            Some(ChapterChange {
                book_number: 1,
                book_name: "Genesis".to_string(),
                new_chapter: 7,
                start_verse: None,
                emit_start_verse: false,
            })
        );
    }

    #[test]
    fn test_chapter_command_with_digit() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 5, 1, vec![(1, "Text.".to_string())]);

        let result = rm.check_chapter_command("let's go to Genesis chapter 8");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 8);
        assert!(!change.emit_start_verse);
    }

    #[test]
    fn chapter_only_sermon_transition_does_not_emit_verse_one() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> = (1..=20)
            .map(|i| (i, format!("Revelation 20 verse {i}.")))
            .collect();
        rm.start(66, "Revelation", 20, 12, verses);

        let result =
            rm.check_chapter_command("let's now turn a few chapters back to Revelation chapter 13");

        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.book_name, "Revelation");
        assert_eq!(change.new_chapter, 13);
        assert_eq!(change.start_verse, None);
        assert!(!change.emit_start_verse);
    }

    #[test]
    fn test_chapter_command_same_chapter_ignored() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 5, 1, vec![(1, "Text.".to_string())]);

        let result = rm.check_chapter_command("chapter five");
        assert_eq!(result, None);
    }

    #[test]
    fn test_chapter_command_no_verses_ignored() {
        let mut rm = ReadingMode::new();
        let result = rm.check_chapter_command("chapter seven");
        assert_eq!(result, None);
    }

    #[test]
    fn test_next_chapter_command() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 5, 1, vec![(1, "Text.".to_string())]);

        let result = rm.check_chapter_command("next chapter");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 6);
        assert!(change.emit_start_verse);
    }

    #[test]
    fn test_previous_chapter_command() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 5, 1, vec![(1, "Text.".to_string())]);

        let result = rm.check_chapter_command("previous chapter");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 4);
        assert!(change.emit_start_verse);
    }

    #[test]
    fn test_previous_chapter_at_chapter_1() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 1, 1, vec![(1, "Text.".to_string())]);

        let result = rm.check_chapter_command("previous chapter");
        assert_eq!(result, None);
    }

    #[test]
    fn test_chapter_verse_command() {
        let mut rm = ReadingMode::new();
        rm.start(
            1,
            "Genesis",
            1,
            1,
            vec![(1, "In the beginning.".to_string())],
        );

        let result = rm.check_chapter_command("Genesis chapter 3 verse 5");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 3);
        assert_eq!(change.start_verse, Some(5));
        assert!(change.emit_start_verse);
    }

    #[test]
    fn same_chapter_verse_command_keeps_current_chapter_and_sets_verse() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(1, "Genesis", 5, 1, verses);

        let result = rm.check_chapter_command("in same chapter verse 7");

        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 5);
        assert_eq!(change.start_verse, Some(7));
        assert!(change.emit_start_verse);
    }

    #[test]
    fn same_chapter_verse_command_works_when_chapter_is_trailing_word() {
        // "verse 2 in the same chapter" ends in "chapter"; the trailing-keyword
        // guard must not swallow it as an "expecting a chapter number" command.
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(1, "Genesis", 5, 1, verses);

        let result = rm.check_chapter_command("verse 2 in the same chapter");

        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 5);
        assert_eq!(change.start_verse, Some(2));
        assert!(change.emit_start_verse);
    }

    #[test]
    fn current_chapter_verse_command_is_not_restarted() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=26).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(2, "Exodus", 20, 9, verses);

        let result = rm.check_chapter_command("Exodus chapter 20 verse 9");

        assert_eq!(result, None);
        assert_eq!(rm.current_verse(), Some(9));
    }

    #[test]
    fn stale_current_chapter_previous_verse_command_does_not_reanchor_backward() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=20).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(27, "Daniel", 7, 10, verses);

        let result = rm.check_chapter_command("Daniel chapter 7 verse 9");

        assert_eq!(result, None);
        assert_eq!(rm.current_verse(), Some(10));
    }

    #[test]
    fn explicit_backward_chapter_verse_command_can_reanchor() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=20).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(27, "Daniel", 7, 10, verses);

        let result = rm.check_chapter_command("go back to Daniel chapter 7 verse 9");

        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 7);
        assert_eq!(change.start_verse, Some(9));
    }

    #[test]
    fn test_number_verse_command() {
        let mut rm = ReadingMode::new();
        rm.start(
            1,
            "Genesis",
            1,
            1,
            vec![(1, "In the beginning.".to_string())],
        );

        let result = rm.check_chapter_command("Genesis 3 verse 5");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 3);
        assert_eq!(change.start_verse, Some(5));
        assert!(change.emit_start_verse);
    }

    #[test]
    fn test_spoken_chapter_verse_command() {
        let mut rm = ReadingMode::new();
        rm.start(
            1,
            "Genesis",
            1,
            1,
            vec![(1, "In the beginning.".to_string())],
        );

        let result = rm.check_chapter_command("Genesis chapter three verse five");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 3);
        assert_eq!(change.start_verse, Some(5));
        assert!(change.emit_start_verse);
    }

    #[test]
    fn test_extract_chapter_and_verse() {
        assert_eq!(extract_chapter_and_verse("chapter 3"), Some((3, None)));
        assert_eq!(
            extract_chapter_and_verse("chapter 3 verse 5"),
            Some((3, Some(5)))
        );
        assert_eq!(extract_chapter_and_verse("3 verse 5"), Some((3, Some(5))));
        assert_eq!(
            extract_chapter_and_verse("chapter three verse five"),
            Some((3, Some(5)))
        );
        assert_eq!(
            extract_chapter_and_verse("chapter three verse five."),
            Some((3, Some(5)))
        );
        assert_eq!(extract_chapter_and_verse("hello world"), None);
    }

    #[test]
    fn test_extract_verse_number_spoken() {
        // Single numbers should be treated as verses
        assert_eq!(extract_verse_number("five"), Some(5));
        assert_eq!(extract_verse_number("three"), Some(3));
        assert_eq!(extract_verse_number("five?"), Some(5));
        assert_eq!(extract_verse_number("verse five"), Some(5));
        assert_eq!(extract_verse_number("5"), Some(5));
        assert_eq!(extract_verse_number("three books"), None);

        // Two numbers should return None (defer to chapter command handler)
        assert_eq!(extract_verse_number("five two"), None);
        assert_eq!(extract_verse_number("5 2"), None);
        assert_eq!(extract_verse_number("three five"), None);
        assert_eq!(extract_verse_number("3 5"), None);
    }

    // --- Context-aware navigation tests ---

    #[test]
    fn test_bare_number_as_chapter_after_chapter_keyword() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(1, "Genesis", 1, 1, verses);

        // Say "chapter" - sets context to ExpectingChapter
        let result = rm.check_chapter_command("chapter");
        assert_eq!(result, None); // No navigation yet, just context set

        // Now say "5" - should be interpreted as chapter 5
        let result = rm.check_chapter_command("5");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 5);
        assert_eq!(change.start_verse, Some(1));
        assert!(!change.emit_start_verse);
    }

    #[test]
    fn test_bare_two_numbers_as_chapter_verse() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(1, "Genesis", 1, 1, verses);

        // Say "chapter" - sets context
        let _ = rm.check_chapter_command("chapter");

        // Say "5 2" - should be interpreted as chapter 5 verse 2
        let result = rm.check_chapter_command("5 2");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 5);
        assert_eq!(change.start_verse, Some(2));
        assert!(change.emit_start_verse);
    }

    #[test]
    fn test_spoken_numbers_in_context() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(1, "Genesis", 1, 1, verses);

        // Say "chapter" - sets context
        let _ = rm.check_chapter_command("chapter");

        // Say "five" - should be interpreted as chapter 5
        let result = rm.check_chapter_command("five");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 5);
        assert!(!change.emit_start_verse);
    }

    #[test]
    fn test_context_resets_after_full_reference() {
        let mut rm = ReadingMode::new();
        let verses: Vec<(i32, String)> =
            (1..=10).map(|i| (i, format!("Verse {i} text."))).collect();
        rm.start(1, "Genesis", 1, 1, verses);

        // Say "chapter" - sets context
        let _ = rm.check_chapter_command("chapter");

        // Say "chapter 3 verse 5" - full reference should reset context
        let result = rm.check_chapter_command("Genesis chapter 3 verse 5");
        assert!(result.is_some());
        let change = result.unwrap();
        assert_eq!(change.new_chapter, 3);
        assert_eq!(change.start_verse, Some(5));
        assert!(change.emit_start_verse);

        // Context should be reset to None after full reference
        // Next bare number should be handled by verse navigation
    }

    fn genesis_chapter_verses() -> Vec<(i32, String)> {
        (1..=26)
            .map(|i| (i, format!("Genesis verse {i} text.")))
            .collect()
    }

    #[test]
    fn testing_one_two_does_not_hijack_genesis_3_15_to_1_15() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 3, 15, genesis_chapter_verses());

        let change = rm.check_chapter_command("One , two and turn to Genesis three, verse 15");

        assert_eq!(change, None, "leading one/two must not become chapter 1");
        assert_eq!(rm.current_verse(), Some(15));
        assert_eq!(rm.current_chapter(), 3);
    }

    #[test]
    fn leaving_chapter_requires_book_name() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 3, 15, genesis_chapter_verses());

        assert_eq!(rm.check_chapter_command("chapter 4 verse 8"), None);
        assert_eq!(rm.check_chapter_command("4 verse 8"), None);

        let change = rm
            .check_chapter_command("Genesis four, verse eight")
            .expect("book name plus chapter:verse may leave the chapter");
        assert_eq!(change.new_chapter, 4);
        assert_eq!(change.start_verse, Some(8));
    }

    #[test]
    fn same_chapter_verse_command_still_advances() {
        let mut rm = ReadingMode::new();
        rm.start(1, "Genesis", 3, 15, genesis_chapter_verses());

        let advance = rm
            .check_transcript("verse 16")
            .expect("same-chapter verse N must still advance");
        assert_eq!(advance.chapter, 3);
        assert_eq!(advance.verse, 16);
    }
}
