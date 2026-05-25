# SabbathCue Release Security Checklist

**Version:** 0.1.3  
**Last updated:** 2026-05-24  

Check each item before tagging a release. CI automates the core checks; manual
verification remains required for claims that cannot be proven by a test runner.

## Automated CI gates

- [ ] `bun run typecheck` - TypeScript typecheck passes
- [ ] `bun run lint` - App ESLint passes with no errors
- [ ] `bun run test -- --run` or `bun run test:unit` - Vitest unit test suite passes
- [ ] `bun run build` - Tauri frontend build succeeds
- [ ] `bun run web:lint` - Web documentation lint runs; any warnings are reviewed
- [ ] `bun run web:build` - Web documentation build succeeds
- [ ] `cargo check --workspace` - Rust workspace compiles
- [ ] `cargo test --workspace` - Rust test suite passes
- [ ] `cargo clippy --workspace --all-targets` - Rust Clippy runs; warnings are reviewed and tracked
- [ ] `npm audit --audit-level=moderate` - No moderate+ npm vulnerabilities
- [ ] `cargo deny check` - Cargo dependency license/security policy check passes in CI

## Release artifact evidence

- [ ] Windows installer built and uploaded as CI artifact
- [ ] SHA-256 checksums generated for all release artifacts
- [ ] CycloneDX SBOM JSON published alongside release
- [ ] Release notes include security-relevant changes
- [ ] Known Clippy warnings are either resolved or documented before procurement distribution

## Manual verification

- [ ] CSP has not been weakened (check `src-tauri/tauri.conf.json`)
- [ ] No new Tauri capabilities granted (check `src-tauri/capabilities/`)
- [ ] No new outbound network calls from Rust except documented setup/STT flows
- [ ] Remote control defaults remain loopback-only with token auth for HTTP control endpoints
- [ ] No new GPL or AGPL dependencies without explicit legal review
- [ ] Vosk default mode is verse-focused constrained grammar (verified via `--grammar-json` in spawn)
- [ ] Deepgram API key handling remains OS-keychain backed and not logged

## Procurement artifact bundle

Each procurement release should produce:

1. Windows installer (`.msi` or `.exe`)
2. SHA-256 checksum file
3. CycloneDX SBOM JSON
4. Dependency audit evidence (`cargo deny check`, `npm audit`)
5. Release notes

## Tools required

```bash
cargo install cargo-deny
cargo install cargo-cyclonedx
```

Automated steps are configured in `.github/workflows/desktop-ci.yml` and
`.github/workflows/release-desktop.yml`.
