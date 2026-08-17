# SabbathCue Privacy & Data Flow

**Version:** 0.1.9  
**Last updated:** 2026-07-31

## Data inventory

| Data type            | Storage location                                            | Network transmission                                                   | Notes                                                                    |
| -------------------- | ----------------------------------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Audio input          | RAM only                                                    | None in local Vosk mode / cloud STT provider when selected             | Not intentionally written to disk by the app                             |
| Transcripts          | RAM and UI state during the active session                  | None in local Vosk mode / cloud STT response stream                    | Cleared through transcript/session controls or app close                 |
| Transcript snippets  | RAM only                                                    | A single phrase (max 500 characters) to DeepSeek or Cerebras when AI ranking is on | Off by default; never the rolling transcript. See "AI candidate ranking" |
| Bible database       | Bundled `rhema.db` resource, or development `data/rhema.db` | None during normal app operation                                       | Built during setup/release from source data                              |
| Deepgram API key     | OS keychain                                                 | Sent to Deepgram only when Deepgram mode is used                       | Never intentionally stored in plaintext app settings                     |
| Soniox API key       | OS keychain                                                 | Sent to Soniox only when Soniox mode is used                           | Never intentionally stored in plaintext app settings                     |
| Speechmatics API key | OS keychain                                                 | Sent to Speechmatics only when Speechmatics mode is used               | Never intentionally stored in plaintext app settings                     |
| DeepSeek API key     | OS keychain                                                 | Sent to DeepSeek only when DeepSeek ranking is selected and enabled    | Read only by the Rust backend; never exposed to the WebView              |
| Cerebras API key     | OS keychain                                                 | Sent to Cerebras only when Cerebras ranking is selected and enabled    | Read only by the Rust backend; never exposed to the WebView              |
| HTTP bearer token    | OS keychain                                                 | Sent by local clients in the `Authorization` header over loopback HTTP | Generated locally; used to authenticate remote-control requests          |
| Service plans        | Local app data/settings storage                             | None                                                                   | User-created local files/data                                            |
| Settings             | Local app data/settings storage                             | None                                                                   | Includes non-secret preferences and feature toggles                      |
| Detection models     | App resources or local `models/` directory                  | None during normal operation                                           | ONNX and Vosk model files run locally                                    |
| Embeddings           | App resources or local `embeddings/` directory              | None during normal operation                                           | Pre-computed verse vectors                                               |

## Network flows

### No network (local Vosk mode, default)

- Audio -> local Vosk worker -> transcript -> UI
- No outbound STT traffic

### Cloud STT modes (opt-in)

Exactly one STT provider is active at a time, chosen by the operator in
Settings -> Speech Recognition. Each requires the operator's own API key,
stored in the OS keychain.

- Audio -> Deepgram WebSocket (`wss://api.deepgram.com/v1/listen`) -> transcript -> UI
  - A REST fallback may upload buffered audio windows if WebSocket transcription fails
- Audio -> Soniox WebSocket (`wss://stt-rt.soniox.com/transcribe-websocket`) -> transcript -> UI
- Audio -> Speechmatics WebSocket (`wss://eu2.rt.speechmatics.com/v2`) -> transcript -> UI

Gladia was evaluated in earlier versions and has been removed. Persisted
settings naming Gladia migrate to local Vosk on load.

### AI candidate ranking (opt-in, off by default)

An optional feature that helps disambiguate _indirect_ spoken references
(for example, "the passage where Paul and Silas sang in prison") when local
detection surfaces several plausible passages.

- Short transcript phrase + candidate packs -> DeepSeek (`https://api.deepseek.com/chat/completions`) or Cerebras (`https://api.cerebras.ai/v1/chat/completions`) -> candidate selection or abstention -> UI badge
- Key validation only:
  - DeepSeek: `GET https://api.deepseek.com/models`
  - Cerebras: `GET https://api.cerebras.ai/v1/models`

#### What is transmitted

- One transcript phrase, hard-truncated to 500 characters
- Up to 8 candidate packs already found locally, each containing the reference, bounded verse text (max 500 chars), and local confidence score
- The operator's provider API key as a bearer token

#### What is never transmitted

- The rolling or full-service transcript
- Audio of any kind
- Service plans, settings, church identity, or operator identity
- Any installation or device identifier

#### Trigger conditions

A request is made only when _all_ of the following
hold: the feature toggle is on, a key is configured for the active provider,
the current detection batch contains two or more ambiguous semantic candidates,
and no explicit reference already passed the operator's confidence threshold.
Ordinary spoken references such as "John chapter three verse sixteen" are
resolved entirely locally and produce no outbound traffic.

**Response handling.** The model returns a choice identifying one of the supplied
candidates, or an abstention. It cannot inject unvetted text, and the displayed verse
is always read from the local Bible database. The result is advisory: it marks a
suggestion badge in the operator's panel and never changes what is sent to the projector.

**Data handling by the providers.**
- **DeepSeek:** Published policies allow submitted input to be used for service improvement, and data may be processed in the People's Republic of China.
- **Cerebras:** Operates in US/international cloud infrastructure; handles JSON Schema completions under its standard enterprise and cloud terms.

Because sermon speech can incidentally contain personal information, this feature ships
**disabled by default** and is strictly bounded. Organisations with specific compliance
or data-residency obligations can choose their preferred provider or leave AI candidate
ranking switched off; all detection features continue to operate without it.

