//! Real-time Bible verse detection for the `SabbathCue` application.
//!
//! Combines direct pattern matching and semantic vector search into a
//! unified pipeline that identifies Bible references in sermon transcripts.
//!
//! # Key types
//!
//! - [`DetectionPipeline`] — orchestrates all detection strategies
//! - [`DirectDetector`] — regex and Aho-Corasick pattern matching
//! - [`SemanticDetector`] — ONNX embedding and vector similarity search
//! - [`Detection`], [`VerseRef`] — detection results
//!
//! # Feature flags
//!
//! - `onnx` — enables ONNX Runtime for local embedding inference
//! - `vector-search` — enables HNSW vector index for similarity search

pub mod command_eval;
pub mod direct;
pub mod egw_quote;
pub mod error;
pub mod merger;
pub mod pipeline;
pub mod reading_mode;
pub mod semantic;
pub mod sentence_buffer;
pub mod types;

pub use egw_quote::{
    egw_quote_score, longest_shared_content_run, quote_has_negation_conflict, SharedRun,
    EGW_QUOTE_MAX_CONFIDENCE, EGW_QUOTE_RUN_AUTO_QUEUE, EGW_QUOTE_RUN_CUED_HINT,
    EGW_QUOTE_RUN_FIRE, EGW_RUN_MAX_GAP,
};

pub use direct::detector::{is_voice_command_utterance, DirectDetector};
pub use error::*;
pub use merger::{AutoQueueCooldown, DetectionMerger, MergedDetection};
pub use pipeline::DetectionPipeline;
pub use reading_mode::{
    is_complete_verse_navigation_command, ChapterChange, ReadingAdvance, ReadingMode,
};
pub use semantic::detector::SemanticDetector;
pub use sentence_buffer::SentenceBuffer;
pub use types::*;

#[cfg(feature = "onnx")]
pub use semantic::onnx_embedder::OnnxEmbedder;

#[cfg(feature = "vector-search")]
pub use semantic::hnsw_index::HnswVectorIndex;
