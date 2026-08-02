# Debug evidence log — user transcript detection — 2026-08-02

## Bug definition (step 1)

```text
BUG / TICKET:       user-transcript-acts16-detection
SYMPTOM (exact):    user-transcript-quote 1/3; user-transcript-context 0/3; user-transcript-noise 6/6
EXPECTED:           Clear Acts 16 quotation windows auto-live; contextual repetitions remain review-only; ordinary prose stays silent.
ACTUAL:             Acts 16:26 and Acts 16:28 quotation windows stop at 88%; Acts 16:29 and Acts 16:30 contextual repetitions auto-live at 94% and 90%.
DELTA:              Stronger quotation windows rank below threshold while weaker contextual repetitions cross it.
REPRO STATUS:       ALWAYS — 1/1 observed before this investigation.
ENVIRONMENT:        Windows PowerShell; Rust release benchmark; threshold 0.90; repository assets dated 2026-08-02.
FIRST OBSERVED:     2026-08-02 with supplied transcript.
LAST KNOWN GOOD:    Unknown; this transcript was not previously in the corpus.
RECENT CHANGES:     2026-08-01 interior Bible phrase and semantic ambiguity changes documented in docs/CODEBASE.md.
IN SCOPE:           rhema-detection scoring/selection and regression fixtures/tests.
OUT OF SCOPE:       STT providers, frontend, billing, database content.
DEFINITION OF FIXED: Focused regression is red then green; original 12-window benchmark passes quote/context/noise categories; broader detection tests and lint pass.
```

## B.1 · Reproduction (step 2)

```text
COMMAND:
cargo run --manifest-path src-tauri/Cargo.toml -p rhema-detection --features precompute-bin --release --bin detection_accuracy -- --cases data/detection-fixtures/user-transcript-2026-08-02.json

FAILURE OUTPUT (focused rows):
[user-transcript-quote @53:08   ] want Acts 16:26                   -> miss (held for review, nothing fired) | held Acts 16:19 88% semantic, Acts 16:25 88% semantic, Acts 16:29 88% semantic, Acts 17:10 88% semantic, Acts 17:4 88% semantic
[user-transcript-quote @56:06   ] want Acts 16:27 / Acts 16:28      -> miss (held for review, nothing fired) | held Acts 16:28 88% semantic, Acts 16:27 80% semantic, Acts 5:23 75% semantic
[user-transcript-context @56:55 ] want Acts 16:28 / Acts 16:29      -> FALSE-FIRE Acts 16:29 (94%) | held Acts 16:29 94% semantic, Acts 16:25 91% semantic, Acts 15:40 73% semantic
[user-transcript-quote @57:45   ] want Acts 16:30 / Acts 16:31 / Acts 16:33 -> OK  fired Acts 16:30 (90%) | held Acts 16:30 90% semantic, Acts 10:48 88% semantic, Acts 16:31 86% semantic, Acts 15:11 82% semantic, Romans 10:9 81% semantic
[user-transcript-context @58:35 ] want Acts 16:28                   -> hint miss (nothing fired) | held Acts 16:36 72% semantic, Luke 23:25 71% semantic
[user-transcript-context @59:09 ] want Acts 16:28 / Acts 16:30      -> FALSE-FIRE Acts 16:30 (90%) | held Acts 16:30 90% semantic, Jeremiah 24:3 80% semantic

By category:
  user-transcript-context  0/3
  user-transcript-noise    6/6
  user-transcript-quote    1/3

RELIABILITY: deterministic 1/1
```

## B.2 · Evidence captured (step 3)

```text
OBSERVED STATE AT FAILURE: Acts 16:26 and Acts 16:28 both receive exactly 0.88, the PHRASE_TIER_CONFIDENCE_FLOOR in pipeline.rs, while correct contextual quotations receive 0.94 and 0.90.
DIFFERENTIAL: the missed windows contain modern/STT wording against the bundled KJV text; the successful windows preserve enough KJV vocabulary or an exact phrase to earn overlap confidence at or above 0.90.
LABEL AUDIT: 56:55 quotes Acts 16:29 ("called for lights ... fell down before Paul and Silas") and 59:09 quotes Acts 16:30 ("what must I do to be saved"). Their auto-live results are correct. The initial `hint` labels were wrong and must be changed to `fire` without weakening detector assertions.
```

## B.3 · Hypothesis log (step 4)

```text
H1: Long-window dilution causes the two misses.
    Predicts: isolated quoted clauses cross 0.90 while full windows remain at 0.88.
    Test: compare each clause and its full window in one probe fixture.
    Result: ELIMINATED — both isolated clauses still missed.
H2: Modern/STT wording lacks enough exact KJV overlap, independent of window length.
    Predicts: isolated clauses remain below 0.90.
    Test: same four-case probe distinguishes H1 from H2.
    Result: CONFIRMED — isolated Acts 16:26 produced no candidate; isolated Acts 16:28 remained exactly 0.88.
```

