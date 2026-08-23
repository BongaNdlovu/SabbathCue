//! Speech-to-text integration for the `SabbathCue` application.
//!
//! Provides real-time transcription via multiple providers:
//! - **Deepgram** (cloud): WebSocket streaming with keyword boosting
//! - **Soniox** (cloud): WebSocket streaming with language hints
//! - **Speechmatics** (cloud): WebSocket streaming with partial and final transcripts
//! - **Vosk** (local): offline compatibility provider via a worker process
//!
//! # Key types
//!
//! - [`SttProvider`]: trait for swappable STT backends
//! - [`DeepgramClient`]: Deepgram WebSocket/REST provider
//! - [`SonioxClient`]: Soniox WebSocket provider
//! - [`TranscriptEvent`]: streaming transcript events
//! - [`SttConfig`]: API configuration
//! - [`SttError`]: error type for STT operations
//!
//! # Feature flags
//!
//! - `rest-fallback`: enables REST API fallback client

pub mod bench;
pub mod credentials;
pub mod deepgram;
pub mod error;
pub mod keyterms;
pub mod provider;
pub mod reconnect;
pub mod rest;
pub mod soniox;
pub mod speechmatics;
pub mod types;
pub mod vosk;
pub mod worker;

pub use deepgram::DeepgramClient;
pub use error::SttError;
pub use keyterms::bible_keyterms;
pub use provider::SttProvider;
pub use soniox::SonioxClient;
pub use soniox::SONIOX_MODEL;
pub use speechmatics::{SpeechmaticsClient, SPEECHMATICS_MODEL};
pub use types::{SttConfig, TranscriptEvent, Word};
pub use vosk::VoskProvider;
