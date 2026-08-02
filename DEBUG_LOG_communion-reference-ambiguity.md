# Debug evidence log — communion reference ambiguity — 2026-08-02

## Bug definition and reproduction

```text
SYMPTOM: At 01:15:06, correct 1 Corinthians 11:23 wins, but Matthew 11:23 appears at 100% and 1 Corinthians 11:1 at 92%.
EXPECTED: Matthew 26 may remain a valid chapter-only mention; 1 Corinthians 11:23 is emitted; Matthew must not consume later Corinthians numbers; the refined Corinthians reference supersedes its same-chapter placeholder.
ACTUAL: [communion-direct @01:15:06] held 1 Corinthians 11:23 100%, Matthew 11:23 100%, 1 Corinthians 11:1 92%.
REPRO: deterministic 2/2 with `detection_accuracy.exe --cases data/detection-fixtures/user-transcript-communion-2026-08-02.json`.
IN SCOPE: `rhema-detection` direct detector/parser and regression tests.
DEFINITION FIXED: focused test red→green; transcript no longer contains the two bad alternatives; full benchmark, tests, and clippy pass.
```

## Evidence, hypotheses, and root cause

```text
OBSERVED: `DirectDetector::detect` iterates every `BookMatch` but passes the full transcript to `parser::parse_reference` each time (`detector.rs:1116-1143`).
OBSERVED: `parse_reference` tokenizes every character after the selected book through end-of-text (`parser.rs:18-23`).
OBSERVED: chapter-only detections are pushed immediately and no post-pass removes one when a later full same-book/chapter citation exists (`detector.rs:1158-1224`).

H1: Matthew consumes the later Corinthians chapter/verse because parsing is not bounded at the next book match.
    Prediction: bounding the first book's parse text before the next book leaves Matthew 26 chapter-only and removes Matthew 11:23.
    Result: CONFIRMED by source trace; regression test will lock behavior.
H2: 1 Corinthians 11:1 is a stale chapter placeholder retained after same-fragment refinement.
    Prediction: removing chapter-only results only when a full same-book/chapter result exists removes 11:1 without harming standalone chapter navigation.
    Result: CONFIRMED by source trace; existing chapter-only tests guard standalone behavior.

ROOT CAUSE: Book parsing has no next-book boundary, and the detector lacks same-fragment chapter-placeholder refinement.
```

## Regression test RED

```text
test direct::detector::tests::later_book_reference_does_not_rewrite_earlier_book_and_refines_chapter_placeholder ... FAILED
left:  [("Matthew", 11, 23), ("1 Corinthians", 11, 23)]
right: [("Matthew", 26, 1), ("1 Corinthians", 11, 23)]
The failure is the production defect, not test setup: Matthew consumed the later chapter/verse.
```

## Fix, verification, and closeout

```text
FIX:
1. Bound each book's parser input at the next book match.
2. After the fragment pass, remove a chapter-only result when a full result for the same book/chapter exists.

REGRESSION GREEN:
test direct::detector::tests::later_book_reference_does_not_rewrite_earlier_book_and_refines_chapter_placeholder ... ok
test result: ok. 1 passed; 0 failed

ORIGINAL REPRO AFTER FIX:
[communion-direct @01:15:06] fired 1 Corinthians 11:23 (100%) | held 1 Corinthians 11:23 100% direct, Matthew 26:1 92% direct, Acts 21:13 72% semantic
Matthew 11:23 and 1 Corinthians 11:1 are absent. Matthew 26 remains because the speaker explicitly named that chapter.

FULL CORPUS:
125 cases; precision 100.0%; recall 98.7%; accuracy 99.2%; false positives 0.

FULL TESTS:
cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection
342 unit tests passed; all integration and doc-test groups passed.

CLIPPY:
cargo clippy --manifest-path src-tauri/Cargo.toml -p rhema-detection --all-targets --all-features --locked -- -D warnings
exit code 0.

ROOT-CAUSE WRITEUP:
The parser tokenized the entire suffix after every book match, allowing Matthew to consume a later Corinthians chapter/verse. It also retained a temporary chapter-start detection after a later full citation refined the same book/chapter. Bounding each parse at the next book and pruning only refined same-chapter placeholders removes both causes while preserving legitimate chapter-only references and multi-reference speech.

SIGN-OFF:
Root cause confirmed: YES
Regression red→green: YES
Original reproduction passes: YES
Symptom masking: NONE
Full verification passes: YES
```
