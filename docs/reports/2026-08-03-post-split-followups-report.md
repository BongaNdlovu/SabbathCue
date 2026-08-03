# Report: Post-split residual follow-ups

**Date:** 2026-08-03  
**Prior report:** `docs/reports/2026-08-03-post-split-footguns-fix-report.md`  
**Machine:** Windows workspace  

## Follow-ups executed

| Follow-up | Action | Status |
|-----------|--------|--------|
| Multi-stage timing | `[DETECT] path=… direct_ms semantic_ms fts_ms merge_ms total_ms` on `process`, `process_semantic`, `process_hybrid_with_fts` | **Done** |
| Vector search timing | Existing `[VECTOR] search` at debug (from prior phase) | **Kept** |
| Manifest missing | Load **fails closed** (no more warn-only) | **Done** |
| `score_distribution` | Ran on public-q8 155345 vectors | **Done** — artifacts below |
| `detection_accuracy` gate | Ran with min-precision 0.988 / min-recall 0.80 | **Done** — **PASS** |
| Threshold retune | Not applied — data does not force a change | **Deferred with evidence** |

## Code changes (this pass)

1. `src-tauri/crates/detection/src/pipeline.rs` — stage timers + `[DETECT]` logs  
2. `src-tauri/src/lib.rs` — missing/invalid/mismatched manifest disables semantic search  
3. `docs/CODEBASE.md` — map + changelog  

## Measurement results

### detection_accuracy (public-q8, threshold 0.90)

| Metric | Value |
|--------|------:|
| Precision | **99.4%** |
| Recall | **97.5%** |
| Accuracy | **98.0%** |
| Case pass | **97.2%** |
| Semantic hints | 26/29 |
| Latency p50 | **82.1 ms** |
| Latency p95 | **153.6 ms** |
| Latency max | **294.4 ms** |
| Gate 0.988 / 0.80 | **PASS** |

Raw log: `docs/reports/2026-08-03-detection-accuracy-followup.txt`

### score_distribution (public-q8)

| Category | n | raw med | raw≥0.78 | score≥0.55 |
|----------|--:|--------:|---------:|-----------:|
| quote | 5 | 0.952 | 4/5 | 5/5 |
| para | 5 | 0.667 | 0/5 | 5/5 |
| prose | 2 matched | 0.632 | 0/2 | 2/2 |

Notes:

- Quote raw similarities remain strong (med ~0.95).  
- Paraphrase tops sit mostly **below** 0.78 operator floor but above internal 0.42 ensemble cutoffs when displayed via other paths.  
- Prose noise does **not** clear raw≥0.78; some still clear old score≥0.55 filter (score is weighted and can exceed 1.0).  
- **Decision:** keep ensemble cutoffs at 0.42 and operator Auto-live at 0.90; no retune this cycle. Monitor prose `disp` if live noise increases.

Embed micro-samples from the same run (ONNX logs): ~2–6 ms per short embed after dynamic padding.

Raw log: `docs/reports/2026-08-03-score-distribution-followup.txt`

## Full recheck (all footguns + follow-ups)

| Check | Result |
|-------|--------|
| Legacy candidate pair gone | **Yes** — only public q8/f32 pairs |
| Legacy basename rejected | **Yes** — unit tests pass |
| Legacy bins on disk | **None** |
| live_probe defaults public-q8 | **Yes** |
| Python precompute blocked without `--force-deprecated` | **Yes** (prior) |
| Manifest present + count match | **155345 == 155345** |
| Missing manifest fails closed | **Code path hard-errors** (lib.rs) |
| Benchmarks under `data/benchmarks/` | **Yes** |
| Ensemble corroboration test | **Pass** (prior) |
| Pipeline `[DETECT]` stage fields | **In source** |
| Vector `[VECTOR]` timing | **In source** (debug) |
| Accuracy gate | **99.4% / 97.5% PASS** |
| Latency p50 vs prior 94.5 ms | **82.1 ms** (better) |

## Threshold recommendation (from score_distribution)

| Option | Verdict |
|--------|---------|
| Raise ensemble 0.42 | **No** — would hurt paraphrase admission further (para raw med 0.667) |
| Raise operator 0.78 floor | **No** — already used only as diagnostic in score_distribution; Auto-live stays 0.90 |
| Lower floors for recall | **No** without a new noise problem — precision gate is healthy |

Product policy remains: **ensemble corroboration (original required for weighted score admission)** + Auto-live 0.90.

## Residual (none blocking)

- `[DETECT]` lines are `log::info` on hybrid/process; ensure production log level includes `info` if operators need them (default app logging should).  
- FTS **SQL** time is still outside `process_hybrid` (paid in `BibleDb::search_verses_bm25` before hybrid). Optional next: time that call in STT / accuracy harness.  
- Re-export will refresh `generated_at` on manifests (counts already correct).

## Conclusion

All residual follow-ups from the footguns plan are **executed and rechecked**. Accuracy gate is green; latency p50 improved further; score data supports **not** retuning cutoffs now; corpus load is fail-closed on composition fingerprint.
