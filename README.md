<p align="center">
  <img src="public/sabbathcue.svg" alt="SabbathCue logo" width="160" height="160" />
</p>

<h1 align="center">SabbathCue</h1>

<p align="center">Real-time AI-powered Bible verse detection for live sermons and broadcasts. A Tauri v2 desktop app with a React frontend and Rust backend.</p>

SabbathCue listens to a live sermon audio feed, transcribes speech in real time, detects Bible verse references (both explicit citations and quoted passages), and renders them as broadcast-ready overlays via NDI for live production.

## Free Desktop Distribution

SabbathCue's public desktop installer is local-first and free to operate. It ships
with redistributable Bible content only, defaults to local Vosk speech-to-text,
does not require Deepgram, and does not bundle NDI SDK binaries. Deepgram,
Soniox, and NDI remain optional integrations that users configure separately.

### Building for Public Release

To build the public release version with only redistributable content:

```bash
bun run build:bible:public
bun run download:vosk
bun run build:vosk-sidecar
bun run download:model
bun run export:verses
bun run precompute:embeddings
bun run tauri:build:release
```

This creates an installer that includes only public-domain Bible translations
(KJV, WEB, Reina-Valera 1909, J.N. Darby French 1885, Biblia Livre) and defaults
to local Vosk speech recognition with the model and self-contained worker bundled.
Local builds create an unsigned NSIS installer; official release CI uses
`bun run tauri:build:release` with updater signing secrets.

## Features

- **Real-time speech-to-text** via local Vosk or cloud Deepgram/Soniox/Speechmatics
  - Vosk runs locally with no API costs; cloud providers stream through their live APIs
- **Voice-controlled translation switching** — say "read in NIV" or "switch to ESV" to change translations instantly during a sermon
- **Multi-strategy verse detection**
  - Direct reference parsing (Aho-Corasick automaton + fuzzy matching)
  - Semantic search — MiniLM-L6-v2 ONNX embeddings, brute-force cosine similarity over ~31k verse vectors (the `hnsw_index.rs` file is named after a future plan; today it scans linearly)
  - Quotation matching against known verse text
  - Reading mode — locks to book/chapter as soon as it's mentioned, with voice navigation ("next chapter", "chapter 5")
  - Sermon context tracking and sentence buffering
