use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::Receiver;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

use crate::error::SttError;
use crate::keyterms::bible_keyterms;
use crate::provider::SttProvider;
use crate::types::{SttConfig, TranscriptEvent, Word};

const MAX_RECONNECT_ATTEMPTS: u32 = 5;
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
/// Batch up to 50ms of audio before sending (at 16kHz, that is 800 samples).
/// This keeps Deepgram behaving like live captions instead of delayed dictation.
const BATCH_SAMPLES: usize = 800;
pub(crate) const DEEPGRAM_ENDPOINTING_MS: &str = "300";
pub(crate) const DEEPGRAM_UTTERANCE_END_MS: &str = "1000";
pub(crate) const MAX_DEEPGRAM_KEYTERMS: usize = 100;

pub struct DeepgramClient {
    config: SttConfig,
    cancelled: Arc<AtomicBool>,
}

impl std::fmt::Debug for DeepgramClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepgramClient")
            .field("model", &self.config.model)
            .finish_non_exhaustive()
    }
}

impl DeepgramClient {
    pub fn new(config: SttConfig) -> Self {
        Self {
            config,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Build the Deepgram WebSocket URL with query parameters and keyword boosting.
    fn build_url(&self) -> Result<Url, SttError> {
        let mut url = Url::parse("wss://api.deepgram.com/v1/listen")
            .map_err(|e| SttError::ConnectionFailed(e.to_string()))?;

        {
            let mut q = url.query_pairs_mut();
            append_deepgram_base_query(&mut q, &self.config);
            q.append_pair("interim_results", "true");
            q.append_pair("endpointing", DEEPGRAM_ENDPOINTING_MS);
            q.append_pair("utterance_end_ms", DEEPGRAM_UTTERANCE_END_MS);
            q.append_pair("vad_events", "true");
            append_deepgram_keyterms(&mut q);
            log::info!(
                "Deepgram keyterm boosting: {} keyterms added",
                deepgram_keyterms().len()
            );
        }

        log::info!("Deepgram WebSocket URL: {}", url.as_str());
        Ok(url)
    }

    /// Connect to Deepgram and stream audio from `audio_rx`, emitting transcript events to `event_tx`.
    pub async fn connect(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        if self.config.api_key.is_empty() {
            return Err(SttError::ApiKeyMissing);
        }

        let cancelled = self.cancelled.clone();
        let mut attempts: u32 = 0;

        loop {
            if cancelled.load(Ordering::SeqCst) {
                log::info!("DeepgramClient: cancelled, stopping connection loop");
                break;
            }

            match self
                .try_connect(audio_rx.clone(), event_tx.clone(), cancelled.clone())
                .await
            {
                Ok(()) => {
                    // Clean shutdown
                    log::info!("DeepgramClient: connection closed normally");
                    break;
                }
                Err(e) => {
                    attempts += 1;
                    log::warn!(
                        "DeepgramClient: connection error (attempt {attempts}/{MAX_RECONNECT_ATTEMPTS}): {e}",
                    );

                    if attempts >= MAX_RECONNECT_ATTEMPTS {
                        log::error!("DeepgramClient: max reconnection attempts reached");
                        return Err(e);
                    }

                    tokio::time::sleep(RECONNECT_DELAY).await;
                }
            }
        }

        Ok(())
    }

    /// Attempt a single WebSocket connection and run send/receive loops.
    #[allow(clippy::too_many_lines)]
    async fn try_connect(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), SttError> {
        let url = self.build_url()?;

        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|e| SttError::ConnectionFailed(e.to_string()))?;

        let auth_value = format!("Token {}", self.config.api_key);
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&auth_value)
                .map_err(|e| SttError::ConnectionFailed(e.to_string()))?,
        );

