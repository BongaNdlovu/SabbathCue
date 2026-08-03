# Verification report: post-split footguns & follow-ups

**Date:** 2026-08-03  
**Scope:** Independent evidence check of claims #1–#9 about embeddings fallbacks, dead assets, thresholds, ensemble math, latency instrumentation, manifests, and harnesses.  
**Method:** Read primary source (`file:line`), measure on-disk binaries, re-run pad/cosine and Gen 1:1 probes. No implementation in this pass.

**Grades:**  
- **VERIFIED** — code/measurement matches the claim  
- **MOSTLY VERIFIED** — core true; numbers or wording need a correction  
- **INFERENCE** — plausible from evidence but not re-measured here  
- **FALSE / OVERSTATED** — contradicted or not supported  

---

## 1 · Direct answer

| # | Claim (compressed) | Grade | Action warranted? |
|---|--------------------|-------|-------------------|
| 1 | Legacy `kjv-minilm` fallback can load silently; guards miss it | **VERIFIED** | **Yes — high** |
| 2 | Rust comments point at deprecated Python Fixed(128) path | **MOSTLY VERIFIED** | **Yes — low effort** |
| 3 | `live_probe` defaults to dead NLT corpus | **VERIFIED** | **Yes — high** |
| 4 | ~143 MB superseded 31k binaries on disk | **MOSTLY VERIFIED** (144.1 MB) | **Yes — cleanup** |
| 5 | Thresholds need retune after split (over-fire bias) | **INFERENCE** | **Yes — measure first** |
| 6 | Ensemble score bar unreachable without “original” | **VERIFIED** | **Yes — decide & document** |
| 7 | ~90 ms e2e tail uninstrumented; padding ≠ main remaining cost | **MOSTLY VERIFIED** | **Yes — instrument** |
| 8 | No corpus composition manifest | **VERIFIED** | **Yes — medium** |
| 9 | Split A/B harness only under gitignored `.tmp/` | **VERIFIED** | **Yes — promote** |
| — | Precision gate now fine (99.4% / 97.5%) | **VERIFIED** (from shipped report) | No action |

