//! Speech-to-text integration for the `SabbathCue` application.
//!
//! Provides real-time transcription via multiple providers:
//! - **Deepgram** (cloud) — WebSocket streaming with keyword boosting
//! - **Vosk** (local) — offline low-latency streaming via a worker process
//!
//! # Key types
//!
//! - [`SttProvider`] — trait for swappable STT backends
//! - [`DeepgramClient`] — Deepgram WebSocket/REST provider
//! - [`TranscriptEvent`] — streaming transcript events (partial, final, etc.)
//! - [`SttConfig`] — API configuration
//! - [`SttError`] — error type for STT operations
//!
//! # Feature flags
//!
//! - `rest-fallback` — enables REST API fallback client
//! - `whisper` — enables local Whisper STT provider

pub mod bench;
pub mod deepgram;
pub mod error;
pub mod keyterms;
pub mod provider;
pub mod rest;
pub mod types;
pub mod vosk;

#[cfg(feature = "faster-whisper")]
pub mod faster_whisper;
#[cfg(feature = "whisper")]
pub mod whisper;

pub use deepgram::DeepgramClient;
pub use error::SttError;
pub use keyterms::bible_keyterms;
pub use provider::SttProvider;
pub use types::{SttConfig, TranscriptEvent, Word};
pub use vosk::VoskProvider;

#[cfg(feature = "faster-whisper")]
pub use faster_whisper::FasterWhisperProvider;
#[cfg(feature = "whisper")]
pub use whisper::WhisperProvider;
