## Bug definition (step 1)

```
BUG / TICKET:       detection-accuracy-precision-gate
SYMPTOM (exact):    Accuracy gate failed: precision=0.988 recall=1.000 minimum_precision=Some(0.988) minimum_recall=Some(0.8)
EXPECTED:           Hint-only paraphrases remain held for operator review and the 0.988 precision gate passes.
ACTUAL:             Two hint-only cases auto-fire at 92% and 91%, yielding exact precision 162/164 = 0.987804878.
DELTA:              Two automatic presentations violate the benchmark's authored hint policy.
REPRO STATUS:       deterministic 1/1 from the supplied release benchmark output.
ENVIRONMENT:        Windows release detection_accuracy.exe, threshold 0.90, public MiniLM q8 embeddings.
IN SCOPE (may modify): Detection auto-live policy, its benchmark mirror, tests, and CODEBASE map.
OUT OF SCOPE:       Lowering the precision threshold or relabeling hint cases as fire cases.
DEFINITION OF FIXED: The original benchmark command passes at min precision 0.988, the two hint-only cases remain non-live, and regression tests are red then green.
```

## B.1 - Reproduction (step 2)

```
COMMAND:
src-tauri\\target\\release\\detection_accuracy.exe --threshold 0.90 --embeddings embeddings/public-minilm-l6-v2-q8.bin --ids embeddings/public-minilm-l6-v2-q8-ids.bin --min-precision 0.988 --min-recall 0.80

FAILURE OUTPUT (verbatim excerpt from supplied run):
[       quote          ] want Revelation 21:4              -> FALSE-FIRE Revelation of John 21:4 (92%)
[no-condemnation-quote ] want Romans 8:38-39 / Romans 8:38 / Romans 8:39 -> FALSE-FIRE Romans 8:39 (91%)
true positives (correct verse live): 162
false positives (wrong/noise live):  2
Accuracy gate failed: precision=0.988 recall=1.000 minimum_precision=Some(0.988) minimum_recall=Some(0.8)

RELIABILITY: supplied release run failed 1/1; local source reproduction pending.
```

## B.2 - Evidence captured (step 3)

```
OBSERVED STATE: built_in_mode marks "And God shall wipe away all tears from their eyes" as Hint via BUILT_IN_HINT_UTTERANCES (src-tauri/crates/detection/src/bin/detection_accuracy.rs:573).
OBSERVED STATE: the external Romans 8:38-39 fixture explicitly has "mode": "hint" (data/detection-fixtures/sermon-transcript-cases.json:730).
OBSERVED STATE: select_stable_case replays each case twice, and AutoLiveSelector turns any 90%-94% semantic candidate live on the second identical event (src-tauri/crates/detection/src/bin/detection_accuracy.rs:925-986).
DIFFERENTIAL: both false fires are moderate semantic candidates (91%-92%) that gain automatic presentation only through that two-event rule; direct citations are not involved.
```

## B.3 - Hypothesis log (step 4)

```
H1: The two-event confirmation policy lacks evidence-quality discrimination, so it promotes authored hint-only semantic paraphrases.
    Predicts: the same two-event mechanism is present in the frontend live workflow.
    Test: compare AutoLiveSelector with confirmedSemanticHit.
    Result: CONFIRMED. Both use a 0.95 single-pass boundary and auto-select a lower-confidence semantic candidate after the repeated event (src/lib/verse-detection-workflow.ts:306-348).

H2: The benchmark false fires are reference alias mismatches.
    Predicts: ref_eq rejects "Revelation" and "Revelation of John".
    Test: inspect ref_eq normalization.
    Result: ELIMINATED. ref_eq explicitly normalizes "Revelation of John" to "Revelation" (src-tauri/crates/detection/src/bin/detection_accuracy.rs:1195-1218).
```

## B.4 - Isolation / bisection log (step 5)