**Ordering judgment:** The suggested order (#1+#2 → #7 → #5+#6; #8+#9 with #7) is **sound**. #3 belongs with #1 (same class of “health tool points at wrong corpus”). #4 follows #1/#3.

---

## 2 · Claim-by-claim evidence

### #1 — Legacy embeddings fallback = silent corpus downgrade

#### Claimed

- `asset_paths.rs` still names `kjv-minilm-l6-v2.bin` as third fallback.  
- `semantic_assets_are_compatible` is a `"minilm-l6-v2"` substring check → legacy name matches.  
- Startup sanity uses KJV Gen 1:1 at ≥0.80 → passes on legacy.  
- Dynamic-vs-fixed cosine ~0.985–0.994 → legacy still looks “healthy.”  
- Net: missing/corrupt public index → app runs English-only 31k June corpus without failing closed.

#### Evidence

**Fallback constants and chain (third pair):**

```12:17:src-tauri/src/asset_paths.rs
pub const PREFERRED_EMBEDDINGS_FILENAME: &str = "public-minilm-l6-v2-q8.bin";
// ...
const LEGACY_EMBEDDINGS_FILENAME: &str = "kjv-minilm-l6-v2.bin";
const LEGACY_EMBEDDING_IDS_FILENAME: &str = "kjv-minilm-l6-v2-ids.bin";
```

```372:380:src-tauri/src/asset_paths.rs
    let pairs = [
        (PREFERRED_…, PREFERRED_…_IDS),
        (F32_…, F32_…_IDS),
        (LEGACY_…, LEGACY_…_IDS),  // third family
    ];
```

Unit test explicitly prefers q8 → f32 → legacy (`semantic_candidates_prefer_paired_q8_then_f32_then_legacy_assets`).

**Compatibility guard is filename family only:**

```55:69:src-tauri/src/asset_paths.rs
fn is_minilm_asset(path: &Path) -> bool {
    path.to_string_lossy().to_ascii_lowercase().contains("minilm-l6-v2")
}
// all of model, tokenizer, embeddings, ids must contain that substring
```

Probe:

| Path | contains `minilm-l6-v2` |
|------|-------------------------|
| `kjv-minilm-l6-v2.bin` | **true** |
| `public-minilm-l6-v2-q8.bin` | **true** |
| `kjv-nkjv-nlt-gte-small.bin` | **false** (not in candidate list anyway) |

**Sanity probe:**

```32:38:src-tauri/src/lib.rs
const SEMANTIC_SANITY_PROBE: &str = "In the beginning God created the heaven and the earth.";
const SEMANTIC_SANITY_MIN_SIMILARITY: f64 = 0.80;
```

**On-disk legacy file (this workspace):**

| File | mtime | vectors | unique ids |
|------|-------|--------:|-----------:|
| `embeddings/kjv-minilm-l6-v2.bin` | 2026-06-04 | **31,102** | 31,102 |
| ids sibling | 2026-06-04 | 31,102 | 31,102 |

**Live public corpus (post-split):** 155,345 vectors / 31,102 unique ids (2026-08-03).

**Gen 1:1 self-search with *current* dynamic-pad query embedder (measured 2026-08-03):**

| Corpus | top cosine | ≥0.80? |
|--------|----------:|--------|
| `kjv-minilm-l6-v2.bin` (legacy) | **0.9908** | **pass** |
| `public-minilm-l6-v2.bin` (split) | **1.0000** | pass |

**Fixed vs dynamic pad cosine (short English, same ONNX int8, mean-pool + L2):**

| Text | cosine(fixed128, dynamic) |
|------|--------------------------:|
| Gen 1:1 | 0.990821 |
| Ps 23:1-ish | 0.992858 |
| John 3:16-ish | 0.991208 |

Report band 0.985–0.994 (**VERIFIED** this session in the same ballpark; not “~0.92”).

**Logging if legacy loads:**

```166:173:src-tauri/src/lib.rs
// public-* → "public-domain multi-vector corpus"
// else     → "KJV canonical legacy"
```

So it is not *completely* silent (log says legacy), but it is **not fail-closed**: load is treated as success, pipeline stays enabled.

#### Nuance (not a rebuttal)

Downgrade only happens when **preferred q8 and f32** candidates are missing or fail sanity *before* legacy is tried. With a healthy `public-minilm-l6-v2-q8.bin` present, legacy is never selected. The footgun is **availability/stale-path**, not “legacy always wins.”

#### Verdict

**VERIFIED.** Guards do not distinguish composition or build policy. Legacy is a real silent-quality failure mode when public assets are absent.  
**Recommendation:** remove legacy from the candidate list (or require explicit env opt-in + hard log error). Prefer fail-closed over English-only 31k.

---

### #2 — Rust comments point the wrong way at Python precompute

#### Evidence

`onnx_embedder.rs` still says:

- ~L55–56: `MUST match the Python precompute script (... MAX_LENGTH)` (truncation cap).  
- ~L312–314: `MUST match the Python precompute script` for **mean pooling**.

Python:

| Script | Header | Padding |
|--------|--------|---------|
| `data/precompute-embeddings.py` | **DEPRECATED** — do not use for app embeddings | Fixed `enable_padding(length=MAX_LENGTH)` |
| `data/precompute-embeddings-onnx.py` | **DEPRECATED for release**; claims “dynamic padding” in prose | Still `tokenizer.enable_padding(length=MAX_LENGTH)` **Fixed(128)** |

Rust runtime/precompute now uses `PaddingStrategy::BatchLongest` (`onnx_embedder.rs` ~167–168).

#### Correction to the claim

- Comment at ~312 is about **pooling**, not padding; that half is still a valid invariant (mean-pool vs last-token).  
- Comment at ~55 is about **max length 128**, which Python still shares; the danger is readers treating “match Python” as full tokenizer parity including **padding**.  
- The ONNX Python deprecation blurb **claims** dynamic padding while the body still Fixed(128) — **internal contradiction** in that file.

#### Verdict

**MOSTLY VERIFIED** as a documentation footgun.  
**Recommendation:** rewrite comments to “Rust `OnnxEmbedder` / `bun run precompute:embeddings` is source of truth; Python scripts are deprecated diagnostics.” Either delete scripts or make padding match BatchLongest and stop claiming false parity.

---

### #3 — `live_probe` defaults to a dead index

#### Evidence

```33:36:src-tauri/crates/detection/src/bin/live_probe.rs
let embeddings = ... "embeddings/kjv-nkjv-nlt-minilm-l6-v2.bin"
let ids = ... "embeddings/kjv-nkjv-nlt-minilm-l6-v2-ids.bin"
```

On disk: **31,102** vectors, mtime **2026-06-14**. App preferred path is `public-minilm-l6-v2-q8.bin` (155,345 vectors, 2026-08-03). That June file is **not** in `semantic_embedding_candidates`.

Doc header of `live_probe` still shows the NLT path as the example.

#### Verdict

**VERIFIED.** Default diagnostic validates a corpus the production loader does not use.  
**Recommendation:** default to `public-minilm-l6-v2-q8.bin` + matching ids, or require `--embeddings` / `--ids`.

---

### #4 — ~143 MB of superseded binaries

#### Measurement (this workspace)

| Asset set | Vectors | Emb size | Dates |
|-----------|--------:|---------:|-------|
| `kjv-minilm-l6-v2*` | 31,102 | 47.8 + 0.2 MB | 2026-06-04 |
| `kjv-nkjv-nlt-minilm-l6-v2*` | 31,102 | 47.8 + 0.2 MB | 2026-06-14 |
| `kjv-nkjv-nlt-gte-small*` | 31,102 | 47.8 + 0.2 MB | 2026-06-25 |
| **Sum (all six files)** | | **144.1 MB** (137.4 MiB) | |

`gte-small` is not MiniLM and is not a loader candidate; it is still dead weight once tooling no longer points at it.

#### Verdict

**MOSTLY VERIFIED** (144.1 MB, not exactly 143). Safe to delete after #1 and #3 (and any local scripts) no longer reference them. Note `embeddings/*` is gitignored — this is local disk hygiene + release-machine cleanup, not a git history issue.

---

### #5 — Threshold retune deferred; now due (over-fire bias)

#### Facts (calibration constants still at pre-split values)

| Constant | Location | Value |
|----------|----------|------:|
| `ORIGINAL_CUTOFF` / `SYNONYM_CUTOFF` | `ensemble.rs` | 0.42 |
| `CONCEPT_CUTOFF` | `ensemble.rs` | 0.40 |
| `ENSEMBLE_THRESHOLD` | `ensemble.rs` | 0.42 |
| `DEFAULT_CONFIDENCE_THRESHOLD` (semantic detector) | `detector.rs` | 0.42 |
| `DECISIVE_SEMANTIC_CONFIDENCE` | `deepseek-ranker.ts` | 0.90 |
| `MIN_AMBIGUITY_MARGIN` | `deepseek-ranker.ts` | 0.15 |

Plan intentionally left these alone during the swap.

#### Similarity shift evidence (from controlled A/B + report)

| Query type | Blend hit@1 | Split hit@1 |
|------------|------------:|------------:|
| KJV first-half lower | 90.7% | 97.2% |
| SpaRV verbatim | 79.6% | 99.6% |

`detection_accuracy` precision did **not** collapse after split (baseline 98.7% → after OR-guard 99.4%); that is **not** proof of over-firing — the curated gate may not stress the new score mass.

#### Verdict

**INFERENCE** that cutoffs “now sit lower relative to the score distribution than intended.” Directionally plausible; **not re-proven** without a fresh `score_distribution` (or equivalent) on the 155k index.  
**Recommendation:** run `score_distribution` / accuracy score histograms **before** changing constants. Do not retune from A/B hit rates alone.

---

### #6 — Ensemble threshold unreachable without original strategy

#### Arithmetic (from code)

Weights: original **0.7**, synonym **0.2** total (two variants × `SYNONYM_WEIGHT/2`), concept **0.1**.

Filter:

```126:130:src-tauri/crates/detection/src/semantic/ensemble.rs
.filter(|r| r.score >= ENSEMBLE_THRESHOLD)  // 0.42
```

| Scenario | Max weighted `score` at sim=1.0 |
|----------|--------------------------------:|
| Original only | 0.70 |
| Synonym + concept only (no original) | **0.20 + 0.10 = 0.30** |
| All three | 1.00 |

**0.30 < 0.42** → a verse never hit by the original query **cannot** pass the ensemble score filter, even at perfect synonym+concept similarity.

**Original-only bar for score filter:** need \(0.7 \times s \ge 0.42\) ⇒ \(s \ge 0.60\).

#### Interaction with `best_similarity` gate

`detector.rs` gates display inclusion on `best_similarity >= confidence_threshold` (0.42), **after** ensemble already filtered on weighted `score`. So:

- Weighted filter can demand **≥0.60** original similarity if no corroboration.  
- Confidence path uses **raw** best similarity (post-fix).  
- Synonym/concept act as **corroboration / score boost**, not independent discovery under current constants.

`set_use_synonyms(false)` still exists for A/B (direct embedding path).

#### Verdict

**VERIFIED.** Behavior may be intentional but is **implicit**.  
**Recommendation:** document (and optionally test) “original required for ensemble admission”; or redesign if independent synonym recovery is a product goal.

---

### #7 — Profile the ~90 ms; padding was not the main e2e lever

#### From shipped report (`docs/reports/2026-08-03-split-corpus-dynamic-padding-report.md`)

| Metric | Before | After |
|--------|-------:|------:|
| detection_accuracy latency p50 | **101.3 ms** | **94.5 ms** (Δ **−6.8 ms**) |
| Short ONNX embed p50 (microbench) | **11.762 ms** | **2.648 ms** (~**4.44×**) |

#### Instrumentation status (this tree)

| Path | Timing log? |
|------|-------------|
| `OnnxEmbedder::embed` | Yes — `[ONNX] embed() took …` |
| `HnswVectorIndex::search` | **No** `Instant` |
| FTS / full pipeline stages | Partial / not a structured hot-path breakdown in `search()` |
| `detection_accuracy` | Wall time per case (end-to-end) |

#### Arithmetic consistency (host-class)

If ensemble does up to ~4 embeds × ~2.6 ms ≈ **10 ms**, and search is order **10–20 ms** at 155k, that still leaves a large fraction of **~95 ms** e2e unaccounted for (STT not in accuracy bin; chunking, merger, FTS, SQLite, ensemble overhead, logging, etc.).

#### Verdict

**MOSTLY VERIFIED.** Embed win is large in isolation; e2e p50 only moved ~7 ms — so remaining work is elsewhere. “~90 ms nobody has looked at” is slightly rhetorical; e2e is measured, **stage breakdown is not**.  
**Recommendation:** add `Instant` around vector `search()` and the FTS leg; log once per detection pass before optimizing further.

---

### #8 — Composition manifest scoped but missing

#### Evidence

- `Test-Path data/embedding-corpus-manifest.json` → **False**  
- `Test-Path embeddings/manifest.json` → **False**  
- Plan (`docs/plans/2026-08-03-split-corpus-dynamic-padding-plan.md`) listed T-A4 / composition fingerprint as **optional**  
- `prepare-embeddings.ts` still skips export/precompute when artifacts exist unless `--force`  

Combined with #1: neither filename family check nor Gen 1:1 sanity encodes **composition** (blend vs split, translation set, vector count, padding policy).

#### Verdict

**VERIFIED** gap.  
**Recommendation:** write a small manifest at export/precompute time (blended list, separate list, record count, unique ids, padding/truncation policy, model id) and refuse load on mismatch — or at least hard-fail if `index.len()` is not the expected public count when filename claims `public-minilm-*`.

---

### #9 — Split evidence harness lives under gitignored `.tmp/`

#### Evidence

`.gitignore:35` → `.tmp/`  

Present on this machine (untracked):

| Path | Role |
|------|------|
| `.tmp/measure_blend_tokens.py` | Token audit (mean 170.2, trunc 67.5%, …) |
| `.tmp/retrieval_ab_corpus.py` | Retrieval A/B (EN partial / SpaRV / …) |
| `.tmp/blend_token_stats.json` | Cached token stats |

Headline numbers also **copied into** `docs/reports/2026-08-03-split-corpus-dynamic-padding-report.md` (committed narrative), but the **re-runnable harness** is not under version control.

Plan line about promoting the A/B was not executed as a `data/` script or CI-facing test.

#### Verdict

**VERIFIED.** Risk: next corpus change cannot re-prove split quality without recreating scratch scripts.  
**Recommendation:** promote to `data/` or a `#[ignore]` Rust/ONNX integration test; keep fixtures small enough for CI optional jobs.

---

### Precision gate (claimed “genuinely fine”)

From the same report, post broad-OR guard:

- Precision **99.4%**, recall **97.5%**, gates 0.988 / 0.80 **pass**.

**VERIFIED** as documented. Not re-run in this verification session (would require `detection_accuracy` binary + fixtures).

---

## 3 · Cross-cutting risk model

```text
Healthy public-q8 present ──► app OK (legacy never tried)
         │
         ▼ missing / fails load
     f32 public ──► app OK if present
         │
         ▼ missing / fails
     kjv-minilm legacy ──► LOAD SUCCESS, log "KJV canonical legacy"
                           Gen1:1 ~0.99 with dynamic query pad
                           English-only 31k, no WEB/ES/FR/PT split vectors
```

**Second failure mode:** operator runs `live_probe` / ad-hoc tools on NLT/June indexes while the app uses public-q8 → false diagnostics.

**Third:** setup skip + no manifest → stale 62k or mixed policy can persist after list/padding changes until someone `--force`s.

---

## 4 · Recommended work packages (priority)

### P0 — Active footguns (minutes–hour)

1. **Remove legacy candidate pair** from `semantic_embedding_candidates_for_roots` (or gate behind explicit env). Update unit test that expects legacy.  
2. **Repoint or require args** for `live_probe` → `public-minilm-l6-v2-q8.bin`.  
3. **Rewrite onnx_embedder comments**; fix or delete deprecated Python scripts’ padding lie.  
4. **Delete or quarantine** local `kjv-*` / `gte-small` bins after (1–2).

### P1 — Make the next change verifiable

5. **Instrument** vector `search()` + FTS timing; one log line per detection path.  
6. **Promote** `.tmp/measure_blend_tokens.py` + `.tmp/retrieval_ab_corpus.py` (or successors) to `data/` with a short README.  
7. **Add composition manifest** written at export/precompute; optional load check (vector count + translation lists).

### P2 — Score policy (measure → decide → document)

8. Run **`score_distribution`** (and/or accuracy score dumps) on current 155k index.  
9. Decide **ensemble original-required** policy; document; add unit test for max synonym+concept score &lt; `ENSEMBLE_THRESHOLD`.  
10. Only then retune cutoffs / decisive margins if histograms show systematic over-fire.

---

## 5 · What this verification does *not* claim

- Did not re-run full `detection_accuracy` gate in this session.  
- Did not profile production e2e with new timers (those do not exist yet).  
- Did not prove over-firing from live sermons — only that constants are unchanged while retrieval strengths moved.  
- Did not audit app-data directory on a shipped install (only repo `embeddings/` tree).

---

## 6 · Source list

| ID | Source |
|----|--------|
| S1 | `src-tauri/src/asset_paths.rs` (legacy constants, candidate order, `is_minilm_asset`) |
| S2 | `src-tauri/src/lib.rs` (sanity probe, corpus log label) |
| S3 | `src-tauri/crates/detection/src/bin/live_probe.rs` defaults |
| S4 | `src-tauri/crates/detection/src/semantic/onnx_embedder.rs` (BatchLongest + comments) |
| S5 | `data/precompute-embeddings.py`, `data/precompute-embeddings-onnx.py` |
| S6 | `src-tauri/crates/detection/src/semantic/ensemble.rs`, `detector.rs` |
| S7 | `src/lib/deepseek-ranker.ts` decisive thresholds |
| S8 | On-disk `embeddings/*` size/count/mtime measurements (2026-08-03) |
| S9 | ONNX pad cosine + Gen1:1 probes (2026-08-03) |
| S10 | `docs/reports/2026-08-03-split-corpus-dynamic-padding-report.md` |
| S11 | `.tmp/blend_token_stats.json`, `.tmp/*_ab*.py`, `.gitignore` |
| S12 | `docs/plans/2026-08-03-split-corpus-dynamic-padding-plan.md` |

---

## 7 · Bottom line

The critique is **substantially correct**. The two active footguns (#1 legacy fallback + #3 dead `live_probe` defaults) are real and evidence-backed. Documentation drift (#2) and missing manifest/harness (#8–#9) keep the next change under-specified. Ensemble “original required” (#6) is arithmetically true and under-documented. Threshold retune (#5) and deep latency work (#7) should wait on **measurement**, not vibes — but the e2e-vs-embed gap in the existing report already shows padding is no longer the main latency story.

**Do P0 first.** Then instrument and promote harnesses. Then retune with `score_distribution` in hand.
