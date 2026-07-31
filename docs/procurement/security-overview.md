# SabbathCue Security Overview

**Version:** 0.1.9  
**Last updated:** 2026-07-31  
**Classification:** Vendor Packet - Public

## Architecture

SabbathCue is a Tauri v2 desktop application combining a React frontend
(WebView) with a Rust backend. The WebView runs under a restrictive Content
Security Policy. External network traffic, when enabled, originates from the
Rust side rather than browser-side JavaScript.

### Key security properties

| Property            | Implementation                                                                               |
| ------------------- | -------------------------------------------------------------------------------------------- |
| No telemetry        | No analytics, crash reporting, or usage tracking pipeline                                    |
| Local-first STT     | Vosk runs locally through the bundled worker by default; audio stays on the machine          |
| Opt-in cloud AI     | AI candidate ranking is disabled by default and requires an operator-supplied key            |
| Bundled SQLite      | `rusqlite` with the `bundled` feature avoids system SQLite dependency drift                  |
| Remote control      | Loopback-only by default (`127.0.0.1`); HTTP control endpoints require bearer token auth     |
| CSP enforcement     | No inline scripts or eval; external origins limited to YouTube embed and Supabase licensing  |
| Secrets storage     | API keys and tokens stored with the Rust `keyring` crate using OS keychain facilities        |
| Backend-only egress | STT and AI provider calls are made from Rust; those hosts are not reachable from the WebView |
| Dynamic NDI loading | NDI SDK loaded dynamically; app can run without NDI installed                                |

### Content Security Policy

Authoritative source: `app.security.csp` in `src-tauri/tauri.conf.json`.

```text
default-src 'self';
script-src 'self' https://www.youtube.com https://www.youtube-nocookie.com;
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob: https://i.ytimg.com;
font-src 'self' data:;
connect-src 'self' https://pdpigafulitwdzbwzelb.supabase.co;
media-src 'self' blob: asset: http://asset.localhost https:;
worker-src 'self';
frame-src https://www.youtube-nocookie.com;
frame-ancestors 'none';
object-src 'none';
base-uri 'self';
form-action 'self';
manifest-src 'self'
```

- `script-src` prevents inline scripts and `eval`. It permits YouTube's
  player script, which backs the in-app video embed feature.
- `style-src 'self' 'unsafe-inline'` is required by the current React/Tailwind/Radix styling path.
- `connect-src` limits WebView-originated network calls to the app itself
  and the Supabase endpoint used for account and licence checks. **No STT
  provider and no AI provider appears in `connect-src`**, so those calls
  cannot be made from browser-side JavaScript; they are issued from Rust.
- `frame-src` permits only the privacy-preserving `youtube-nocookie.com`
  origin for embedded video; `frame-ancestors 'none'` prevents the app
  itself from being embedded.
- `media-src` includes the Tauri `asset:` scheme so locally imported media
  can play, plus `https:` for remote media a user deliberately adds.

The YouTube and Supabase entries widen the policy beyond a pure `'self'`
baseline. They are the deliberate cost of the video-embed and licensing
features, and they are the full set of external origins the WebView may
contact.

### Credential handling

All third-party credentials follow the same pattern, with no exceptions:

1. The operator enters the key in Settings; it is passed once over the Tauri
   IPC boundary to the Rust backend.
2. Rust stores it via the `keyring` crate under the `sabbathcue` service.
3. Every later use reads the key **inside Rust**. The key is never returned
   to the WebView, never written to `settings.json`, and never embedded in
   the shipped binary.
4. The frontend only ever observes a boolean "is a key configured" flag,
   resolved from the keychain at startup rather than from persisted
   settings, so a tampered settings file cannot fake credential presence.
5. Keys can be removed from the same settings screen at any time.

This applies to the Deepgram, Soniox, Speechmatics, and DeepSeek keys, and
to the locally generated remote-control bearer token.

### Third-party API access

Outbound calls to STT providers and to the optional AI ranking provider are
issued from the Rust backend, not from browser-side JavaScript. Because
neither provider's host appears in `connect-src`, a content-injection bug in
the WebView could not reach them directly, and stored credentials are never
present in the WebView to be read in the first place.

The optional AI candidate ranking feature applies further containment:

| Control                | Implementation                                                                                                 |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- |
| Disabled by default    | Toggle defaults off and cannot be enabled without a stored key                                                 |
| Bounded payload        | One transcript phrase capped at 500 characters, plus at most 5 candidate summaries                             |
| Bounded response       | The model returns a single character; it cannot emit scripture or arbitrary text                               |
| Output validated       | Any value outside the supplied candidate set is treated as an abstention                                       |
| No autonomous action   | Results are advisory only and never change what is sent to the projector                                       |
| Prompt-injection guard | Transcript is passed as quoted data and the system prompt instructs the model to ignore instructions inside it |
| Bounded failure        | Hard 1800 ms deadline, no retries, and a circuit breaker after 3 consecutive failures                          |

Because the model is constrained to choosing among references SabbathCue
already found locally, and the displayed text is always read from the local
Bible database, a model error or a hostile response cannot place fabricated
scripture on the live screen.

See [privacy-data-flow.md](privacy-data-flow.md) for the corresponding data
inventory, retention posture, and the provider's stated data-handling terms.

### Remote control defaults

- **OSC listener**: binds `127.0.0.1:8000` by default.
- **HTTP listener**: binds `127.0.0.1:8080` by default.
- **HTTP auth**: private HTTP endpoints require `Authorization: Bearer <token>`.
- **Token storage**: the HTTP bearer token is generated locally, stored in the OS keychain, and rotatable from Settings -> Remote.

LAN exposure is not currently enabled through the UI. Any future LAN mode should be an explicit opt-in feature with authentication, clear operator warnings, and firewall guidance.

### Vulnerability reporting

See [.github/SECURITY.md](https://github.com/Bongisto/SabbathCue/blob/main/.github/SECURITY.md) for the responsible disclosure process.