### Setup-time downloads

- Bible source data, ML models, the Vosk STT model, and the optional NDI SDK may be downloaded during setup or release preparation.
- These downloads are not part of normal local operation after assets are installed.

### Remote control (loopback only by default)

- OSC: UDP on `127.0.0.1:8000`
- HTTP: TCP on `127.0.0.1:8080` with bearer-token authentication for private endpoints
- Remote-control traffic is local inbound traffic unless a future LAN opt-in feature changes the bind host

## Data retention

- Audio: held in memory during active transcription
- Transcripts: held in application state for the active session
- Transcript snippets sent for AI ranking: held in memory for the duration of the request only; retention beyond that point is governed by the provider's policy
- Service plans and settings: persisted locally until removed by the user or application cleanup
- API keys and HTTP tokens: persisted in the OS keychain until removed or rotated
- No server-side SabbathCue storage, cloud sync, analytics database, or telemetry pipeline

## Third-party dependencies

| Dependency   | Purpose                       | Data shared                                                                                        |
| ------------ | ----------------------------- | -------------------------------------------------------------------------------------------------- |
| Vosk         | Local worker STT              | Audio stays on the local machine                                                                   |
| Deepgram     | Optional cloud STT            | Audio stream and API key when enabled                                                              |
| Soniox       | Optional cloud STT            | Audio stream and API key when enabled                                                              |
| Speechmatics | Optional cloud STT            | Audio stream and API key when enabled                                                              |
| DeepSeek     | Optional AI candidate ranking | One <=500-character transcript phrase, up to 5 local candidate summaries, and API key when enabled |
| LibreOffice  | Optional PPTX-to-PDF          | Deck file stays on the local machine                                                               |
| ONNX Runtime | Local ML inference            | None                                                                                               |
| SQLite       | Local Bible database          | None                                                                                               |
| NDI SDK      | Video broadcast output        | Video frames to configured local/broadcast NDI consumers                                           |

## Operator controls

| Control                        | Location                       | Default      |
| ------------------------------ | ------------------------------ | ------------ |
| STT provider (local vs. cloud) | Settings -> Speech Recognition | Vosk (local) |
| AI candidate ranking on/off    | Settings -> AI Ranking         | Off          |
| DeepSeek API key add/remove    | Settings -> AI Ranking         | Not set      |
| Remove any stored key          | Respective settings section    | n/a          |

Removing the DeepSeek key also switches the ranking toggle off, so the
feature cannot be left in a state that reports as active without
credentials.

## Compliance notes

- **GDPR**: SabbathCue is local-first and does not operate a cloud account system or telemetry backend. Where an optional cloud provider is enabled, the operating organisation is the controller for the content it chooses to transmit, and the selected provider is the processor. AI candidate ranking involves a transfer outside the EEA/UK and should be assessed before enabling in jurisdictions where that matters.
- **HIPAA**: Not applicable; SabbathCue is not a healthcare application.
- **SOC 2**: Not certified; this document provides self-attested evidence for buyer evaluation.

## Verification status

The transmission limits described above are enforced in code and covered by
automated tests:

| Claim                                              | Enforced at                                                     | Test evidence                                                     |
| -------------------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------- |
| Transcript truncated to 500 characters             | `src-tauri/src/commands/deepseek.rs` (`build_request_body`)     | `request_body_clamps_inputs_and_pins_speed_config`                |
| At most 5 candidates, each summary <=80 characters | `src-tauri/src/commands/deepseek.rs` (`build_request_body`)     | `request_body_clamps_inputs_and_pins_speed_config`                |
| Model can only select a supplied candidate         | `src-tauri/src/commands/deepseek.rs` (`letter_to_candidate_id`) | `letters_map_back_to_supplied_candidate_ids_only`                 |
| Unexpected model output abstains rather than acts  | `src-tauri/src/commands/deepseek.rs` (`SseLetterScanner`)       | `scanner_abstains_on_unexpected_content`, `scanner_abstains_on_n` |
| Feature off by default                             | `src/stores/settings-store.ts`                                  | `deepseek ranking defaults off with no key configured`            |
| Ranking never influences what is projected         | `src/lib/verse-detection-workflow.ts`                           | `does not influence which detection is previewed`                 |
| Key presence read from keychain, never from disk   | `src/stores/settings-store.ts` (`hydrateSettings`)              | `hydrates DeepSeek key presence from the keychain, not from disk` |

**Live path verified 2026-07-31.** A live service test recorded 23 ranking
requests for approximately six spoken indirect phrases, with zero failures or
timeouts. Observed ranking latency was 715 ms at P50 and approximately 925 ms
at P95; the slowest cold-start request was 1,221 ms against the 1,800 ms cap.
The indirect-reference result was correct for 4 of 6 cases, while explicit
references remained 3 of 3 locally resolved. Source: the supplied
`SabbathCue Personal.log` from the 2026-07-31 test session.

The live trace also confirmed that STT can emit multiple detection batches
while a phrase is still extending; batches do not arrive exactly once per
finished phrase. The client now waits for a 400 ms quiet period and caches
identical transcript/candidate-set requests, while the request construction,
response parsing, gating, and failure handling remain covered by automated
tests. These figures are a baseline from that session, not a guarantee for
other networks or providers.
