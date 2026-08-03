# Report: Post-split footguns fix

**Date:** 2026-08-03  
**Plan:** `docs/plans/2026-08-03-post-split-footguns-fix-plan.md`  
**Machine:** Windows workspace  

## Plan recheck (before execute)

| Plan assumption | Status at execute time |
|-----------------|------------------------|
| Legacy third candidate pair present | Confirmed `asset_paths.rs` |
| live_probe defaulted to NLT June | Confirmed |
| Python Fixed(128) + DEPRECATED | Confirmed |
| Public corpus 155,345 | Confirmed on disk |
| No composition manifest | Confirmed |
| Harnesses only under `.tmp/` | Confirmed |
| Ensemble no-original max 0.30 | Confirmed |

Plan remained valid; no scope change required. **Threshold retune (H3)** deferred by design (measure-first; accuracy already at 99.4%/97.5% from prior report).

## Changes shipped

| WP | Done | Detail |
|----|------|--------|
| **A** Legacy fail-closed | Yes | Removed legacy pair; reject basenames `kjv-minilm`, `kjv-nkjv-nlt-minilm`, `kjv-nkjv-nlt-gte`; clearer regenerate message |
| **B** Source of truth | Yes | OnnxEmbedder comments → Rust precompute; Python scripts exit 2 unless `--force-deprecated` |
| **C** live_probe | Yes | Defaults → `public-minilm-l6-v2-q8.bin` + ids |
| **D** Dead bins | Yes | Deleted 6 superseded files (~144 MB) from local `embeddings/` |
| **E** Manifest | Yes | `data/embedding-corpus-manifest.json` + `embeddings/public-minilm-l6-v2.manifest.json`; export writes both; load checks `record_count` |
| **F** Harnesses | Yes | `data/benchmarks/{measure_blend_tokens,retrieval_ab_corpus}.py` + README |
| **G** Search timing | Yes | `log::debug!` `[VECTOR] search n=… k=… took …` in `hnsw_index::search` |
| **H** Ensemble policy | Yes (docs+test only) | Corroboration policy documented; unit test locks 0.30 &lt; 0.42; **no constant retune** |
| Map | Yes | `docs/CODEBASE.md` flow + changelog 2026-08-03 |

## Tests run (execution verification)

| Test | Result |
|------|--------|
| `semantic_candidates_prefer_paired_q8_then_f32_public_assets_only` | **pass** |
| `semantic_assets_are_compatible_rejects_legacy_english_only_corpus` | **pass** |
| `semantic_assets_are_compatible_accepts_matching_minilm_assets` | **pass** |
| `semantic_assets_are_compatible_rejects_unknown_{model,tokenizer}_family` | **pass** |
| `synonym_and_concept_alone_cannot_pass_ensemble_threshold` | **pass** |
| `bun test data/compute-embeddings.test.ts` (3 tests) | **pass** |
| `python data/precompute-embeddings-onnx.py` (no flag) | **exit 2** with Rust instruction |
| `python data/precompute-embeddings.py` (no flag) | **exit 2** with Rust instruction |
| Manifest `record_count` == ids.bin length | **155345 == 155345** |
| Legacy bins absent under `embeddings/` | **confirmed** |

## Post-execution recheck

| Claim | Evidence after change |
|-------|------------------------|
| No production load of legacy | Candidate list is q8+f32 only; reject markers on basenames |
| live_probe points at public-q8 | Source defaults + doc comments |
| Dead 31k corpora gone | Directory listing: only public-minilm-* + manifest |
| Manifest fingerprint | Files present; count matches live index |
| Harnesses tracked | Under `data/benchmarks/` (not only `.tmp/`) |
| Ensemble original-required | Comment + unit test in `ensemble.rs` |
| Search timing | `hnsw_index.rs` debug log |

## Deviations from plan

1. **H3 threshold retune not applied** — plan required `score_distribution` first; prior gate already green. Documented corroboration only.  
2. **FTS stage timing not added** in this pass — vector search timer landed; full pipeline multi-stage log left as follow-up if needed.  
3. **Full `detection_accuracy` gate not re-run** in this session (long ONNX run); behavior of accuracy code path unchanged by these edits.  
4. Manifest for existing bins written with known counts rather than re-running full export (export code path will rewrite on next `export:verses`).

## Conclusion

- **Footguns closed:** silent legacy downgrade, wrong live_probe defaults, deprecated Python rebuild trap, dead local bins.  
- **Verifiability improved:** composition manifest + tracked benchmarks.  
- **Score policy:** explicit corroboration (no retune).  
- **Latency next target:** enable `RUST_LOG=debug` and sample `[VECTOR] search` vs existing `[ONNX] embed` logs; remaining e2e gap still largely outside pure embed.

## Residual follow-ups

- Optional: multi-stage `fts_ms` / `semantic_ms` / `total_ms` single log line in pipeline.  
- Optional: hard-fail load if manifest **missing** after one release cycle (currently warn).  
- Run `score_distribution` before any future cutoff change.
