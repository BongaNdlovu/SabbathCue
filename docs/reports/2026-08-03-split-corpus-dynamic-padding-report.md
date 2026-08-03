# Report: Split corpus + dynamic padding

Date / commit / machine: 2026-08-03 / `a82ebe3306d56efc5b94bdaae590d45635ffd9d4` / Windows workspace host

## Baseline (Phase 0)

- Vector count: 62,197 q8/f32 vectors; 31,102 unique KJV IDs.
- `detection_accuracy` at 90%: precision 98.7%, recall 96.9%, accuracy 97.2%, case pass 96.0%; the requested 98.8% precision floor already failed by 0.1 percentage points.
- Token sample (`n=4,000`, seed 42): current multilingual blend mean 170.2 tokens, p90 267, 67.5% truncated at 128; KJV share of retained tokens 27.1%.
- Baseline detection latency: p50 101.3 ms, p95 194.5 ms, max 282.8 ms.

## Changes shipped

- `BLENDED_TRANSLATIONS = ["KJV"]`.
- `SEPARATE_VECTOR_TRANSLATIONS = ["WEB", "SpaRV", "FreJND", "PorBLivre"]`.
- `buildEmbeddingEntries` is a pure, tested exporter that emits independent records keyed to the canonical KJV verse ID.
- `OnnxEmbedder` configures `BatchLongest` padding with max truncation 128 and logs `padding=dynamic max_tokens=128`. Mean pooling, L2 normalization, and dimension remain unchanged.
- Rust precompute is the release source of truth so runtime and corpus generation use the same tokenizer/padding path. The Python ONNX script remains an explicitly deprecated diagnostic fallback.

## Post-change measurements

- Export: 155,345 JSON records, 31,102 unique IDs, maximum five records per ID.
- Binary precompute: 155,345 f32 vectors (238,609,920 bytes) and 155,345 IDs.
- `detection_accuracy` at 90%: precision 98.7%, recall 96.9%, accuracy 97.2%, case pass 96.4%, semantic hints 25/29 (baseline 24/29), safe abstentions 10/10; latency p50 94.5 ms, p95 181.3 ms, max 285.8 ms. The 0.988 precision gate remains intentionally red.
- Controlled retrieval A/B (`n=800`, same verse sample): KJV first-half hit@1 improved 90.7% (blend) to 97.2% (all separate); SpaRV verbatim hit@1 improved 79.6% to 99.6%; WEB text remained 99.6%.
- Dynamic ONNX microbench (five short requests, 30 repetitions): dynamic p50 2.648 ms vs fixed-128 p50 11.762 ms (4.44x p50 speedup; sequence lengths 11–15).
- Quantization: worst per-vector cosine 0.999913; 227.56 MiB f32 to 57.48 MiB q8.
- `compare:embeddings`: top-1 agreement 1.000000, exact-order agreement 0.699219, top-k overlap 0.990234, max similarity drift 0.001866; all configured gates passed.

## Tests run

- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`: pass.
- `cargo test -p rhema-detection --features onnx,vector-search`: all crate targets passed (363 library tests plus integration/binary/doc targets; one model-loading test is intentionally ignored by default).
- `cargo test --manifest-path src-tauri/Cargo.toml --workspace`: all workspace targets passed, including Bible retrieval, detection, STT, API, and resource-bundle tests.
- Bundled ONNX dimension test with `--ignored`: pass (384 dimensions).
- `bun test data/compute-embeddings.test.ts`: 2 exporter tests passed (Bun emitted a trailing workspace `EPERM` read warning after the tests).
- Targeted frontend regression suite: 6 files, 144 tests passed.
- Full frontend unit suite: 1,289 passed across 188 files; one unrelated live Paddle sandbox test failed because outbound network access is denied in this workspace (`fetch failed`, `EACCES`).

## Incidents / surprises

- Dynamic and fixed padding are not vector-identical for this model because mean pooling changes when padded sequence positions are included in the ONNX computation. The recorded 20-string probe was approximately 0.985–0.994 cosine, so the plan's original `≥0.999` T-B3 criterion was corrected to a diagnostic and the corpus was fully rebuilt with dynamic padding.
- The existing accuracy corpus remains just below its precision floor. This is reported as a release decision point, not hidden by lowering the threshold.

## Conclusion

- Accuracy: split-language retrieval improved (English partial and Spanish hit@1). The initial rebuild measured 98.7% precision/96.9% recall; the follow-up broad-OR guard raised the final gate result to 99.4% precision/97.5% recall.
- Latency: improved for short embedding requests (4.44x p50 in the direct ONNX probe); end-to-end detection p50 also fell from 101.3 ms to 94.5 ms. Search quality/fidelity gates passed after q8 regeneration.
- Follow-ups: review the remaining hint-mode Isaiah ASR case and retain the baseline binaries in `C:\Users\fanel\AppData\Local\Temp\sabbathcue-baseline-20260803\` for rollback comparison.

## Follow-up precision-gate fix

The remaining gate failure was reproduced and traced to `exact_quote_keys`: short ordered-overlap evidence was being applied to broad OR-tier FTS hits. A broad Deuteronomy 31:19 result consequently received 92% quote confidence and auto-fired over the expected 1 John 2:1 hint.

The narrow fix admits short-overlap quote evidence only for non-broad FTS candidates. Long exact quotes remain eligible regardless of tier. Regression coverage was red before the guard and green after it.

Rerun of the original accuracy command:

- 158 true positives, 1 false positive, 4 false negatives, 87 true negatives.
- Precision 99.4%, recall 97.5%, accuracy 98.0%, case pass 97.2%.
- The 98.8% precision and 0.80 recall gates now pass.
- Full Rust workspace tests, ONNX detection tests, TypeScript typecheck, and lint pass.
