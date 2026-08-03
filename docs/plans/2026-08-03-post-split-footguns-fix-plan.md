# Plan: Post-split footguns, verification harnesses, and score/latency follow-ups

**Date:** 2026-08-03  
**Status:** Proposed (investigation complete; implementation not started)  
**Evidence base:**  
- `docs/reports/2026-08-03-post-split-footguns-verification.md` (technical)  
- `docs/reports/2026-08-03-post-split-footguns-plain-language.md` (operator)  
- `docs/reports/2026-08-03-split-corpus-dynamic-padding-report.md` (shipped split + padding)  
**Goal:** Eliminate silent corpus downgrade and dead diagnostics, make the next corpus change verifiable, instrument remaining latency, then retune or document score policy with measurements—without breaking the healthy public MiniLM path.

---

## 0 · Why this plan exists

| # | Problem | Evidence (rechecked 2026-08-03) | Risk if ignored |
|---|---------|----------------------------------|------------------|
| 1 | Legacy `kjv-minilm-l6-v2` is still a load candidate | `asset_paths.rs:16–17,379`; Gen 1:1 on legacy ≈ **0.99** ≥ 0.80 | Silent English-only 31k index |
| 2 | Rust comments point at deprecated Python Fixed(128) | `onnx_embedder.rs:55,312`; both Python scripts still `enable_padding(length=128)` | Wrong rebuild → broken public embeddings |
| 3 | `live_probe` defaults to NLT June corpus | `live_probe.rs:34–36`; 31,102 vectors; not in app candidate list | False health diagnostics |
| 4 | ~144 MB dead bins on disk | Measured `kjv-*` + `gte-small` sets | Confusion + disk waste |
| 5 | Cutoffs still pre-split | `ensemble.rs` 0.42; `deepseek-ranker.ts` 0.90 / 0.15 | Possible over-fire (unproven) |
| 6 | Ensemble score bar needs original strategy | Max synonym+concept **0.30** &lt; **0.42** | Undocumented product behavior |
| 7 | E2E ~95 ms; embed ~2.6 ms; search untimed | Split report p50 101.3→94.5; no `Instant` in `hnsw_index::search` | Optimize wrong layer |
| 8 | No composition manifest | No `data/` or `embeddings/` manifest file | Stale rebuild skip trap |
| 9 | A/B harness only under gitignored `.tmp/` | `.gitignore:35`; scripts present only there | Proof not re-runnable |

**Product conclusion:** Fix footguns first (fail closed on wrong corpus). Then make measurement permanent. Then decide score policy from data.

---

## 1 · Compatibility map (do not break these)

### 1.1 Asset resolution (must stay coherent after #1)

```text
Today:
  app_data / resource / dev_root
    × preferred q8  →  f32 public  →  LEGACY kjv-minilm

Target:
  app_data / resource / dev_root
    × preferred q8  →  f32 public only
  missing both → semantic DISABLED (error log), no silent legacy
```

| Consumer | File | Constraint |
|----------|------|------------|
| Candidate list | `asset_paths.rs` | Update pairs + unit test that encodes legacy |
| Compatibility | `semantic_assets_are_compatible` | Keep MiniLM family check; optionally **reject** paths containing `kjv-minilm` / non-`public-` |
| Load + sanity | `lib.rs` | Sanity Gen 1:1 remains; corpus label already branches on `public-minilm-l6-v2` prefix |
| Assets command | `commands/assets.rs` | Uses same compatibility helper |
| Bundle | `tauri.conf.json` | Already bundles **q8 public** only — no change required for #1 |

### 1.2 What we will **not** change in P0

