# SabbathCue Desktop Release Checklist

## Free Public Installer Rules

- [ ] Installer includes only public-domain/default redistributable Bible translations.
- [ ] Installer does not include NIV, ESV, NASB, NKJV, NLT, or AMP unless written redistribution permission exists.
- [ ] Installer does not include NDI SDK/runtime binaries.
- [ ] App launches without `.env`.
- [ ] Default STT provider is local Whisper.
- [ ] Deepgram is labeled optional paid cloud mode.
- [ ] HTTP and OSC remote control bind to localhost by default.
- [ ] Packaged app is tested outside the source repository.

## Build

Run these from the repo root unless noted:

- [ ] `bun install`
- [ ] `bun run build:bible:public`
- [ ] `bun run verify:bible:public`
- [ ] `bun run download:whisper`
- [ ] `bun run typecheck`
- [ ] `bun run test -- --run`
- [ ] `bun run build`

Run these from `src-tauri/`:

- [ ] `cargo check --workspace`
- [ ] `cargo test --workspace`

If PowerShell cannot find `bun` or `cargo`, restart the terminal after installing the toolchain and confirm:

- [ ] `where.exe bun`
- [ ] `where.exe cargo`

If PowerShell blocks `npm`, use `npm.cmd` explicitly.

- [ ] `bun run tauri build`

## Clean Machine Smoke Test

- [ ] App opens.
- [ ] Bible translations load.
- [ ] Manual search works.
- [ ] Preview changes when selecting verses.
- [ ] Live output only changes when presenting.
- [ ] Settings persist after restart.
- [ ] Whisper missing state is clear.
- [ ] Deepgram key is optional.
- [ ] HTTP remote starts on localhost.
- [ ] NDI missing state is clear and non-fatal.

## Known v0.1 Limitations

- [ ] In-app model download is not implemented yet. Local Whisper works after the model is placed in the resolved app data or development model directory.
- [ ] Semantic search remains optional and disabled when ONNX/tokenizer/embedding files are missing.
- [ ] NDI remains optional and user-installed; SabbathCue must not ship NDI SDK/runtime binaries in the public installer.
