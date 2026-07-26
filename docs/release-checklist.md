# SabbathCue Desktop Release Checklist

## Free Public Installer Rules

- [ ] Installer includes only public-domain/default redistributable Bible translations.
- [ ] Installer does not include NIV, ESV, NASB, NKJV, NLT, or AMP unless written redistribution permission exists.
- [ ] Installer does not include NDI SDK/runtime binaries.
- [ ] App launches without `.env`.
- [ ] Default STT provider is local Vosk.
- [ ] Deepgram is labeled optional paid cloud mode.
- [ ] HTTP and OSC remote control bind to localhost by default.
- [ ] Packaged app is tested outside the source repository.

## Build

Run these from the repo root unless noted:

- [ ] `bun install`
- [ ] `bun run build:bible:public`
- [ ] `bun run verify:bible:public`
- [ ] Confirm the packaged Vosk model and worker are present.
- [ ] (Optional) Confirm LibreOffice is available for PowerPoint import testing.
- [ ] `bun run download:model`
- [ ] `bun run export:verses`
- [ ] `bun run precompute:embeddings`
- [ ] `bun run quantize:embeddings`
- [ ] `bun run compare:embeddings`
- [ ] `bun run typecheck`
- [ ] `bun run test -- --run`
- [ ] `bun run build`
- [ ] `cd web && bun install && cd ..` (required before `web:build`)
- [ ] `bun run web:build`

Run these from `src-tauri/`:

- [ ] `cargo check --workspace`
- [ ] `cargo test --workspace`

If PowerShell cannot find `bun` or `cargo`, restart the terminal after installing the toolchain and confirm:

- [ ] `where.exe bun`
- [ ] `where.exe cargo`

If PowerShell blocks `npm`, use `npm.cmd` explicitly.

- [ ] `bun run tauri:build:release`

## Clean Machine Smoke Test

- [ ] App opens.
- [ ] Bible translations load.
- [ ] Manual search works.
- [ ] Preview changes when selecting verses.
- [ ] Live output only changes when presenting.
- [ ] Settings persist after restart.
- [ ] Vosk missing-model/worker state is clear.
- [ ] Deepgram key is optional.
- [ ] HTTP remote starts on localhost.
- [ ] NDI missing state is clear and non-fatal.

## Known v0.1 Limitations

- [ ] In-app model download is not implemented yet. Local Vosk works after the model is placed in the resolved app data or development model directory (`bun run download:vosk`).
- [ ] Semantic search remains optional and disabled when ONNX/tokenizer/embedding files are missing.
- [ ] NDI remains optional and user-installed; SabbathCue must not ship NDI SDK/runtime binaries in the public installer.