| Leave alone until later phase | Why |
|-------------------------------|-----|
| Ensemble weights / cutoffs | Need `score_distribution` first (#5/#6) |
| `detection_accuracy` floors | Already green at 99.4% / 97.5% after OR-guard |
| HNSW / ANN | Not the footgun |
| Frontend Auto-live rules | Orthogonal |
| Re-precomputing 155k | Not required for #1–#4, #7–#9 |

### 1.3 Tests that **must** be updated when legacy is removed

| Test | Current expectation | New expectation |
|------|---------------------|-----------------|
| `semantic_candidates_prefer_paired_q8_then_f32_then_legacy_assets` | Includes legacy pairs | Rename to `…_q8_then_f32`; **no** legacy entries |
| Any doc/README mentioning `kjv-minilm` as fallback | Present | Point to fail-closed + regenerate |

---

## 2 · Work packages

### WP-A — Fail closed on corpus (items #1, partial #8)

#### A1. Remove legacy from candidate list

**Files:** `src-tauri/src/asset_paths.rs`

**Change:**
- Delete `LEGACY_EMBEDDINGS_FILENAME` / `LEGACY_EMBEDDING_IDS_FILENAME` constants **or** keep constants private unused only if referenced in “reject” tests—prefer **delete**.
- Remove third pair from `semantic_embedding_candidates_for_roots`.
- Update unit test accordingly.

**Why:** Legacy is English-only 31k, built under Fixed(128) policy era; Gen 1:1 cannot distinguish it from a healthy index (measured top sim **0.9908**). A fallback whose only remaining function is silent degradation is worse than no fallback.

**Code check before merge:**
```text
rg "LEGACY_EMBEDDINGS|kjv-minilm-l6-v2" src-tauri/
# expect: zero production references; docs may mention historical
```

#### A2. Harden compatibility (optional but recommended with A1)

**Change options (pick one):**

| Option | Behavior | Pros | Cons |
|--------|----------|------|------|
| **A2-strict** | Embeddings path must contain `public-minilm-l6-v2` | Blocks any non-public MiniLM name | Breaks intentional local experiments unless renamed |
| **A2-reject-legacy** | Explicitly reject `kjv-minilm` and `kjv-nkjv-nlt` substrings | Narrow | Must maintain denylist |
| **A2-manifest** | Prefer manifest match (WP-E) | Strongest | Depends on WP-E |

**Plan default:** A1 + **A2-reject-legacy** denylist on embeddings/ids basenames + keep `minilm-l6-v2` family check for model/tokenizer. When WP-E lands, add vector-count/composition check.

#### A3. Fail-closed messaging in `lib.rs`

When no candidate loads:
- Keep existing SEMANTIC DISABLED error.
- Ensure message names **exact** recovery:  
  `bun run export:verses` → `bun run precompute:embeddings` → `bun run quantize:embeddings`.

**Why:** Operators currently may still have legacy files on disk; after A1 those files no longer “save” them—must regenerate public corpus.

#### A tests

| ID | Test | Assertion |
|----|------|-----------|
| T-A1 | Unit: candidate list | Only q8 + f32 public pairs; length = 2 × roots |
| T-A2 | Unit: reject legacy name | `semantic_assets_are_compatible(..., "kjv-minilm-l6-v2.bin", ...)` → false if A2-reject |
| T-A3 | Unit: accept public | `public-minilm-l6-v2-q8.bin` still true |
| T-A4 | Manual/smoke | With only legacy bins present, app logs DISABLED and does not load 31102 index |

---

### WP-B — Source-of-truth docs & scripts (item #2)

#### B1. Rewrite `onnx_embedder.rs` comments

**Replace** “MUST match the Python precompute script” with:

- Truncation max 128 and mean-pool + L2 must match **`bun run precompute:embeddings`** (Rust `OnnxEmbedder` / precompute bin).  
- Python scripts under `data/precompute-embeddings*.py` are **deprecated diagnostics** and must not define production padding.

**Why:** Production path is `BatchLongest`; Python still Fixed(128). The old comment pre-loads the broken-public-embeddings failure mode.

#### B2. Python scripts

**Minimum:** Align headers so they cannot claim “dynamic padding” while calling Fixed(128).  
**Preferred:**  
- Keep DEPRECATED banner.  
- Either switch padding to no fixed pad / batch longest **or** add `sys.exit("Use bun run precompute:embeddings")` at `main()`.  
- `package.json` scripts `precompute:embeddings-onnx` / `precompute:embeddings-py`: prefix with warning or remove.

**Why:** `prepare-embeddings.ts` already uses Rust precompute; npm aliases still invite wrong path.

#### B tests

| ID | Test | Assertion |
|----|------|-----------|
| T-B1 | Existing onnx padding unit tests | Still `BatchLongest` |
| T-B2 | Doc grep CI optional | No required “MUST match Python” in `onnx_embedder.rs` |
| T-B3 | If Python exits | `python data/precompute-embeddings-onnx.py` exits non-zero with Rust instruction |

---

### WP-C — Diagnostics point at live corpus (item #3)

#### C1. `live_probe.rs` defaults

**Change defaults to:**
```text
embeddings/public-minilm-l6-v2-q8.bin
embeddings/public-minilm-l6-v2-q8-ids.bin
```
Update module docs / usage comment the same way.

**Alternative (stricter):** require `--embeddings` and `--ids` (no defaults). Prefer **public-q8 defaults** for ergonomics + correctness.

**Why:** Tool claims to “replicate the exact live semantic path”; defaults currently validate a corpus the app never loads (31,102 NLT-era MiniLM).

#### C tests

| ID | Test | Assertion |
|----|------|-----------|
| T-C1 | Compile + doc example | Paths match preferred filenames in `asset_paths.rs` (single source: re-export const or duplicate string with comment “keep in sync”) |
| T-C2 | Manual | `live_probe` without args loads index with **155345** vectors (log) when public-q8 present |

**Code check:** Prefer sharing constants from a small shared module if crates allow; detection bin cannot easily import tauri `asset_paths`. **Pragmatic:** hardcode the same public-q8 names with a one-line comment linking to `PREFERRED_EMBEDDINGS_FILENAME`.

---

### WP-D — Delete superseded binaries (item #4)

#### D1. Local + release hygiene (not a code change)

**Delete after WP-A and WP-C land:**

```text
embeddings/kjv-minilm-l6-v2.bin
embeddings/kjv-minilm-l6-v2-ids.bin
embeddings/kjv-nkjv-nlt-minilm-l6-v2.bin
embeddings/kjv-nkjv-nlt-minilm-l6-v2-ids.bin
embeddings/kjv-nkjv-nlt-gte-small.bin
embeddings/kjv-nkjv-nlt-gte-small-ids.bin
```

**Why:** ~144 MB; all 31,102 vectors; none are production candidates after A/C. `embeddings/*` is gitignored—cleanup is local/CI cache/docs.

#### D2. Docs

- `README` / plans that mention `kjv-nkjv-nlt-minilm` for probes → public-q8.  
- `docs/release-checklist.md` if it lists legacy names.

#### D tests

| ID | Check |
|----|--------|
| T-D1 | After delete, `bun run` / app still loads public-q8 |
| T-D2 | `rg "kjv-nkjv-nlt|kjv-minilm-l6-v2" --glob '!docs/reports/**'` clean of **instructions** (historical reports OK) |

---

### WP-E — Composition manifest (item #8)

#### E1. Write manifest at export time

**File:** e.g. `data/embedding-corpus-manifest.json` (tracked) **and/or** `embeddings/public-minilm-l6-v2.manifest.json` (next to bins, gitignored with bins).

**Recommended tracked template generated by `compute-embeddings.ts`:**

```json
{
  "schema_version": 1,
  "blended_translations": ["KJV"],
  "separate_translations": ["WEB", "SpaRV", "FreJND", "PorBLivre"],
  "record_count": 155345,
  "unique_verse_ids": 31102,
  "model_family": "minilm-l6-v2",
  "padding": "batch_longest",
  "max_tokens": 128,
  "generated_at": "ISO-8601"
}
```

Update counts from actual export.

**Why:** Filename + Gen 1:1 cannot encode composition; setup skip can leave stale JSON/binaries.

#### E2. Precompute / quantize

- Precompute bin logs `record_count` vs manifest if present.  
- Optional: write digests of ids.bin into manifest after precompute.

#### E3. Runtime check (phase 2 of E)

In `lib.rs` after load:
- If manifest present and `index.len() != record_count` → **fail** this candidate, try next (or disable).  
- If manifest missing → **warn** once (transition), later **error** after one release.

**Why:** Catches both legacy (if somehow reintroduced) and half-finished rebuilds.

#### E tests

| ID | Test | Assertion |
|----|------|-----------|
| T-E1 | Export unit | Manifest fields match lists in `compute-embeddings.ts` |
| T-E2 | Export smoke | `record_count` equals JSON array length |
| T-E3 | Unit load helper | Mismatched count rejects |
| T-E4 | Fixture | Manifest with wrong count fails candidate selection |

---

### WP-F — Promote evidence harnesses (item #9)

#### F1. Move / rewrite scripts into tracked tree

| From (gitignored) | To |
|-------------------|-----|
| `.tmp/measure_blend_tokens.py` | `data/benchmarks/measure_blend_tokens.py` |
| `.tmp/retrieval_ab_corpus.py` | `data/benchmarks/retrieval_ab_corpus.py` |

Add `data/benchmarks/README.md`: purpose, deps (`tokenizers`, `onnxruntime`, `numpy`, `rhema.db`), sample commands, interpretation.

#### F2. Optional CI

- Not default CI (heavy ONNX).  
- Document as manual gate before any corpus composition change.  
- Or `#[ignore]` Rust integration later.

#### F tests

| ID | Test | Assertion |
|----|------|-----------|
| T-F1 | Scripts importable / `--help` or dry-run | Exit 0 without full embed |
| T-F2 | Small fixture mode | n=20 verses completes &lt; 2 min |
| T-F3 | Golden: on current public composition, KJV-alone trunc rate 0; multi-lang separate means untruncated | Within tolerance of prior table |

---

### WP-G — Latency instrumentation (item #7)

#### G1. Vector search timing

**File:** `hnsw_index.rs` `search`  
- `let t0 = Instant::now();` … log at `log::debug!` or `info!` throttled:  
  `[VECTOR] search n={len} k={k} took {elapsed:?}`

#### G2. Detection pipeline stages

In the path used by live/accuracy (e.g. `pipeline` semantic + FTS):
- Time FTS leg and semantic leg separately if not already.  
- Prefer **one structured log line** per `detect` / accuracy case:  
  `fts_ms=… semantic_embed_ms=… semantic_search_ms=… merge_ms=… total_ms=…`

Reuse `detection_accuracy` case timer; break out stages.

**Why:** Report showed embed 4.44× faster but e2e only −6.8 ms p50. Optimizing without stage times is guesswork.

#### G3. Do **not** optimize in this WP

No ANN, no ensemble reduction, no threshold games—**measure only**.

#### G tests

| ID | Test | Assertion |
|----|------|-----------|
| T-G1 | Unit with fake Instant hard | search still correct (existing hnsw tests) |
| T-G2 | Manual | One detection log contains stage fields |
| T-G3 | Report | Capture p50 embed / search / fts / total on same host as split report |

---

### WP-H — Score policy: measure, decide, document, maybe retune (#5, #6)

#### H0. Freeze baseline (before any constant change)

```text
cargo run --manifest-path src-tauri/Cargo.toml -p rhema-detection --features precompute-bin --release --bin score_distribution -- \
  --embeddings embeddings/public-minilm-l6-v2-q8.bin \
  --ids embeddings/public-minilm-l6-v2-q8-ids.bin
```

Also:

```text
detection_accuracy --threshold 0.90 ... --min-precision 0.988 --min-recall 0.80
```

Save stdout to `docs/reports/…-score-baseline.md`.

#### H1. Document ensemble invariant (code + test) — **no behavior change**

**File:** `ensemble.rs` module docs:

> Ensemble `score` filter requires enough mass that a result with **no original-strategy hit** cannot pass: max synonym+concept contribution is `SYNONYM_WEIGHT + CONCEPT_WEIGHT = 0.3 < ENSEMBLE_THRESHOLD 0.42`. Synonym/concept only corroborate original hits (or raise combined score when original already present). Display confidence uses `best_similarity` (see `detector.rs`).

**Test T-H1:**
```rust
// pure arithmetic unit test (no ONNX)
assert!(SYNONYM_WEIGHT + CONCEPT_WEIGHT < ENSEMBLE_THRESHOLD);
assert!((ENSEMBLE_THRESHOLD / ORIGINAL_WEIGHT - 0.6).abs() < 1e-9); // original-only bar
```

**Why:** Makes implicit product rule explicit; prevents “fix” that only tweaks synonym weight without understanding discovery.

#### H2. Product decision (recorded in final report)

Choose **one**:

| Decision | Meaning | Follow-on code |
|----------|---------|----------------|
| **H2-corroboration** (status quo) | Original required for admission | Docs + T-H1 only |
| **H2-discovery** | Synonym/concept may surface alone | Lower threshold **or** separate gate on `best_similarity` only; redesign score filter |

**Default recommendation:** **H2-corroboration** unless product wants synonym-only recovery (rare for English MiniLM on sermons).

#### H3. Retune only if histograms demand it

**Inputs:** `score_distribution` quote vs para vs prose tops; FP rate on `detection_accuracy`.

**Possible knobs (touch one family at a time):**

| Knob | File | When to raise |
|------|------|----------------|
| `ORIGINAL_CUTOFF` / `ENSEMBLE_THRESHOLD` | `ensemble.rs` | Prose noise tops often &gt; 0.42 |
| Detector `DEFAULT_CONFIDENCE_THRESHOLD` | `detector.rs` | Same for displayed confidence |
| `DECISIVE_SEMANTIC_CONFIDENCE` / `MIN_AMBIGUITY_MARGIN` | `deepseek-ranker.ts` | Ranker over-invoked or under |

**Rules:**
- One PR per knob family.  
- Re-run accuracy gate after each.  
- No change if baseline already meets precision/recall **and** prose probes stay below operator floor.

#### H tests

| ID | Test | Assertion |
|----|------|-----------|
| T-H1 | Ensemble arithmetic | No-original max &lt; threshold |
| T-H2 | Optional behavior test | Fixture: synonym-only sources never appear in ensemble results at threshold (if H2-corroboration) |
| T-H3 | Accuracy gate | ≥ 0.988 precision, ≥ 0.80 recall after any retune |
| T-H4 | `set_use_synonyms(false)` A/B | Document precision/recall delta (optional one-pager) |

---

## 3 · Phased execution order

```text
Phase 0  Baseline capture (accuracy + score_distribution + note vector count)
    │
Phase 1  WP-A footgun legacy + WP-C live_probe + WP-B comments/scripts
    │      (single PR or stacked PR1a/PR1b)
Phase 2  WP-D delete local dead bins; doc greps
    │
Phase 3  WP-E manifest write + optional load check
    │
Phase 4  WP-F promote harnesses
    │
Phase 5  WP-G instrumentation + capture stage timings
    │
Phase 6  WP-H document ensemble + decide; retune only if data says so
    │
Phase 7  Final report + CODEBASE.md update
```

**Rationale:** A/C are active footguns (minutes–hours). D depends on A/C. E/F make later changes safe. G before H so retune isn’t confused with latency work. H last.

**Do not** combine H retune with A in one PR—regression blame becomes impossible.

---

## 4 · Detailed change checklist by file

| File | WP | Change |
|------|----|--------|
| `src-tauri/src/asset_paths.rs` | A | Remove legacy pair; tests; optional reject |
| `src-tauri/src/lib.rs` | A, E | Fail message; optional manifest count check; keep sanity |
| `src-tauri/crates/detection/src/bin/live_probe.rs` | C | Defaults + docs |
| `src-tauri/crates/detection/src/semantic/onnx_embedder.rs` | B | Comments only (behavior already BatchLongest) |
| `data/precompute-embeddings*.py` | B | Exit or fix padding claim |
| `package.json` | B | Soft-remove or warn py precompute scripts |
| `data/compute-embeddings.ts` | E | Write manifest |
| `data/compute-embeddings.test.ts` | E | Manifest assertions |
| `data/benchmarks/*` | F | Promoted scripts + README |
| `hnsw_index.rs` | G | Search timing log |
| `pipeline.rs` / `detection_accuracy.rs` | G | Stage timings as needed |
| `ensemble.rs` | H | Docs + arithmetic test; constants only if retune |
| `detector.rs` / `deepseek-ranker.ts` | H | Only if retune |
| `docs/CODEBASE.md` | 7 | Candidate list, no legacy, manifest, public-q8 probe |
| `docs/release-checklist.md` | D/E | Force export notes; no legacy |
| `README.md` | B/C | Precompute path; live_probe paths |

---

## 5 · Non-breakage / regression matrix

| Scenario | Expected after plan |
|----------|---------------------|
| Public q8 present | Load q8; log multi-vector public; vector count ~155345 |
| Public f32 only | Load f32 |
| Only legacy present | **Semantic disabled**; clear regenerate instructions |
| `live_probe` no args | Uses public-q8; n≈155345 |
| `detection_accuracy` gate | Still ≥ floors (no intentional threshold change in P1) |
| Manual mode detection batches | Unaffected |
| Auto Preview stale AI await fix | Unaffected (separate prior work) |
| Bundle resources | Still q8 public names |

---

## 6 · Full test plan (evidence)

### Automated (every PR in Phases 1–6 as applicable)

```text
# Rust asset + detection
cargo test --manifest-path src-tauri/Cargo.toml -p rhema-app asset_paths
cargo test -p rhema-detection --features onnx,vector-search
# package name may be rhema / sabbathcue — use workspace test for asset_paths crate

cargo test --manifest-path src-tauri/Cargo.toml --workspace

# TS export / frontend if E/F touch TS
bun test data/compute-embeddings.test.ts
bun run typecheck   # if frontend packages affected
```

### Accuracy gate (Phase 0 baseline + Phase 6 if retune + Phase 7)

```text
cargo run --manifest-path src-tauri/Cargo.toml -p rhema-detection --features precompute-bin --release --bin detection_accuracy -- \
  --threshold 0.90 \
  --embeddings embeddings/public-minilm-l6-v2-q8.bin \
  --ids embeddings/public-minilm-l6-v2-q8-ids.bin \
  --min-precision 0.988 --min-recall 0.80
```

### Score distribution (Phase 0 + Phase 6)

```text
cargo run ... --bin score_distribution -- \
  --embeddings embeddings/public-minilm-l6-v2-q8.bin \
  --ids embeddings/public-minilm-l6-v2-q8-ids.bin
```

### Manual footgun proof (Phase 1)

1. Temporarily rename `public-minilm-l6-v2*` out of the way; leave only `kjv-minilm*`.  
2. Start app / load semantic.  
3. **Expect:** semantic disabled; **must not** log 31102 vectors loaded from legacy.  
4. Restore public files; **expect:** 155345 (or q8 equivalent) loaded.

### Latency capture (Phase 5)

1. Run accuracy or live_probe with new stage logs.  
2. Record p50 for embed / search / fts / total.  
3. Confirm search scales with n≈155k (order of tens of ms class on prior host).

### Harness (Phase 4)

```text
python data/benchmarks/measure_blend_tokens.py   # or promoted path
python data/benchmarks/retrieval_ab_corpus.py --n 200   # smaller smoke
```

---

## 7 · PR strategy

| PR | Contents | Gate |
|----|----------|------|
| **PR1** | WP-A + WP-C + WP-B | Unit tests; footgun manual; workspace tests |
| **PR2** | WP-E manifest write + tests; soft runtime warn | Export tests |
| **PR3** | WP-F harness promotion | Scripts run smoke |
| **PR4** | WP-G instrumentation | Logs visible; no behavior change |
| **PR5** | WP-H docs + T-H1; optional retune | Accuracy gate |

WP-D is a local ops step noted in PR1 description (not necessarily committed).

---

## 8 · Rollback

| If | Then |
|----|------|
| Installs break without public bins | Restore legacy pair **behind env** `SABBATHCUE_ALLOW_LEGACY_EMBEDDINGS=1` only if emergency—not default |
| Manifest false positive | Soft-warn only; disable hard fail |
| Retune hurts precision | Revert constant PR; keep docs/tests for ensemble arithmetic |
| Timing logs too noisy | Drop to `debug` level |

Keep pre-change public-q8 copy until Phase 7 report signed off.

---

## 9 · Success definition (done means)

1. No code path loads `kjv-minilm-l6-v2` without an explicit emergency env (default: never).  
2. Missing public corpus → semantic **off**, not legacy **on**.  
3. `live_probe` defaults = production public-q8.  
4. Comments/scripts do not instruct Fixed(128) production rebuilds.  
5. Dead 31k bins removed from this machine (and documented for others).  
6. Manifest written on export; load can detect count mismatch.  
7. Blend/retrieval harnesses live under tracked `data/benchmarks/`.  
8. Stage timings logged for search (+ FTS if in path).  
9. Ensemble original-required rule documented + unit-tested.  
10. Any threshold change backed by score_distribution + accuracy gate.  
11. Final report filed; `docs/CODEBASE.md` updated.

---

## 10 · Final report template

Copy to `docs/reports/YYYY-MM-DD-post-split-footguns-fix-report.md` after execution:

```markdown
# Report: Post-split footguns fix

Date / commit / machine:

## Phase 0 baseline
- Vector count loaded:
- detection_accuracy P/R:
- score_distribution summary (quote/para/prose):

## Changes shipped
- [ ] Legacy candidate removed
- [ ] Compatibility reject rules:
- [ ] live_probe defaults:
- [ ] Comment/script cleanup:
- [ ] Dead bins deleted (yes/no):
- [ ] Manifest path + fields:
- [ ] Harnesses path:
- [ ] Timing logs:
- [ ] Ensemble decision (corroboration vs discovery):
- [ ] Threshold deltas (table before→after or "none"):

## Evidence
- Manual legacy-only load: (disabled / unexpected)
- live_probe n= 
- Unit tests: 
- detection_accuracy after: 
- Stage timing p50: embed / search / fts / total
- Harness smoke:

## Deviations from plan
- …

## Conclusion
- Footguns: closed / residual
- Latency next target (from timers):
- Score policy: stable / retuned
```

---

## 11 · Appendix — key receipts

| Topic | Locator |
|-------|---------|
| Legacy constants | `asset_paths.rs:16–17` |
| Candidate order | `asset_paths.rs:372–380` |
| MiniLM substring check | `asset_paths.rs:55–69` |
| Sanity Gen 1:1 ≥ 0.80 | `lib.rs:32–38` |
| Corpus log label | `lib.rs:165–173` |
| live_probe defaults | `live_probe.rs:33–36` |
| Ensemble weights/threshold | `ensemble.rs:9–20,126–130` |
| best_similarity gate | `detector.rs` ensemble branch |
| BatchLongest | `onnx_embedder.rs` padding |
| Python Fixed pad | `precompute-embeddings*.py` |
| Preferred q8 name | `asset_paths.rs:12–13` |
| Split report metrics | `docs/reports/2026-08-03-split-corpus-dynamic-padding-report.md` |
| Footgun verification | `docs/reports/2026-08-03-post-split-footguns-verification.md` |

---

## 12 · Implementation operator checklist

- [ ] Phase 0 baselines saved  
- [ ] PR1: A+C+B green  
- [ ] Manual legacy-only fail-closed  
- [ ] WP-D delete bins  
- [ ] PR2: manifest  
- [ ] PR3: benchmarks promoted  
- [ ] PR4: timers + stage table  
- [ ] PR5: ensemble docs + optional retune  
- [ ] Final report  
- [ ] CODEBASE.md changelog line  

---

*End of plan. Do not retune thresholds in the same change set as legacy removal. Measure before optimizing latency or scores.*