        let (ws_stream, _response) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| SttError::ConnectionFailed(e.to_string()))?;

        log::info!("DeepgramClient: connected to Deepgram");
        let _ = event_tx.send(TranscriptEvent::Connected).await;

        let (mut write, mut read) = ws_stream.split();

        let send_cancelled = cancelled.clone();
        let recv_cancelled = cancelled.clone();

        // Track unexpected disconnects so try_connect returns Err and triggers reconnection.
        let send_error_flag = Arc::new(AtomicBool::new(false));
        let recv_error_flag = Arc::new(AtomicBool::new(false));
        let error_detail = Arc::new(Mutex::new(None::<String>));
        let send_err = send_error_flag.clone();
        let recv_err = recv_error_flag.clone();
        let send_error_detail = error_detail.clone();
        let recv_error_detail = error_detail.clone();

        // Split the sender into two parts to avoid blocking the tokio runtime:
        // 1. A blocking thread reads audio from crossbeam → sends to a tokio channel
        // 2. An async task reads from the tokio channel → writes to the WebSocket
        //
        // The blocking crossbeam recv_timeout() was starving the receiver/consumer
        // tasks when run inside tokio::spawn, causing events to stop flowing.

        // Bridge channel: blocking audio reader → async WebSocket writer
        #[allow(clippy::items_after_statements)]
        enum WsCommand {
            Audio(Vec<u8>),
            KeepAlive,
            Close,
        }
        let (ws_tx, mut ws_rx) = tokio::sync::mpsc::channel::<WsCommand>(64);

        // Part 1: Blocking thread reads audio from crossbeam channel
        let mut audio_reader = {
            let ws_tx = ws_tx.clone();
            let cancelled = send_cancelled.clone();
            tokio::task::spawn_blocking(move || {
                let mut batch_buf: Vec<u8> = Vec::with_capacity(BATCH_SAMPLES * 2);
                let batch_byte_threshold = BATCH_SAMPLES * 2;
                let mut last_send = std::time::Instant::now();
                let keepalive_interval = Duration::from_secs(5);

                loop {
                    if cancelled.load(Ordering::SeqCst) {
                        let _ = ws_tx.blocking_send(WsCommand::Close);
                        break;
                    }

                    match audio_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(samples) => {
                            for sample in &samples {
                                batch_buf.extend_from_slice(&sample.to_le_bytes());
                            }
                            if batch_buf.len() >= batch_byte_threshold {
                                let data = std::mem::take(&mut batch_buf);
                                if ws_tx.blocking_send(WsCommand::Audio(data)).is_err() {
                                    break;
                                }
                                last_send = std::time::Instant::now();
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            // Flush remaining audio
                            if !batch_buf.is_empty() {
                                let data = std::mem::take(&mut batch_buf);
                                if ws_tx.blocking_send(WsCommand::Audio(data)).is_err() {
                                    break;
                                }
                                last_send = std::time::Instant::now();
                            }
                            // KeepAlive if idle >5s
                            if last_send.elapsed() >= keepalive_interval {
                                if ws_tx.blocking_send(WsCommand::KeepAlive).is_err() {
                                    break;
                                }
                                last_send = std::time::Instant::now();
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            // Audio source closed
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

        // Part 2: Async task writes to WebSocket (non-blocking, doesn't starve tokio)
        let mut ws_writer = tokio::spawn(async move {
            while let Some(cmd) = ws_rx.recv().await {
                match cmd {
                    WsCommand::Audio(data) => {
                        if let Err(e) = write.send(Message::Binary(data.into())).await {
                            log::error!("DeepgramClient ws_writer: send error: {e}");
                            send_err.store(true, Ordering::SeqCst);
                            if let Ok(mut detail) = send_error_detail.lock() {
                                *detail = Some(format!("send error: {e}"));
                            }
                            break;
                        }
                    }
                    WsCommand::KeepAlive => {
                        let ka = serde_json::json!({"type": "KeepAlive"}).to_string();
                        if let Err(e) = write.send(Message::Text(ka.into())).await {
                            log::error!("DeepgramClient ws_writer: keepalive error: {e}");
                            send_err.store(true, Ordering::SeqCst);
                            if let Ok(mut detail) = send_error_detail.lock() {
                                *detail = Some(format!("keepalive error: {e}"));
                            }
                            break;
                        }
                    }
                    WsCommand::Close => {
                        let close_msg = serde_json::json!({"type": "CloseStream"}).to_string();
                        let _ = write.send(Message::Text(close_msg.into())).await;
                        let _ = write.close().await;
                        break;
                    }
                }
            }
        });

        // Receiver task: reads text frames and parses Deepgram JSON.
        let mut receiver = tokio::spawn(async move {
            while let Some(msg_result) = read.next().await {
                if recv_cancelled.load(Ordering::SeqCst) {
                    break;
                }

                match msg_result {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = parse_and_send(&text, &event_tx).await {
                            log::warn!("DeepgramClient receiver: parse error: {e}");
                        }
                    }
                    Ok(Message::Close(close)) => {
                        let reason = close
                            .as_ref()
                            .map(|frame| {
                                format!(
                                    "server closed connection: code={} reason={}",
                                    frame.code, frame.reason
                                )
                            })
                            .unwrap_or_else(|| "server closed connection without a reason".into());
                        log::info!("DeepgramClient receiver: {reason}");
                        if !recv_cancelled.load(Ordering::SeqCst) {
                            recv_err.store(true, Ordering::SeqCst);
                            if let Ok(mut detail) = recv_error_detail.lock() {
                                *detail = Some(reason);
                            }
                        }
                        break;
                    }
                    Ok(_) => {
                        // Ignore binary/ping/pong frames
                    }
                    Err(e) => {
                        log::error!("DeepgramClient receiver: WebSocket error: {e}");
                        recv_err.store(true, Ordering::SeqCst);
                        if let Ok(mut detail) = recv_error_detail.lock() {
                            *detail = Some(format!("WebSocket error: {e}"));
                        }
                        break;
                    }
                }
            }
        });

        // Return promptly when any connection side finishes. On errors, this lets the outer
        // reconnect loop run instead of waiting for audio capture to stop.
        tokio::select! {
            _ = &mut audio_reader => {}
            _ = &mut ws_writer => {}
            _ = &mut receiver => {}
        }

        audio_reader.abort();
        ws_writer.abort();
        receiver.abort();

        let _ = tokio::join!(audio_reader, ws_writer, receiver);

        // If either side had an unexpected error, return Err so the connection loop retries.
        if send_error_flag.load(Ordering::SeqCst) || recv_error_flag.load(Ordering::SeqCst) {
            let detail = error_detail
                .lock()
                .ok()
                .and_then(|detail| detail.clone())
                .unwrap_or_else(|| "Connection lost unexpectedly".into());
            return Err(SttError::ConnectionFailed(detail));
        }

        Ok(())
    }

    /// Cancel the current connection and signal shutdown.
    pub fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

pub(crate) fn append_deepgram_base_query(
    q: &mut url::form_urlencoded::Serializer<'_, url::UrlQuery<'_>>,
    config: &SttConfig,
) {
    q.append_pair("model", &config.model);
    q.append_pair("encoding", &config.encoding);
    q.append_pair("sample_rate", &config.sample_rate.to_string());
    q.append_pair("channels", "1");
    q.append_pair("punctuate", "true");
    q.append_pair("smart_format", "true");
    q.append_pair("numerals", "true");
    q.append_pair("dictation", "true");
    q.append_pair("filler_words", "true");

    if let Some(ref lang) = config.language {
        q.append_pair("language", lang);
    }
}

pub(crate) fn append_deepgram_keyterms(
    q: &mut url::form_urlencoded::Serializer<'_, url::UrlQuery<'_>>,
) {
    for term in deepgram_keyterms() {
        q.append_pair("keyterm", &term);
    }
}

pub(crate) fn deepgram_keyterms() -> Vec<String> {
    let core_terms = vec![
        "Jesus".to_string(),
        "Christ".to_string(),
        "God".to_string(),
        "Lord".to_string(),
        "Holy Spirit".to_string(),
        "Sabbath".to_string(),
        "sanctuary".to_string(),
        "commandments".to_string(),
        "second coming".to_string(),
        "three angels messages".to_string(),
        "spirit of prophecy".to_string(),
        "Adventist".to_string(),
        "hymn number".to_string(),
        "scripture reading".to_string(),
        "responsive reading".to_string(),
    ];
    let bible_terms = bible_keyterms();

    let mut seen = std::collections::HashSet::new();
    let mut all_keyterms: Vec<String> = Vec::new();
    for term in core_terms.into_iter().chain(bible_terms) {
        if seen.insert(term.clone()) {
            all_keyterms.push(term);
        }
        if all_keyterms.len() >= MAX_DEEPGRAM_KEYTERMS {
            break;
        }
    }

    all_keyterms
}

/// Parse a Deepgram JSON response and send the corresponding `TranscriptEvent`.
async fn parse_and_send(
    text: &str,
    event_tx: &mpsc::Sender<TranscriptEvent>,
) -> Result<(), SttError> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| SttError::ParseError(e.to_string()))?;

    // Deepgram may send different message types; we only handle "Results"
    let msg_type = json
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    match msg_type {
        "UtteranceEnd" => {
            let _ = event_tx.send(TranscriptEvent::UtteranceEnd).await;
            return Ok(());
        }
        "SpeechStarted" => {
            let _ = event_tx.send(TranscriptEvent::SpeechStarted).await;
            return Ok(());
        }
        "Results" => { /* continue parsing below */ }
        _ => {
            // Metadata, etc. — ignore silently.
            return Ok(());
        }
    }

    let is_final = json
        .get("is_final")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let speech_final = json
        .get("speech_final")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let channel = json.get("channel");
    let alternatives = channel
        .and_then(|c| c.get("alternatives"))
        .and_then(|a| a.as_array());

    let first_alt = alternatives.and_then(|arr| arr.first());

    let transcript = first_alt
        .and_then(|a| a.get("transcript"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    let confidence = first_alt
        .and_then(|a| a.get("confidence"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);

    let words = first_alt
        .and_then(|a| a.get("words"))
        .and_then(|w| w.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| {
                    Some(Word {
                        text: w.get("word")?.as_str()?.to_string(),
                        start: w.get("start")?.as_f64()?,
                        end: w.get("end")?.as_f64()?,
                        confidence: w.get("confidence")?.as_f64()?,
                        punctuated_word: w
                            .get("punctuated_word")
                            .and_then(|p| p.as_str())
                            .map(ToString::to_string),
                    })
                })
                .collect::<Vec<Word>>()
        })
        .unwrap_or_default();

    let event = if is_final {
        TranscriptEvent::Final {
            transcript,
            words,
            confidence,
            speech_final,
        }
    } else {
        TranscriptEvent::Partial { transcript, words }
    };

    event_tx
        .send(event)
        .await
        .map_err(|e| SttError::SendError(e.to_string()))?;

    Ok(())
}

// ── SttProvider implementation ───────────────────────────────────────────────

#[async_trait::async_trait]
impl SttProvider for DeepgramClient {
    async fn start(
        &self,
        audio_rx: Receiver<Vec<i16>>,
        event_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<(), SttError> {
        let result = self.connect(audio_rx.clone(), event_tx.clone()).await;

        // On max reconnect failure, fall back to REST mode (hybrid).
        if let Err(ref e) = result {
            log::warn!(
                "[STT-Deepgram] WebSocket failed after retries: {e}, switching to REST fallback"
            );
            let _ = event_tx.send(TranscriptEvent::Connected).await;

            let rest_client = crate::rest::DeepgramRestClient::new(self.config.clone());
            let flush_interval = std::time::Duration::from_millis(1500);
            let cancelled = self.cancelled.clone();
            let (rest_audio_tx, mut rest_audio_rx) = mpsc::channel::<Vec<i16>>(4);

            let rest_reader = tokio::task::spawn_blocking(move || {
                let mut audio_buffer: Vec<i16> = Vec::new();
                let mut last_flush = std::time::Instant::now();

                loop {
                    if cancelled.load(Ordering::SeqCst) {
                        if !audio_buffer.is_empty() {
                            let _ = rest_audio_tx.blocking_send(std::mem::take(&mut audio_buffer));
                        }
                        break;
                    }

                    match audio_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(samples) => {
                            audio_buffer.extend(samples);

                            if last_flush.elapsed() >= flush_interval && !audio_buffer.is_empty() {
                                if rest_audio_tx
                                    .blocking_send(std::mem::take(&mut audio_buffer))
                                    .is_err()
                                {
                                    break;
                                }
                                last_flush = std::time::Instant::now();
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            if last_flush.elapsed() >= flush_interval && !audio_buffer.is_empty() {
                                if rest_audio_tx
                                    .blocking_send(std::mem::take(&mut audio_buffer))
                                    .is_err()
                                {
                                    break;
                                }
                                last_flush = std::time::Instant::now();
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            if !audio_buffer.is_empty() {
                                let _ =
                                    rest_audio_tx.blocking_send(std::mem::take(&mut audio_buffer));
                            }
                            break;
                        }
                    }
                }
            });

            while let Some(samples) = rest_audio_rx.recv().await {
                match rest_client.transcribe(&samples).await {
                    Ok(events) => {
                        for evt in events {
                            let _ = event_tx.send(evt).await;
                        }
                    }
                    Err(e) => {
                        log::error!("[STT-REST] Transcription failed: {e}");
                    }
                }
            }

            let _ = rest_reader.await;
        }

        Ok(())
    }

    fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn name(&self) -> &'static str {
        "deepgram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn parse_one(payload: &str) -> Option<TranscriptEvent> {
        let (tx, mut rx) = mpsc::channel(4);
        parse_and_send(payload, &tx).await.unwrap();
        rx.try_recv().ok()
    }

    #[tokio::test]
    async fn websocket_url_preserves_accuracy_and_latency_params() {
        let client = DeepgramClient::new(SttConfig {
            api_key: "test".into(),
            model: "nova-3".into(),
            sample_rate: 16_000,
            encoding: "linear16".into(),
            language: Some("en".into()),
        });

        let url = client.build_url().unwrap();
        let pairs = url.query_pairs().collect::<Vec<_>>();

        assert!(pairs.contains(&("model".into(), "nova-3".into())));
        assert!(pairs.contains(&("encoding".into(), "linear16".into())));
        assert!(pairs.contains(&("sample_rate".into(), "16000".into())));
        assert!(pairs.contains(&("channels".into(), "1".into())));
        assert!(pairs.contains(&("numerals".into(), "true".into())));
        assert!(pairs.contains(&("dictation".into(), "true".into())));
        assert!(pairs.contains(&("filler_words".into(), "true".into())));
        assert!(pairs.contains(&("interim_results".into(), "true".into())));
        assert!(pairs.contains(&("endpointing".into(), DEEPGRAM_ENDPOINTING_MS.into())));
        assert!(pairs.contains(&("utterance_end_ms".into(), DEEPGRAM_UTTERANCE_END_MS.into())));
        assert!(pairs.contains(&("language".into(), "en".into())));
        assert!(pairs.contains(&("keyterm".into(), "Jesus".into())));
        assert!(!pairs.iter().any(|(key, _)| key == "no_delay"));
    }

    #[tokio::test]
    async fn keyterms_are_deduplicated_and_capped() {
        let terms = deepgram_keyterms();

        let unique = terms.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(terms.len(), unique.len());
        assert!(terms.len() <= MAX_DEEPGRAM_KEYTERMS);
        assert_eq!(terms.first().map(String::as_str), Some("Jesus"));
        assert!(terms.iter().any(|term| term == "John"));
    }

    #[tokio::test]
    async fn parses_speech_started_event() {
        let event = parse_one(r#"{"type":"SpeechStarted"}"#).await;

        assert!(matches!(event, Some(TranscriptEvent::SpeechStarted)));
    }

    #[tokio::test]
    async fn parses_utterance_end_event() {
        let event = parse_one(r#"{"type":"UtteranceEnd"}"#).await;

        assert!(matches!(event, Some(TranscriptEvent::UtteranceEnd)));
    }

    #[tokio::test]
    async fn parses_interim_result_as_partial() {
        let event = parse_one(
            r#"{
                "type":"Results",
                "is_final":false,
                "speech_final":false,
                "channel":{
                    "alternatives":[{
                        "transcript":"John three sixteen",
                        "confidence":0.81,
                        "words":[{
                            "word":"john",
                            "start":0.0,
                            "end":0.4,
                            "confidence":0.9,
                            "punctuated_word":"John"
                        }]
                    }]
                }
            }"#,
        )
        .await;

        match event {
            Some(TranscriptEvent::Partial { transcript, words }) => {
                assert_eq!(transcript, "John three sixteen");
                assert_eq!(words.len(), 1);
                assert_eq!(words[0].punctuated_word.as_deref(), Some("John"));
            }
            other => panic!("expected partial, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parses_final_result_with_confidence_and_speech_final() {
        let event = parse_one(
            r#"{
                "type":"Results",
                "is_final":true,
                "speech_final":true,
                "channel":{
                    "alternatives":[{
                        "transcript":"John 3:16",
                        "confidence":0.97,
                        "words":[]
                    }]
                }
            }"#,
        )
        .await;

        match event {
            Some(TranscriptEvent::Final {
                transcript,
                confidence,
                speech_final,
                ..
            }) => {
                assert_eq!(transcript, "John 3:16");
                assert_eq!(confidence, 0.97);
                assert!(speech_final);
            }
            other => panic!("expected final, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ignores_metadata_messages() {
        let event = parse_one(r#"{"type":"Metadata","request_id":"abc"}"#).await;

        assert!(event.is_none());
    }
}
