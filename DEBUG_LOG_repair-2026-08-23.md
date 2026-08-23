# Repair evidence log — 2026-08-23

This append-only log records the reproduce-first repair pass for the eight findings from the 2026-08-23 soundness audit.

## Initial worktree receipt

Command:

```text
git status --short --branch
```

Observed:

```text
## main...origin/main
 M src-tauri/crates/detection/src/pipeline.rs
 M src-tauri/crates/detection/src/presentation.rs
 M src-tauri/src/commands/stt/detection.rs
?? DEBUG_LOG_semantic-quotes-2026-08-23.md
```

The existing uncommitted semantic-quotation changes are preserved. No source fix has been applied at this point.

## Scope

Reported findings under test:

1. Bare “good news” translation false positive.
2. Order-insensitive semantic cache key.
3. Per-dropped-frame thread creation from the audio callback.
4. NDI runtime reference leak on invalid source names.
5. Reconnect budget reset during slow failed connection attempts.
6. Late Vosk finals accepted after disconnect.
7. Speechmatics finishing timeout reset by each frame.
8. Speechmatics coalescing cap based only on word metadata.

## Red-to-green reproduction record

The first test run was intentionally against the uncorrected behavior.

```text
cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection sermon_prose_containing_good_news
test ...::sermon_prose_containing_good_news ... FAILED
thread ... panicked ... input "the good news"

cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection cache_does_not_reuse_results_for_reordered_text
test ...::cache_does_not_reuse_results_for_reordered_text ... FAILED
left:  [(14643, 1.0)]
right: [(14643, 1.0)]

npm.cmd run test:unit -- src/stores/transcript-store.test.ts src/hooks/use-transcription.test.ts
2 failed: metadata-only Speechmatics expected 2 segments but got 1;
late final expected [] but got one segment
```

The first broad translation guard also failed nine pre-existing translation-command tests. It was narrowed to the confirmed ambiguous lead-in case and the complete detection suite then passed.

## Green verification record

```text
cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection
407 passed; 0 failed; detection integration tests passed

cargo test --manifest-path src-tauri/Cargo.toml -p rhema-audio
13 passed; 0 failed

cargo test --manifest-path src-tauri/Cargo.toml -p rhema-broadcast
8 passed; 0 failed

cargo test --manifest-path src-tauri/Cargo.toml -p rhema-stt --all-targets
54 tests: 53 passed; 0 failed; 1 ignored (local Vosk preflight)

npm.cmd run test:unit -- src/stores/transcript-store.test.ts src/hooks/use-transcription.test.ts
33 passed; 0 failed

npm.cmd run typecheck
passed

npm.cmd run lint
passed

cargo test --manifest-path src-tauri/Cargo.toml --workspace --all-targets
passed; application and workspace test targets completed successfully

cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings
Finished successfully
```

The full frontend suite separately reported `1362 passed, 1 skipped, 1 failed`; the sole failure was the unrelated live Paddle sandbox test, which could not fetch because this environment denied outbound network access (`fetch failed`, `EACCES`).

## Static confirmation of repaired paths

The first clippy-safe cache regression fixture used a threshold that mapped both lowercase test inputs to the same synthetic result. That test failed with `left: [(14644, 1.0)]` and `right: [(14644, 1.0)]`; the fixture threshold was corrected so the two order-sensitive inputs produce distinct synthetic index results. The targeted ensemble suite then passed 6/6.

The final browser quality run also passed:

```text
npm.cmd run test:e2e
build completed; 9 passed; 0 failed
```

The post-fix search found no `audio-drop-log` thread, no sorted-token cache construction, no reconnect `connection_started` timing in the affected providers, no per-frame `FINISH_DRAIN_TIMEOUT`, and no word-metadata-only coalescing expression. The remaining `cache_key(text)` and `CString::new(source_name)` occurrences are the corrected exact-text key and pre-acquisition validation helper respectively.
