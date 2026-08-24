use std::time::Instant;

/// Default timeout: flush the buffer if no new text arrives within 3 seconds.
const DEFAULT_FLUSH_TIMEOUT_MS: u128 = 3000;

/// Accumulates transcript fragments into complete sentences before
/// passing them to the detection pipeline.
///
/// Deepgram sends many small `is_final` chunks per utterance. Running
/// detection on each fragment produces worse results than running on
/// complete sentences. This buffer:
///
/// 1. Accumulates `is_final` text fragments.
/// 2. Flushes when sentence-ending (`.!?`) or clause-ending (`;:`) punctuation
///    is detected, so quoted verses with internal clause breaks (e.g. Psalm
///    23:1, "The Lord is my shepherd; I shall not want") reach detection
///    immediately instead of waiting for the timeout.
/// 3. Flushes on timeout if no punctuation arrives (fallback).
/// 4. Flushes on `speech_final` signal (Deepgram utterance boundary).
#[derive(Debug)]
pub struct SentenceBuffer {
    buffer: String,
    last_append: Option<Instant>,
    flush_timeout_ms: u128,
}

impl SentenceBuffer {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            last_append: None,
            flush_timeout_ms: DEFAULT_FLUSH_TIMEOUT_MS,
        }
    }

    /// Append a transcript fragment. Returns the flushed sentence if a
    /// sentence boundary was detected, or None if still accumulating.
    pub fn append(&mut self, text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        if !self.buffer.is_empty() {
            self.buffer.push(' ');
        }
        self.buffer.push_str(text);
        self.last_append = Some(Instant::now());

        // Check for sentence-ending (.!?) or clause-ending (;:) punctuation
        let trimmed = self.buffer.trim_end();
        if trimmed.ends_with(['.', '!', '?', ';', ':']) {
            return Some(self.flush());
        }

        None
    }

    /// Check if the buffer should be flushed due to timeout.
    /// Call this periodically (e.g., on each event loop iteration).
    /// Returns the buffered text if timeout exceeded, None otherwise.
    pub fn check_timeout(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        if let Some(last) = self.last_append {
            if last.elapsed().as_millis() >= self.flush_timeout_ms {
                return Some(self.flush());
            }
        }
        None
    }

    /// Force-flush the buffer (e.g., on `speech_final` signal).
    /// Returns the buffered text, or None if empty.
    pub fn force_flush(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        Some(self.flush())
    }

    /// Current accumulated character count. Used by tests as a growth gate.
    #[cfg(test)]
    pub(crate) fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns true if the buffer has accumulated text.
    pub fn has_content(&self) -> bool {
        !self.buffer.is_empty()
    }

    fn flush(&mut self) -> String {
        let text = self.buffer.trim().to_string();
        self.buffer.clear();
        self.last_append = None;
        text
    }
}

impl Default for SentenceBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentence_ending_flushes() {
        let mut buf = SentenceBuffer::new();
        assert!(buf.append("God so loved").is_none());
        assert!(buf.append("the world.").is_some());
    }

    #[test]
    fn test_accumulates_fragments() {
        let mut buf = SentenceBuffer::new();
        buf.append("God so");
        buf.append("loved the");
        let result = buf.append("world.");
        assert_eq!(result.unwrap(), "God so loved the world.");
    }

    #[test]
    fn test_force_flush() {
        let mut buf = SentenceBuffer::new();
        buf.append("God so loved the world");
        let result = buf.force_flush();
        assert_eq!(result.unwrap(), "God so loved the world");
        assert!(!buf.has_content());
    }

    #[test]
    fn test_empty_buffer() {
        let mut buf = SentenceBuffer::new();
        assert!(buf.force_flush().is_none());
        assert!(buf.check_timeout().is_none());
    }

    #[test]
    fn test_question_mark_flushes() {
        let mut buf = SentenceBuffer::new();
        let result = buf.append("What does John 3:16 say?");
        assert!(result.is_some());
    }

    #[test]
    fn test_semicolon_flushes() {
        // Psalm 23:1 is quoted with an internal semicolon and no terminal
        // punctuation; it must flush on the clause break, not wait for timeout.
        let mut buf = SentenceBuffer::new();
        let result = buf.append("The Lord is my shepherd;");
        assert_eq!(result.unwrap(), "The Lord is my shepherd;");
    }

    #[test]
    fn test_colon_flushes() {
        let mut buf = SentenceBuffer::new();
        assert!(buf.append("And the Lord said unto him:").is_some());
    }

    #[test]
    fn test_exclamation_flushes() {
        let mut buf = SentenceBuffer::new();
        buf.append("Praise the Lord");
        let result = buf.append("for He is good!");
        assert!(result.is_some());
    }

    #[test]
    fn performance_gate_unpunctuated_growth_flushes_on_boundary() {
        let mut buf = SentenceBuffer::new();
        let mut last_len = 0usize;
        for i in 0..40 {
            assert!(
                buf.append(&format!("word{i}")).is_none(),
                "unpunctuated fragments must accumulate"
            );
            let len = buf.buffered_len();
            assert!(len > last_len, "buffer must grow until a sentence ends");
            last_len = len;
        }
        let flushed = buf.append("amen.");
        assert!(flushed.is_some());
        assert_eq!(buf.buffered_len(), 0);
        assert!(last_len < 8_192, "40 short fragments must stay a small segment");
    }
}
