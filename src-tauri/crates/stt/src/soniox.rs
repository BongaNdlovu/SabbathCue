use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::Receiver;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::error::SttError;
use crate::keyterms::bible_keyterms_for_language;
use crate::provider::SttProvider;
use crate::types::{SttConfig, TranscriptEvent};

const SONIOX_RT_URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
const MAX_RECONNECT_ATTEMPTS: u32 = 5;
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
/// Bounded window to drain the server-flushed final tokens after the audio
/// source ends on a clean stop.
const CLEAN_SHUTDOWN_DRAIN: Duration = Duration::from_secs(5);
/// Shorter drain window on error paths.
const ERROR_SHUTDOWN_DRAIN: Duration = Duration::from_secs(1);
const BATCH_SAMPLES: usize = 800;
pub const SONIOX_MODEL: &str = "stt-rt-v5";

#[derive(Debug, Deserialize)]
struct SonioxToken {
    text: String,
    #[serde(default)]
    is_final: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SonioxResponse {
    #[serde(default)]
    tokens: Vec<SonioxToken>,
    #[serde(default)]
    error_code: Option<i32>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug)]
pub struct SonioxClient {
    config: SttConfig,
    cancelled: Arc<AtomicBool>,
}

enum WsCommand {
    Audio(Vec<u8>),
    Close,
}

/// Build the initial Soniox WebSocket configuration payload.
pub(crate) fn build_start_payload(config: &SttConfig) -> serde_json::Value {
    let language = config.language.as_deref().unwrap_or("en");
    let language_hints = match language {
        "af" => vec!["af"],
        other => vec![other],
    };

    serde_json::json!({
        "api_key": config.api_key,
        "model": config.model,
        "audio_format": "pcm_s16le",
        "sample_rate": config.sample_rate,
        "num_channels": 1,
        "language_hints": language_hints,
        "enable_endpoint_detection": true,
        "context": {
            "terms": bible_keyterms_for_language(language),
        },
    })
}

/// Parse a Soniox token stream response into transcript events.
pub(crate) fn parse_token_response(
    json: &SonioxResponse,
    finalized_text: &mut String,
) -> Result<Vec<TranscriptEvent>, SttError> {
    if let Some(message) = soniox_error_message(json) {
        return Err(SttError::ParseError(message));
    }

    let mut events = Vec::new();
    let mut partial_parts: Vec<String> = Vec::new();
    let mut new_final_parts: Vec<String> = Vec::new();
    let mut endpoint = false;

    for token in &json.tokens {
        if token.text == "<end>" {
            endpoint = true;
            continue;
        }
        if token.is_final {
            new_final_parts.push(token.text.clone());
        } else {
            partial_parts.push(token.text.clone());
        }
    }

    if !new_final_parts.is_empty() {
        finalized_text.push_str(&new_final_parts.join(""));
    }

    let partial_transcript = format!("{}{}", finalized_text, partial_parts.join(""));
    // Committed tokens without `<end>` stay Partial. Emitting Final here with
    // speech_final=false made detection treat the citation as a partial (and
    // suppress it as a repeat); the later endpoint Final was then dropped as
    // duplicate_final, so John 1:1 never live-authorized (2026-08-21).
    if endpoint {
        // The endpoint Final must commit the trailing non-final tokens too:
        // [final "John three sixteen", partial "amen", <end>] ends an
        // utterance whose transcript is "John three sixteen amen" — dropping
        // the partials here erased spoken words from the session.
        let committed = partial_transcript;
        if !committed.trim().is_empty() {
            events.push(TranscriptEvent::Final {
                transcript: committed,
                words: vec![],
                confidence: crate::reconnect::UNSCORED_FINAL_CONFIDENCE,
                speech_final: true,
            });
        }
        events.push(TranscriptEvent::UtteranceEnd);
        finalized_text.clear();
    } else if !partial_transcript.trim().is_empty() {
        events.push(TranscriptEvent::Partial {
            transcript: partial_transcript,
            words: vec![],
        });
    }

    Ok(events)
}

fn soniox_error_message(json: &SonioxResponse) -> Option<String> {
    match (json.error_code, json.error_message.as_deref()) {
        (Some(code), Some(message)) => Some(format!("Soniox error {code}: {message}")),
        (Some(code), None) => Some(format!("Soniox error {code}")),
        (None, Some(message)) => Some(format!("Soniox error: {message}")),
        (None, None) => None,
    }
}