```
EXPERIMENT 1 - Add an ambiguous 92%/92% semantic pair to the frontend workflow test and benchmark selector test.
OBSERVED: before the policy change, both tests auto-selected the first candidate after the replayed second event.
VERDICT: CONFIRMED that two-event confirmation has no margin gate.
PROBE REVERTED: n-a; the test is the permanent regression.

NARROWED TO: src/lib/verse-detection-workflow.ts selectPreviewHit and src-tauri/crates/detection/src/bin/detection_accuracy.rs AutoLiveSelector::select.
```

## B.5 - Root cause (step 6)

```
ROOT CAUSE: ambiguous 91%-92% semantic candidates were promoted after repetition because the live workflow and its benchmark mirror selected the highest-ranked semantic result without checking its lead over the runner-up.
WHY CHAIN: close semantic alternatives -> top-only selection -> two-event confirmation -> automatic live presentation -> hint-policy false positive -> exact precision 162/164 below 0.988.
CAUSE->SYMPTOM EVIDENCE: the supplied run lists Revelation 21:4 and Revelation 7:17 at 92%, and Romans 8:39 at 91% beside a 90% runner-up; the RED tests selected the close pair before the fix.
```

## B.8 - Regression test (step 7 -> 8)

```
RED: src/lib/verse-detection-workflow.test.ts "holds repeated semantic candidates when the runner-up is too close" failed: expected selectedVerse null, received Revelation 21:4.
RED: detection_accuracy stable_case_replay_holds_an_ambiguous_semantic_winner failed: ambiguous semantic results stay review-only.
GREEN: both tests passed after requiring a 0.02 rank-score lead.
```

## B.7 - Verification output (step 8)

```
ORIGINAL REPRO RE-RUN:
true positives (correct verse live): 162
false positives (wrong/noise live):  0
false negatives (missed, held back):  0
true negatives (correctly silent):   88
Precision: 100.0%
Recall: 100.0%
Accuracy: 100.0%
Case pass: 100.0%
Exit code: 0
```

```
REGRESSION TESTS:
src/lib/verse-detection-workflow.test.ts: 45 passed
detection_accuracy benchmark unit tests: 13 passed
TYPECHECK: tsc -b passed
LINT: eslint . passed
FORMAT/DIFF: prettier applied to changed TypeScript; rustfmt applied to changed Rust; git diff --check passed.
```

## B.6 - Fix diff (step 8)

```
Added the same 0.02 semantic winner-margin gate to the frontend auto-live selector and the detection_accuracy benchmark selector. Direct references continue to bypass semantic selection. Added frontend and benchmark regression tests for tied 92% semantic candidates.
```

## B.9 - Root-cause writeup (step 9)

```
ROOT-CAUSE WRITEUP - detection-accuracy-precision-gate - 2026-07-31
1. SYMPTOM: Two authored hint cases went live, producing 162/164 exact precision and failing the 0.988 release gate.
2. ROOT CAUSE: Semantic auto-live selected the top candidate even when another semantic result had an indistinguishable or one-point-lower rank score.
3. MECHANISM: The repeated-event confirmation treated persistence as sufficient evidence, even though candidate ambiguity remained unresolved.
4. HOW IT WAS FOUND: The supplied benchmark output exposed the close alternatives; dedicated red tests reproduced auto-selection of a tied 92% pair.
5. THE FIX: The auto-live selector now requires the winning semantic candidate to lead its runner-up by at least 0.02 before confirmation can present it. Direct citations are unchanged.
6. VERIFICATION: The exact release benchmark exited 0 with 100.0% precision/recall/accuracy/case-pass; focused frontend and benchmark tests, typecheck, and lint passed.
7. PREVENTION & FOLLOW-UPS: The new paired-candidate tests lock the ambiguity boundary. Keep the benchmark and frontend selector margins synchronized when policy changes.
```

## Sign-off

```
Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression test (red -> green) attached:   YES
Original reproduction now passes:          YES
Definition of fixed verified:              YES
```