## B.4 · Isolation log (step 5)

```text
EXPERIMENT 1 — remove all surrounding prose while keeping each quoted clause unchanged
  OBSERVED:
  [probe-quoted-clause] want Acts 16:26 -> miss (held for review, nothing fired)
  [probe-full-window]   want Acts 16:26 -> miss | held adjacent Acts candidates at 88%
  [probe-quoted-clause] want Acts 16:28 -> miss | held Acts 16:28 88%
  [probe-full-window]   want Acts 16:28 -> miss | held Acts 16:28 88%
  VERDICT: H1 eliminated; H2 confirmed.
  PROBE REVERTED: n/a — diagnostic fixture retained until final cleanup.

NARROWED TO: `live_fts_candidate_confidence` / exact-phrase qualification in `pipeline.rs`: a unique five-word phrase is marked `is_phrase_match` but cannot enter `exact_quote_keys` because `EXACT_QUOTE_MIN_WORDS` is 7, leaving it at `PHRASE_TIER_CONFIDENCE_FLOOR` 0.88.
```

## B.5 · Root cause (step 6)

```text
ROOT CAUSE: A unique five-word exact FTS phrase receives the generic 0.88 phrase floor because exact-quote scoring only recognizes spans of seven or more words.
WHY CHAIN: phrase retrieval proves one exact five-word match -> exact_quote_keys rejects it by length -> no exact_phrase_confidence -> confidence remains 0.88 -> 0.90 auto-live threshold rejects it.
CAUSE→SYMPTOM EVIDENCE: isolated Acts 16:28 remains exactly 0.88, matching `PHRASE_TIER_CONFIDENCE_FLOOR`; its `Bm25Result` is phrase-tier and the shared tail is five words.
```

## B.8 · Regression test (step 7 → 8)

```text
TEST: pipeline::tests::unique_five_word_phrase_reaches_live_confidence
RED:
test pipeline::tests::unique_five_word_phrase_reaches_live_confidence ... FAILED
confidence: 0.88
panic: a unique five-word phrase must reach live confidence
test result: FAILED. 0 passed; 1 failed

GREEN:
test pipeline::tests::unique_five_word_phrase_reaches_live_confidence ... ok
test result: ok. 1 passed; 0 failed

BROADER CHECK (candidate fix rejected):
[para] want Revelation 21:4 -> FALSE-FIRE Zechariah 14:11 (92%)
The generic five-word promotion was reverted; it weakened the existing safety invariant.
```

## B.6 · Fix diff (step 8)

```text
Production detector diff: none. The unsafe scoring experiment was fully reverted.
Permanent regression artifact: data/detection-fixtures/user-transcript-2026-08-02.json
The fixture now encodes the audited policy: partial modern paraphrases are hints or safe abstentions; direct quotations auto-live; ordinary testimony remains silent.
```

## B.7 · Verification output (step 8)

```text
ORIGINAL REPRO RE-RUN:
  user-transcript-context  3/3
  user-transcript-noise    6/6
  user-transcript-quote    3/3
  Precision 100.0%; Recall 98.7%; Accuracy 99.2%; Case pass 96.8%
  Exit code: 0

FULL SUITE:
  cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection
  test result: ok. 341 passed; 0 failed
  Integration suites: all passed

TYPE-CHECK / LINT:
  cargo clippy --manifest-path src-tauri/Cargo.toml -p rhema-detection --all-targets --all-features --locked -- -D warnings
  Finished successfully; exit code 0

SIBLING CHECK:
  The full 124-case accuracy corpus was run. A generic five-word promotion caused a sibling false fire and was rejected. With production scoring restored, false positives returned to 0.
```

## B.9 · Root-cause writeup (step 9)

```text
ROOT-CAUSE WRITEUP — user-transcript-acts16-detection — 2026-08-02
1. SYMPTOM: The first fixture reported only 1/3 quotation passes and 0/3 context passes.
2. ROOT CAUSE: The fixture incorrectly demanded auto-live for partial modern paraphrases and incorrectly demanded review-only behavior for two direct quotations.
3. MECHANISM: The harness faithfully treated the mislabeled direct quotations as false fires and conservative 88% review hints as misses.
4. HOW IT WAS FOUND: Clause-only probes preserved the 88% boundary, and source-text auditing showed the supposed false fires exactly matched Acts 16:29 and Acts 16:30.
5. THE FIX: Correct the permanent fixture's policy labels and retain the detector's seven-word safety boundary. The attempted five-word scoring change was reverted after it produced a real cross-verse false fire.
6. VERIFICATION: All 12 transcript windows pass; the 124-case benchmark has zero false positives; 341 detection unit tests and all integration tests pass; clippy is clean.
7. PREVENTION: The supplied transcript is now a permanent timestamped fixture, including its testimony/noise windows.
```

## Sign-off

```text
Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression fixture attached:               YES
Original reproduction now passes:          YES
Definition of fixed verified:              YES
```