- **AI candidate ranking (optional, off by default)** — when an indirect reference ("the passage where Paul and Silas sang in prison") leaves several plausible matches, DeepSeek picks the closest one from candidates already found locally. It returns a single choice, never scripture text, and only marks a suggestion badge — it never changes what is projected. Requires your own API key; see [AI candidate ranking](#ai-candidate-ranking-optional)
- **SQLite Bible database** with FTS5 full-text search (BM25 ranking by default)
- **Public-release translations** — KJV, WEB, SpaRV (Spanish), FreJND (French), and PorBLivre (Portuguese) ship in the free public installer. NIV, ESV, NASB, NKJV, NLT, and AMP are supported only for licensed/private builds or user-provided data.
- **Cross-reference lookup** (340k+ refs from openbible.info; the bundled file ships with 344,800 entries)
- **NDI broadcast output** for live production integration — configurable resolution, 24/30/60 fps, and three alpha modes (none, straight, premultiplied)
- **Theme designer** — visual canvas editor for verse overlays with backgrounds (solid, gradient, image, transparent), text styling, positioning, shadows, and outlines
- **Verse queue** with drag-and-drop ordering (`@dnd-kit/react`) and duplicate prevention (flash-highlight on duplicates)
- **Quick navigation** — keyboard-driven verse entry with autocomplete (e.g., type "J" → Joshua, Tab through book → chapter → verse)
- **Fuzzy contextual search** (Fuse.js client-side)
- **Guided Projector Setup** — one-tap "go live on the projector" that remembers last week's display, hot-plug detection (no manual Refresh), on-screen **Identify screens**, and plain-language Duplicate-vs-Extend guidance; reachable from a header status chip
- **Audio level metering** and on-air indicator
- **Interactive onboarding tutorial** — guided tour covering all panels and Projector Setup, auto-launches on first startup
- **Light/dark mode** with system theme detection (light, dark, or follow OS)
- **Settings persistence** — all preferences auto-saved to disk across restarts
- **Cross-platform** — Windows, macOS, and Linux
- **Remote control** via OSC and HTTP API for hardware controllers and automation
  - [Remote control guide](documentation/remote-control.md) — Stream Deck, TouchOSC, REST API integration

## Tech Stack

| Layer         | Technologies                                                      |
| ------------- | ----------------------------------------------------------------- |
| **Frontend**  | React 19, TypeScript, Tailwind CSS v4, shadcn/ui, Zustand, Vite 7 |
| **Backend**   | Tauri v2, Rust (workspace with 6 crates)                          |
| **AI/ML**     | ONNX Runtime (MiniLM-L6-v2 embeddings), Aho-Corasick, Fuse.js     |
| **Database**  | SQLite via rusqlite (bundled) with FTS5                           |
| **Broadcast** | NDI 6 SDK via dynamic loading (libloading FFI)                    |
| **STT**       | Local Vosk worker; Deepgram/Soniox/Speechmatics streaming APIs    |
| **Cloud AI**  | DeepSeek candidate ranking (optional, off by default)             |

### Rust Crates

| Crate             | Purpose                                                                                                               |
| ----------------- | --------------------------------------------------------------------------------------------------------------------- |
| `rhema-audio`     | Audio device enumeration, capture, VAD (cpal)                                                                         |
| `rhema-stt`       | Local Vosk STT plus Deepgram/Soniox/Speechmatics cloud streaming                                                      |
| `rhema-bible`     | SQLite Bible DB, FTS5 search, cross-references                                                                        |
| `rhema-detection` | Verse detection pipeline: direct, semantic, quotation, ensemble merger, sentence buffer, sermon context, reading mode |
| `rhema-broadcast` | NDI video frame output via FFI                                                                                        |
| `rhema-api`       | Tauri command API layer                                                                                               |

## Prerequisites

- [Bun](https://bun.sh/) (runtime for scripts + package manager)
- [Rust](https://rustup.rs/) toolchain (stable, 1.77.2+)
- [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) (platform-specific system dependencies)
- [Python 3](https://www.python.org/) (for downloading copyrighted translations and embedding model export)
- [Deepgram API key](https://deepgram.com/) or Soniox API key (optional, for cloud speech-to-text instead of local STT)

### Platform-specific setup

Follow the official Tauri v2 prerequisites for your operating system. On
Windows, make sure the MSVC C++ build tools are installed before running the
shared setup pipeline.

#### Windows

1. Install [Visual Studio 2022](https://visualstudio.microsoft.com/downloads/) with the **Desktop development with C++** workload (provides MSVC).
2. From the repo root:

   ```powershell
   bun install
   bun run setup:windows
   ```

   This checks for MSVC and prints install guidance if the toolchain is missing.

3. **Close the terminal and open a new one** if you installed new build tools.
4. Continue with [Quick Setup](#quick-setup-recommended) (`bun run setup:all`) and then `bun run tauri dev`.

## Getting Started

```bash
git clone <repo-url>
cd SabbathCue
bun install
```

### Quick Setup (recommended)

One command sets up the development data pipeline: Python virtual environment, Bible data, database, ONNX model, precomputed embeddings, and speech assets:

> **Windows:** run `bun run setup:windows` _before_ `setup:all` and restart your terminal. See [Platform-specific setup](#platform-specific-setup) above.

```bash
bun run setup:all
```

This runs 8 idempotent phases in sequence, skipping any whose output artifacts already exist (pass `--force` to re-run all):

1. Python environment (`.venv` + pip deps: `optimum-onnx[onnxruntime]`, `sentence-transformers`, `accelerate`, `tokenizers`, `numpy`, `torch`, `meaningless`)
2. Download Bible source data - public-release builds include redistributable translations only; private/development builds may include additional licensed translations if the developer has the rights to use them.
3. Build SQLite Bible database (`data/rhema.db` with FTS5 + cross-references)
4. Import EGW books into the SQLite database
5. Download and export the ONNX embedding model plus INT8 quantization
6. Export public-domain multi-vector verses to JSON for embedding precomputation
7. Precompute verse embeddings (GPU sentence-transformers when available, ONNX CPU fallback otherwise)
8. Download local Vosk speech assets into `models/vosk/`

### Environment

#### Speech-to-Text Options

SabbathCue supports four speech-to-text engines. Exactly one is active at a
time, selected under **Settings → Speech Recognition**:

**Option 1: Vosk (Local, Free, Default)**
Vosk runs locally on your machine with no API costs or per-minute billing.

- Development builds can use Python plus `pip install vosk`.
- Public release builds run `bun run build:vosk-sidecar` so the installed app has a self-contained `vosk_worker.exe`.
- The model is fetched with `bun run download:vosk`.

**Option 2: Deepgram (Cloud, Paid)**
The key is stored in the OS keychain, not the settings file.

1. Sign up at [console.deepgram.com](https://console.deepgram.com/signup).
2. In the Console, open the **Projects** dropdown → **Settings** → **API Keys**.
3. Click **Create a New API Key** and copy the secret — it is shown only once.
4. Paste it into the app under **Settings → Speech Recognition** (Deepgram).

**Cost:** new accounts get about **R3,280** in free credit, then pay-as-you-go from about **R0.08/min** for streaming — no subscription, no minimums, no card required. Billed in USD (≈ R16.40/$); current rates at [deepgram.com/pricing](https://deepgram.com/pricing).

**Option 3: Soniox (Cloud, Paid)**
The key is stored in the OS keychain.

1. Sign up at [console.soniox.com](https://console.soniox.com).
2. Create an API key in the Soniox console.
3. Copy the key and paste it into the app under **Settings -> Speech Recognition** (Soniox).

**Cost:** pay-as-you-go streaming; check current rates at [soniox.com/pricing](https://soniox.com/pricing).

**Option 4: Speechmatics (Cloud, Paid — multilingual)**
Supports Afrikaans, English, Spanish, French, Portuguese, and many more. The key is stored in the OS keychain.

1. Create an account or sign in at [portal.speechmatics.com](https://portal.speechmatics.com).
2. Open **API Keys** from your project or account navigation and generate a key named `SabbathCue`.
3. Copy the complete key and keep it private.
4. Paste it into the app under **Settings → Speech Recognition** (Speechmatics), save, then select **Test key**.

**Cost:** a free monthly real-time allowance is currently available; paid usage follows current rates at [speechmatics.com/pricing](https://www.speechmatics.com/pricing).

#### AI candidate ranking (optional)

Off by default. This does **not** affect ordinary reference detection —
"John chapter three verse sixteen" is resolved entirely on-device and makes
no network call. Ranking only engages when an _indirect_ reference leaves
two or more plausible local candidates and no explicit reference has already
cleared your confidence threshold.

1. Create an account at [platform.deepseek.com](https://platform.deepseek.com) and generate an API key.
2. Paste it under **Settings → AI Ranking**, save, then select **Test key**.
3. Switch on **AI candidate ranking**. The toggle stays disabled until a key is stored.

**What is sent:** one transcript phrase (max 500 characters) and up to five
candidate references SabbathCue already found locally. Never the rolling
transcript, never audio. The model replies with a single character choosing
one candidate, so it cannot return scripture text; the verse shown is always
read from the local Bible database. The result is advisory — it marks a
suggestion badge and never changes what is projected.

**Before enabling:** review [docs/procurement/privacy-data-flow.md](docs/procurement/privacy-data-flow.md).
DeepSeek's policy allows submitted input to be used for service improvement
and permits processing in the People's Republic of China, which may not suit
every organisation. Removing the key automatically switches the toggle off.

**Cost:** pay-as-you-go; requests are small (a few hundred input tokens and
at most 4 output tokens each). Current rates at [platform.deepseek.com](https://platform.deepseek.com).

#### Account verification (Supabase)

SabbathCue gates the desktop app behind Supabase email/password auth. Each account may activate up to **two approved computers**. The first computer is approved automatically; later computers wait for approval and can be deactivated by the user or an administrator. First-time verification requires an active network connection. A verified computer receives a signed offline lease for up to **72 hours** before it must reconnect.

Add these variables to your local `.env` file (values from your Supabase project dashboard → Settings → API):

```text
VITE_SUPABASE_URL=https://your-project.supabase.co
VITE_SUPABASE_ANON_KEY=your_anon_key_here
VITE_ACTIVATION_LEASE_PUBLIC_KEY=your_p256_spki_public_key_base64
```

Generate the lease-signing key pair with `node scripts/generate-activation-lease-key.mjs`. Store `ACTIVATION_LEASE_PRIVATE_KEY` only as a Supabase Edge Function secret; put only the generated public key in the desktop build environment. Apply the SQL migrations, then deploy `supabase/functions/device-activation`. The function verifies the installation signature before calling `register_device`, and the RPC enforces the two-approved-computer limit server-side.

**Email confirmation:** If enabled under Authentication → Providers → Email in the Supabase dashboard, sign-up creates the user but returns no session until the confirmation link is clicked. The app shows a “check your email” message in that case.

**E2E / automation bypass:** URLs that include the `e2e` query parameter (for example `/?e2e=1`) skip the verification gate so Playwright specs can reach the dashboard without Supabase credentials.

### NDI SDK (optional)

For broadcast output via NDI:

```bash
bun run download:ndi-sdk
```

### Running individual setup steps

Each phase can also be run independently:

```bash
bun run download:bible-data          # Source translations + cross-refs for local data builds
bun run build:bible:public           # Build redistributable public-release DB only
bun run build:bible                  # Build SQLite database
bun run build:egw                    # Import EGW JSON into data/rhema.db (run after build:bible)
bun run download:model               # Download & export ONNX model
bun run export:verses                # Export public-domain verses to JSON
bun run precompute:embeddings        # Rust ONNX (recommended); see also -onnx and -py variants
bun run download:vosk                # Vosk STT model
bun run build:vosk-sidecar           # Self-contained Vosk worker for release builds
```

### EGW import workflow

The Ellen G. White SQLite import is built from committed JSON sources in
`data/sources/egw`. When rebuilding local or packaged data, use this order:

```bash
bun run convert:egw:pp:pdf <pdf>
bun run convert:egw:sc:pdf <pdf>
bun run convert:egw:da:pdf <pdf>
bun run validate:egw
bun run build:bible
bun run build:egw
```

`build:bible` recreates `data/rhema.db`, so `build:egw` must always run after it.

### Run in development

```bash
bun run tauri dev
```

### Build for production

```bash
bun run tauri:build:release
```

## Project Structure

```text
sabbathcue/
├── src/                          # React frontend
│   ├── components/
│   │   ├── broadcast/            # Theme designer, NDI settings
│   │   ├── controls/             # Transport bar
│   │   ├── layout/               # Dashboard layout
│   │   ├── panels/               # Transcript, preview, live output, queue, search, detections
│   │   └── ui/                   # shadcn/ui + custom components
│   ├── hooks/                    # useAudio, useTranscription, useDetection, useBible, useBroadcast
│   ├── stores/                   # Zustand stores (audio, transcript, bible, queue, detection, broadcast, settings)
│   ├── types/                    # TypeScript type definitions
│   └── lib/                      # Context search (Fuse.js), verse renderer (Canvas 2D), builtin themes
├── src-tauri/                    # Rust backend (Tauri v2)
│   ├── crates/
│   │   ├── audio/                # Audio capture & metering (cpal)
│   │   ├── stt/                  # Deepgram STT (WebSocket + REST)
│   │   ├── bible/                # SQLite Bible DB, search, cross-references
│   │   ├── detection/            # Verse detection pipeline
│   │   │   ├── direct/           # Aho-Corasick + fuzzy reference parsing
│   │   │   └── semantic/         # ONNX embeddings, HNSW index, cloud booster, ensemble
│   │   ├── broadcast/            # NDI output (FFI)
│   │   └── api/                  # Tauri command layer
│   └── tauri.conf.json
├── data/                         # Bible data pipeline
│   ├── prepare-embeddings.ts     # Unified setup orchestrator (bun run setup:all)
│   ├── lib/python-env.ts         # Shared Python venv management utilities
│   ├── download-sources.ts       # Download public domain translations + cross-refs
│   ├── download-biblegateway.py  # Private/licensed translation import helper; not part of public release packaging
│   ├── build-bible-db.ts         # Build SQLite DB from JSON sources
│   ├── compute-embeddings.ts     # Export verses to JSON for embedding
│   ├── precompute-embeddings.py  # Precompute embeddings (GPU auto-detect, ONNX fallback)
│   ├── download-model.ts         # Export & quantize MiniLM ONNX model
│   ├── download-ndi-sdk.ts       # Download NDI SDK libraries
│   └── schema.sql                # Database schema
├── models/                       # ML models (gitignored)
├── embeddings/                   # Precomputed vectors (gitignored)
├── sdk/ndi/                      # NDI SDK files (downloaded)
└── build/                        # Vite build output
```

## Scripts

| Script                                          | Description                                                                                  |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `setup:all`                                     | **Full setup** — runs the data/model/embedding phases (idempotent; pass `--force` to re-run) |
| `setup:windows`                                 | Windows build-tools checker for the MSVC/Tauri toolchain                                     |
| `dev`                                           | Start Vite dev server                                                                        |
| `build`                                         | TypeScript check + Vite production build                                                     |
| `tauri`                                         | Run Tauri CLI commands                                                                       |
| `test`                                          | Run Vitest tests                                                                             |
| `lint`                                          | ESLint                                                                                       |
| `format`                                        | Prettier formatting                                                                          |
| `typecheck`                                     | TypeScript type checking                                                                     |
| `preview`                                       | Preview production build                                                                     |
| `tauri:build:release`                           | Build the signed release bundle configuration used by CI updater artifacts                   |
| `download:bible-data`                           | Download bundled Bible translation archive + cross-references                                |
| `build:bible`                                   | Build SQLite Bible database from JSON sources                                                |
| `download:model`                                | Export all-MiniLM-L6-v2 to ONNX + quantize to INT8                                           |
| `export:verses`                                 | Export public-domain multi-vector verses to JSON for embedding precomputation                |
| `precompute:embeddings`                         | Precompute embeddings via Rust ONNX binary (recommended)                                     |
| `precompute:embeddings-onnx`                    | Precompute embeddings via Python ONNX Runtime                                                |
| `precompute:embeddings-py`                      | Precompute embeddings via Python sentence-transformers (GPU path)                            |
| `download:vosk`                                 | Download the English Vosk model for local STT                                                |
| `build:vosk-sidecar`                            | Build the self-contained Vosk worker executable bundled in public installers                 |
| `download:ndi-sdk`                              | Download NDI 6 SDK headers and platform libraries                                            |
| `web:dev`, `web:build`, `web:start`, `web:lint` | Marketing + Fumadocs documentation site under `web/`                                         |

### Web documentation site (`web/`)

The marketing/docs app under `web/` is a separate Next.js package. Install its dependencies
before running any `web:*` script from the repo root:

```bash
cd web && bun install && cd ..
bun run web:build
```

`web:build` runs `cd web && bun run build` and will fail if `web/node_modules` is missing.

## Security

SabbathCue enforces a restrictive Content Security Policy on the Tauri webview to prevent script injection and unauthorized data exfiltration. The policy is defined in `src-tauri/tauri.conf.json`; see **[SECURITY.md](.github/SECURITY.md)** for the directive-by-directive rationale, threat model, and vulnerability reporting process.

API keys (Deepgram, Soniox, Speechmatics, DeepSeek) are stored in the OS
keychain via the Rust `keyring` crate and are read only by the Rust backend —
they are never returned to the webview, written to `settings.json`, or
embedded in the shipped binary. Calls to those providers are issued from
Rust, and none of their hosts appear in the webview's `connect-src`.

For buyer-facing detail see
[docs/procurement/security-overview.md](docs/procurement/security-overview.md)
and [docs/procurement/privacy-data-flow.md](docs/procurement/privacy-data-flow.md).

## Environment Variables

Create a `.env` file in the project root (optional):

| Variable                           | Required | Description                                                       |
| ---------------------------------- | -------- | ----------------------------------------------------------------- |
| `VITE_SUPABASE_URL`                | Yes      | Supabase project URL                                              |
| `VITE_SUPABASE_ANON_KEY`           | Yes      | Supabase anon key                                                 |
| `VITE_ACTIVATION_LEASE_PUBLIC_KEY` | Yes      | Public P-256 key used to verify 72-hour offline activation leases |
