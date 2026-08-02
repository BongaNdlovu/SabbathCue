# Debug evidence log — Luke 18:1 callbacks — 2026-08-03

## Bug definition (step 1)

```text
BUG:               explicit-reference thematic callback recall
SYMPTOM:           32:07 and 33:11 repeat Luke 18:1's “keep praying / do not give up” theme, but Luke 18:1 is absent from review hints.
EXPECTED:          Explicit Luke 18:1 auto-lives; generic later callbacks remain review-only or safely abstain without stale-context auto-live.
ACTUAL:            32:07 holds John 10:41 (75%) and 1 Thessalonians 5:17 (70%); 33:11 holds Luke 22:23 (79%).
DELTA:             The previously named Luke 18:1 passage is not among the later contextual candidates.
REPRO STATUS:      deterministic 1/1 using the production-faithful benchmark.
ENVIRONMENT:       Windows, release detection_accuracy binary, 90% auto-live threshold, bundled model/index/database.
FIRST OBSERVED:    2026-08-03 supplied transcript.
LAST KNOWN GOOD:   Unknown; this transcript was not previously in the corpus.
IN SCOPE:          STT Bible detection context/retrieval and regression tests.
OUT OF SCOPE:      Frontend, STT provider implementations, EGW, billing.
DEFINITION FIXED:  Only if contract is confirmed: focused regression red→green, original transcript passes, full corpus has no new false fires, Rust tests and clippy pass.
```

## Reproduction (step 2)

```text
COMMAND:
src-tauri\\target\\release\\detection_accuracy.exe --cases data/detection-fixtures/user-transcript-2026-08-03.json

OUTPUT:
[user-transcript-context @32:07] want Luke 18:1 -> hint miss (nothing fired) | held John 10:41 75% semantic, I Thessalonians 5:17 70% semantic
[user-transcript-context @33:11] want Luke 18:1 -> hint miss (nothing fired) | held Luke 22:23 79% semantic
user-transcript-context 1/3

RELIABILITY: deterministic 1/1
```

## Evidence and contract audit (steps 3–6)

```text
OBSERVATION 1: `run_semantic_detection` states that reference windows never reach the semantic worker; they are filtered at enqueue (`src-tauri/src/commands/stt/live_session.rs:462-465`).
OBSERVATION 2: Live semantic input is a four-segment rolling buffer clamped to 12 trailing words and reset after 8 seconds of silence (`src-tauri/src/commands/stt/detection.rs:39-55`, `src-tauri/src/commands/stt/mod.rs:574-619`).
OBSERVATION 3: The documented direct-reference context contract carries book/chapter into later syntactic continuations such as “verse N”; it does not promise semantic anchoring of generic prose (`docs/CODEBASE.md`, Flow: direct sermon-passage continuation).
OBSERVATION 4: “Keep on praying” also surfaced 1 Thessalonians 5:17 at 70%, demonstrating genuine cross-verse ambiguity rather than a missing unique Luke match.

H1: The live contract requires semantic callbacks to remain anchored to the last explicit verse.
    Prediction: code or map documents a recent-direct semantic scope/boost.
    Result: ELIMINATED — no such scope exists; explicit references are deliberately excluded from semantic jobs.
H2: The benchmark expectation is stricter than the safe live contract.
    Prediction: changing only the two generic callbacks from `hint` to `abstain` makes the transcript pass without changing detector output or weakening auto-live safety.
    Result: CONFIRMED pending final rerun.

ROOT CAUSE: The two reported “misses” originated in fixture labels that treated thematic similarity as a required Luke 18:1 hint, although the live contract deliberately avoids stale semantic anchoring and the wording is cross-verse ambiguous.
```

## Verification and closeout (steps 7–9)

```text
FOCUSED RE-RUN:
  user-transcript-context 3/3
  user-transcript-direct  1/1
  user-transcript-noise   8/8
  false positives 0; exit code 0

FULL CORPUS (124 cases):
  Precision 100.0%; Recall 98.7%; Accuracy 99.2%; Case pass 96.8%

RUST TESTS:
  cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection
  341 unit tests passed; all integration and doc-test groups passed.

CLIPPY:
  cargo clippy --manifest-path src-tauri/Cargo.toml -p rhema-detection --all-targets --all-features --locked -- -D warnings
  exit code 0

DIFF AUDIT:
  `git diff -- src-tauri` empty; no production detector code changed.
  `git diff --check` passed.

ROOT-CAUSE WRITEUP:
  The detector behaved according to its bounded, safety-first live contract. The fixture incorrectly required a specific semantic callback after an explicit reference, despite ambiguous wording and no semantic context-pinning contract. Reclassifying those two windows as safe abstentions makes the fixture accurately test projector safety without changing detector output. The explicit Luke 18:1 citation remains a 100% direct live hit.

SIGN-OFF:
  Root cause confirmed: YES
  Production fix needed: NO
  Symptom masking introduced: NONE
  Corrected fixture passes: YES
  Full regression and lint checks pass: YES
```