impl SonioxClient {
    pub fn new(config: SttConfig) -> Self {
        Self {
            config,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn connect(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        if self.config.api_key.trim().is_empty() {
            return Err(SttError::ApiKeyMissing);
        }

        let mut attempts: u32 = 0;
        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                break;
            }

            let connection_started = std::time::Instant::now();
            match self
                .try_connect(audio_rx.clone(), event_tx.clone(), self.cancelled.clone())
                .await
            {
                Ok(()) => break,
                Err(error) => {
                    if !matches!(
                        error,
                        SttError::ConnectionFailed(_) | SttError::SendError(_)
                    ) {
                        return Err(error);
                    }

                    // A connection that stayed up for a healthy stretch earns
                    // a fresh budget: only rapid back-to-back failures exhaust
                    // the retry limit.
                    attempts = crate::reconnect::track_reconnect_attempt(
                        attempts,
                        connection_started.elapsed(),
                    );
                    log::warn!(
                        "SonioxClient: connection error (attempt {attempts}/{MAX_RECONNECT_ATTEMPTS}): {error}"
                    );
                    if attempts >= MAX_RECONNECT_ATTEMPTS {
                        let _ = event_tx
                            .send(TranscriptEvent::Error(error.to_string()))
                            .await;
                        return Err(error);
                    }
                    tokio::time::sleep(RECONNECT_DELAY).await;
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn try_connect(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), SttError> {
        let (ws_stream, _response) = tokio_tungstenite::connect_async(SONIOX_RT_URL)
            .await
            .map_err(|e| SttError::ConnectionFailed(e.to_string()))?;

        log::info!("SonioxClient: connected to Soniox real-time API");
        let _ = event_tx.send(TranscriptEvent::Connected).await;

        let (mut write, mut read) = ws_stream.split();

        let start_payload = build_start_payload(&self.config).to_string();
        write
            .send(Message::Text(start_payload.into()))
            .await
            .map_err(|e| SttError::ConnectionFailed(e.to_string()))?;

        let send_error_flag = Arc::new(AtomicBool::new(false));
        let recv_error_flag = Arc::new(AtomicBool::new(false));
        let fatal_server_error_flag = Arc::new(AtomicBool::new(false));
        let error_detail = Arc::new(Mutex::new(None::<String>));
        let (ws_tx, mut ws_rx) = mpsc::channel::<WsCommand>(64);

        // Set when this connection attempt is over: abort() cannot stop a
        // spawn_blocking task, so the reader needs a flag to stop consuming
        // frames from the shared crossbeam channel.
        let reader_stop = Arc::new(AtomicBool::new(false));

        let mut audio_reader = {
            let ws_tx = ws_tx.clone();
            let cancelled = cancelled.clone();
            let reader_stop = reader_stop.clone();
            tokio::task::spawn_blocking(move || {
                let mut batch_buf = Vec::with_capacity(BATCH_SAMPLES * 2);
                let batch_byte_threshold = BATCH_SAMPLES * 2;

                loop {
                    if cancelled.load(Ordering::SeqCst) || reader_stop.load(Ordering::SeqCst) {
                        let _ = ws_tx.blocking_send(WsCommand::Close);
                        break;
                    }

                    match audio_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(samples) => {
                            for sample in samples {
                                batch_buf.extend_from_slice(&sample.to_le_bytes());
                            }
                            if batch_buf.len() >= batch_byte_threshold {
                                let data = std::mem::take(&mut batch_buf);
                                if ws_tx.blocking_send(WsCommand::Audio(data)).is_err() {
                                    break;
                                }
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            if !batch_buf.is_empty() {
                                let data = std::mem::take(&mut batch_buf);
                                if ws_tx.blocking_send(WsCommand::Audio(data)).is_err() {
                                    break;
                                }
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            if !batch_buf.is_empty() {
                                let data = std::mem::take(&mut batch_buf);
                                let _ = ws_tx.blocking_send(WsCommand::Audio(data));
                            }
                            let _ = ws_tx.blocking_send(WsCommand::Close);
                            break;
                        }
                    }
                }
            })
        };

        let send_err = send_error_flag.clone();
        let send_error_detail = error_detail.clone();
        let mut ws_writer = tokio::spawn(async move {
            while let Some(cmd) = ws_rx.recv().await {
                match cmd {
                    WsCommand::Audio(data) => {
                        if let Err(e) = write.send(Message::Binary(data.into())).await {
                            send_err.store(true, Ordering::SeqCst);
                            if let Ok(mut detail) = send_error_detail.lock() {
                                *detail = Some(format!("send error: {e}"));
                            }
                            break;
                        }
                    }
                    WsCommand::Close => {
                        let _ = write.close().await;
                        break;
                    }
                }
            }
        });

        let recv_cancelled = cancelled.clone();
        let recv_err = recv_error_flag.clone();
        let fatal_server_err = fatal_server_error_flag.clone();
        let recv_error_detail = error_detail.clone();
        let recv_event_tx = event_tx.clone();
        let finalized_text = Arc::new(Mutex::new(String::new()));
        // Soniox re-ships the whole accumulated utterance on every token
        // packet, so consecutive Partial events usually carry identical text.
        // Forwarding each one flooded the pipeline with duplicate work
        // (detection jobs, frontend store writes). Suppress an identical
        // Partial that directly follows the same text; Finals and all other
        // events always pass through.
        let mut last_partial_text = String::new();
        let mut receiver = tokio::spawn(async move {
            while let Some(msg_result) = read.next().await {
                // Do NOT break on cancel here: buffered server messages still
                // carry the final tokens of the session, and the writer's
                // close makes read.next() return None once the server is done.
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let parsed: SonioxResponse = match serde_json::from_str(&text) {
                            Ok(json) => json,
                            Err(error) => {
                                log::warn!("SonioxClient receiver: parse error: {error}");
                                continue;
                            }
                        };

                        let events = match finalized_text.lock() {
                            Ok(mut buffer) => match parse_token_response(&parsed, &mut buffer) {
                                Ok(events) => events,
                                Err(error) => {
                                    log::warn!("SonioxClient receiver: token parse error: {error}");
                                    fatal_server_err.store(true, Ordering::SeqCst);
                                    if let Ok(mut detail) = recv_error_detail.lock() {
                                        *detail = Some(match error {
                                            SttError::ParseError(message) => message,
                                            other => other.to_string(),
                                        });
                                    }
                                    break;
                                }
                            },
                            Err(_) => continue,
                        };

                        for event in events {
                            let duplicated = match &event {
                                TranscriptEvent::Partial { transcript, .. } => {
                                    let dup = *transcript == last_partial_text;
                                    if !dup {
                                        last_partial_text.clone_from(transcript);
                                    }
                                    dup
                                }
                                TranscriptEvent::Final { .. } | TranscriptEvent::UtteranceEnd => {
                                    last_partial_text.clear();
                                    false
                                }
                                _ => false,
                            };
                            if duplicated {
                                continue;
                            }
                            if recv_event_tx.send(event).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(Message::Close(close)) => {
                        if !recv_cancelled.load(Ordering::SeqCst) {
                            let reason = close.as_ref().map_or_else(
                                || "server closed connection without a reason".into(),
                                |frame| {
                                    format!(
                                        "server closed connection: code={} reason={}",
                                        frame.code, frame.reason
                                    )
                                },
                            );
                            recv_err.store(true, Ordering::SeqCst);
                            if let Ok(mut detail) = recv_error_detail.lock() {
                                *detail = Some(reason);
                            }
                        }
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        recv_err.store(true, Ordering::SeqCst);
                        if let Ok(mut detail) = recv_error_detail.lock() {
                            *detail = Some(format!("WebSocket error: {error}"));
                        }
                        break;
                    }
                }
            }
        });

        let audio_ended_normally = tokio::select! {
            _ = &mut audio_reader => true,
            _ = &mut ws_writer => false,
            _ = &mut receiver => false,
        };

        // Stop the blocking reader from stealing further frames from the
        // shared audio channel (abort() cannot stop spawn_blocking tasks).
        reader_stop.store(true, Ordering::SeqCst);

        // On normal audio end the reader queues WsCommand::Close and the
        // writer closes the socket; the server flushes remaining tokens.
        // Aborting the receiver dropped those finals — drain with a bounded
        // window instead; abort only if the server goes quiet.
        let drain_window = if audio_ended_normally {
            CLEAN_SHUTDOWN_DRAIN
        } else {
            ERROR_SHUTDOWN_DRAIN
        };
        let drained = tokio::time::timeout(drain_window, async {
            let _ = (&mut audio_reader).await;
            let _ = (&mut ws_writer).await;
            let _ = (&mut receiver).await;
        })
        .await;
        if drained.is_err() {
            log::warn!("SonioxClient: shutdown drain timed out; aborting tasks");
            audio_reader.abort();
            ws_writer.abort();
            receiver.abort();
            let _ = tokio::join!(audio_reader, ws_writer, receiver);
        }

        if fatal_server_error_flag.load(Ordering::SeqCst) {
            let detail = error_detail
                .lock()
                .ok()
                .and_then(|detail| detail.clone())
                .unwrap_or_else(|| "Soniox returned an error response".into());
            return Err(SttError::ParseError(detail));
        }

        if send_error_flag.load(Ordering::SeqCst) || recv_error_flag.load(Ordering::SeqCst) {
            let detail = error_detail
                .lock()
                .ok()
                .and_then(|detail| detail.clone())
                .unwrap_or_else(|| "Connection lost unexpectedly".into());
            return Err(SttError::ConnectionFailed(detail));
        }

        let _ = event_tx.send(TranscriptEvent::Disconnected).await;
        Ok(())
    }

    pub fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl SttProvider for SonioxClient {
    async fn start(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        self.connect(audio_rx, event_tx).await
    }

    fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn name(&self) -> &'static str {
        "soniox"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_payload_uses_stt_rt_v5_afrikaans_hints_and_endpoint_detection() {
        let payload = build_start_payload(&SttConfig {
            api_key: "test-key".into(),
            model: SONIOX_MODEL.into(),
            sample_rate: 16_000,
            encoding: "pcm_s16le".into(),
            language: Some("af".into()),
        });

        assert_eq!(payload["model"], SONIOX_MODEL);
        assert_eq!(payload["language_hints"], serde_json::json!(["af"]));
        assert_eq!(payload["enable_endpoint_detection"], true);
        assert_eq!(payload["audio_format"], "pcm_s16le");
        assert_eq!(payload["sample_rate"], 16_000);
    }

    #[test]
    fn start_payload_uses_selected_language_context_terms() {
        let payload = build_start_payload(&SttConfig {
            api_key: "test-key".into(),
            model: SONIOX_MODEL.into(),
            sample_rate: 16_000,
            encoding: "pcm_s16le".into(),
            language: Some("es".into()),
        });

        assert_eq!(payload["language_hints"], serde_json::json!(["es"]));
        assert!(payload["context"]["terms"]
            .as_array()
            .is_some_and(|terms| terms.iter().any(|term| term == "Juan")));
    }

    #[test]
    fn non_final_tokens_emit_partial_only() {
        let mut finalized = String::new();
        let events = parse_token_response(
            &SonioxResponse {
                tokens: vec![
                    SonioxToken {
                        text: "Johannes".into(),
                        is_final: false,
                    },
                    SonioxToken {
                        text: " 3".into(),
                        is_final: false,
                    },
                ],
                error_code: None,
                error_message: None,
            },
            &mut finalized,
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], TranscriptEvent::Partial { .. }));
    }

    #[test]
    fn final_tokens_emit_once_for_detection() {
        let mut finalized = String::new();
        let events = parse_token_response(
            &SonioxResponse {
                tokens: vec![
                    SonioxToken {
                        text: "Johannes 3 vers 16".into(),
                        is_final: true,
                    },
                    SonioxToken {
                        text: "<end>".into(),
                        is_final: true,
                    },
                ],
                error_code: None,
                error_message: None,
            },
            &mut finalized,
        )
        .unwrap();

        let finals: Vec<_> = events
            .iter()
            .filter(|event| matches!(event, TranscriptEvent::Final { .. }))
            .collect();
        assert_eq!(
            finals.len(),
            1,
            "a single Soniox packet must not emit two Finals: {events:?}"
        );
        assert!(matches!(
            finals[0],
            TranscriptEvent::Final {
                speech_final: true,
                ..
            }
        ));
        assert!(events
            .iter()
            .any(|event| matches!(event, TranscriptEvent::UtteranceEnd)));
    }

    #[test]
    fn committed_tokens_without_endpoint_stay_partial() {
        // 2026-08-21 live: John 1:1 committed as Final { speech_final: false }
        // before `<end>`. Detection treated that as a partial and suppressed
        // it as a repeat; the later endpoint Final was duplicate_final, so
        // the citation never live-authorized.
        let mut finalized = String::new();
        let events = parse_token_response(
            &SonioxResponse {
                tokens: vec![SonioxToken {
                    text: "John chapter 1 verse 1".into(),
                    is_final: true,
                }],
                error_code: None,
                error_message: None,
            },
            &mut finalized,
        )
        .unwrap();

        assert!(
            events
                .iter()
                .all(|event| matches!(event, TranscriptEvent::Partial { .. })),
            "committed tokens must stay partial until the endpoint: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, TranscriptEvent::Final { .. })),
            "a pre-endpoint Final would swallow the real utterance-end Final"
        );
        assert_eq!(finalized, "John chapter 1 verse 1");
    }

    #[test]
    fn endpoint_final_commits_trailing_non_final_tokens() {
        // [final "John three sixteen", partial "amen", <end>] ends an
        // utterance whose transcript includes the trailing words — the old
        // endpoint branch cloned `finalized_text` only and erased "amen".
        let mut finalized = String::new();
        let events = parse_token_response(
            &SonioxResponse {
                tokens: vec![
                    SonioxToken {
                        text: "John three sixteen".into(),
                        is_final: true,
                    },
                    SonioxToken {
                        text: " amen".into(),
                        is_final: false,
                    },
                    SonioxToken {
                        text: "<end>".into(),
                        is_final: true,
                    },
                ],
                error_code: None,
                error_message: None,
            },
            &mut finalized,
        )
        .unwrap();

        match events.first() {
            Some(TranscriptEvent::Final {
                transcript,
                speech_final,
                confidence,
                ..
            }) => {
                assert_eq!(transcript, "John three sixteen amen");
                assert!(*speech_final);
                assert!(
                    (confidence - crate::reconnect::UNSCORED_FINAL_CONFIDENCE).abs()
                        < f64::EPSILON,
                    "unscored finals must use the shared provider-neutral default"
                );
            }
            other => panic!("expected a speech-final event, got {other:?}"),
        }
        assert!(events
            .iter()
            .any(|event| matches!(event, TranscriptEvent::UtteranceEnd)));
    }

    #[test]
    fn endpoint_after_committed_tokens_emits_one_speech_final() {
        let mut finalized = String::new();
        let _ = parse_token_response(
            &SonioxResponse {
                tokens: vec![SonioxToken {
                    text: "John chapter 1 verse 1".into(),
                    is_final: true,
                }],
                error_code: None,
                error_message: None,
            },
            &mut finalized,
        )
        .unwrap();

        let events = parse_token_response(
            &SonioxResponse {
                tokens: vec![SonioxToken {
                    text: "<end>".into(),
                    is_final: true,
                }],
                error_code: None,
                error_message: None,
            },
            &mut finalized,
        )
        .unwrap();

        let finals: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TranscriptEvent::Final {
                    transcript,
                    speech_final,
                    ..
                } => Some((transcript.as_str(), *speech_final)),
                _ => None,
            })
            .collect();
        assert_eq!(
            finals,
            vec![("John chapter 1 verse 1", true)],
            "the endpoint packet is the only live-authorizing Final: {events:?}"
        );
        assert!(events
            .iter()
            .any(|event| matches!(event, TranscriptEvent::UtteranceEnd)));
        assert!(
            finalized.is_empty(),
            "endpoint must clear the committed buffer"
        );
    }

    #[test]
    fn server_error_code_returns_parse_error() {
        let mut finalized = String::new();
        let error = parse_token_response(
            &SonioxResponse {
                tokens: vec![],
                error_code: Some(401),
                error_message: Some("invalid API key".into()),
            },
            &mut finalized,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "parse error: Soniox error 401: invalid API key"
        );
    }
}
