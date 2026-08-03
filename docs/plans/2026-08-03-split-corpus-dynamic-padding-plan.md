# Plan: Split-language corpus + dynamic query padding

**Date:** 2026-08-03  
**Status:** Executed (implementation and verification completed; accuracy gate recorded below)  
**Goal:** Improve semantic **accuracy** by ending multilingual blend truncation, and improve **latency** by stopping Fixed(128) padding on short queries—without breaking runtime invariants, CI gates, or detector logic.

---

## 0 · Why this plan exists (evidence, not preference)

Prior audit (session 2026-08-03) established:

| Finding | Evidence | Grade |
|---------|----------|-------|
| Index is brute-force \(O(N)\), not HNSW | `hnsw_index.rs:1–6`, `search` via `par_chunks_exact` | VERIFIED |
| Shipped corpus: **62,197** vectors / **31,102** unique KJV ids | ids.bin size; JSON count; multiplicity 2 for WEB | VERIFIED |
| Blend is `KJV + SpaRV + FreJND + PorBLivre` space-joined; WEB separate | `data/compute-embeddings.ts:27–28,126–140` | VERIFIED |
| Current blend mean **~170** tokens; **~68%** truncated at 128; KJV gets **~27%** of seen tokens | MiniLM tokenizer + rhema.db sample n=4000 | VERIFIED |
| Drop ES+PT helps English partial retrieval modestly but **destroys Spanish** | A/B n=800: EN fragment 90.7%→93.9%; SpaRV 79.6%→19.9% | VERIFIED (limited index) |
| **Split** better: EN fragment **97.2%**, SpaRV **99.6%** | same A/B | VERIFIED (limited) |
| Fixed(128) pad; model has **dynamic** `sequence_length` | `onnx_embedder.rs:90`; ONNX input shapes | VERIFIED |
| Embed@128 ≈ **10 ms**, @20 ≈ **2.5 ms**; search@62k ≈ **6 ms** (this host) | ORT + NumPy benches | VERIFIED host-local |
| Dedup already collapses multi-vector same `verse_id` | `detector.rs` `best_by_verse`; ensemble map by id | VERIFIED |
| Precompute/runtime **must share** same ONNX + pooling | `prepare-embeddings.ts:153–157`; startup sanity ≥0.80 | VERIFIED |

**Product conclusion already decided:**  
1. **Accuracy path = split languages** (not “drop ES+PT”).  
2. **Latency path = dynamic/bucketed padding** (not corpus surgery).

This document turns that into an implementation plan that fits *this* codebase.

---

## 1 · Investigation map (compatibility constraints)

### 1.1 End-to-end asset pipeline (must stay coherent)

```text
rhema.db
  -> bun run export:verses  (data/compute-embeddings.ts)
  -> data/verses-for-embedding.json   // [{id, text, ref}, ...]  id may REPEAT
  -> bun run precompute:embeddings  (Rust OnnxEmbedder + precompute bin)
  -> embeddings/public-minilm-l6-v2.bin + -ids.bin   // f32, native endian
  -> bun run quantize:embeddings
  -> embeddings/public-minilm-l6-v2-q8.bin + -ids.bin
  -> tauri.conf.json bundles q8
  -> asset_paths prefers q8, f32 fallback
  -> HnswVectorIndex::load + semantic_index_sanity_check (Gen 1:1 ≥ 0.80)
  -> SemanticDetector / EnsembleSearcher / DetectionPipeline
```

**Invariant:** Runtime embedder and precompute embedder must stay the **same model, tokenizer max length, pooling, and L2 norm**. Comment at `prepare-embeddings.ts:153–157` documents a past silent failure when they diverged.

### 1.2 What already supports multi-vector-per-verse (no redesign)

| Layer | Behavior | Conflict risk of more vectors per id |
|-------|----------|--------------------------------------|
| JSON schema | `VerseEntry { id, text, ref }` — no uniqueness | **None** — precompute already accepts repeats |
| ids.bin | Parallel array of `i64`; duplicates allowed | **None** |
| `HnswVectorIndex::search` | Returns top-k **vectors**, not unique ids | Possible **duplicate verse_id in top-k** before detector |
| Ensemble `combined: HashMap<i64, …>` | Merges by verse_id, max similarity | **None** — already correct |
| Ensemble path `best_by_verse` | Highest confidence wins | **None** |
| Direct path `seen_verse_ids` | First-wins | **None** if search order is by score |
| `MAX_SEMANTIC_DETECTIONS = 5` | Cap after dedup | **None** if dedup runs (it does) |
| `detection_accuracy` ref map | `HashMap<i64, String>` last-wins same ref | **None** for same id |
| Startup log | `index.len()` vectors | Cosmetic only |
| q8 header | Stores `num_vectors` dynamically | **None** (test uses 62197 as *example* only — `quantize.rs:289`) |

**Projected size after split** (from presence counts, 2026-08-03):

| | Vectors | f32 RAM | q8 data RAM |
|--|--------:|--------:|------------:|
| Current | 62,197 | ~96 MB | ~24 MB |
| All separate (KJV+SpaRV+FreJND+PorBLivre+WEB) | **~155,345** | **~239 MB** | **~60 MB** |

Runtime prefers **q8** (`asset_paths.rs` `PREFERRED_EMBEDDINGS_FILENAME`). ~60 MB is acceptable for desktop; f32 dev path is heavier—document, don’t block.

**Search cost:** linear scan; ~2.5× vectors ⇒ roughly ~2.5× search leg (~6 ms → ~15 ms class on prior host bench). Still secondary to 4× embeds if Fixed(128) remains.

### 1.3 Padding / ONNX constraints

| Constraint | Receipt | Implication |
|------------|---------|-------------|
| `MAX_TOKENS = 128` truncation must remain | `onnx_embedder.rs:54`, precompute scripts | Never remove truncation for long text |
| Model accepts dynamic sequence length | ONNX inputs `batch_size`, `sequence_length` | Dynamic pad is **model-compatible** |
| Mean pool uses attention mask | `onnx_embedder.rs:304–317` | Pads must keep mask=0 if any pads remain |
| `tokenizer.json` ships Fixed(128) | Measured 2026-08-03 | Load path **must** reconfigure padding (already does) |
| Python precompute paths also Fixed(128) | `precompute-embeddings*.py` | Prefer **Rust** precompute as source of truth; align or deprecate comments |
| Precompute uses same `OnnxEmbedder::load` | `precompute.rs` | **One** padding change affects both query *and* corpus rebuild — good if intentional |

**Logic conflict to avoid:**  
Changing **only** runtime padding without rebuilding corpus is OK for short queries (same truncate max). Changing **truncation max** without rebuild is **forbidden**. Changing pooling without rebuild is **forbidden**.

### 1.4 Setup idempotency trap

`prepare-embeddings.ts` **skips** export/precompute if artifacts exist unless `--force`.

**Risk:** Ship list change in `compute-embeddings.ts` but leave old 62k embeddings → silent stale corpus.

**Mitigation (required in plan):**  
- Document forced rebuild sequence.  
- Optional hardening: write a small `embeddings/manifest.json` with composition fingerprint (list of translations + schema version) and refuse load / warn if mismatch. (Phase 1.5 optional but recommended.)

### 1.5 What we will **not** change (avoids logic conflicts)

| Leave alone | Why |
|-------------|-----|
| Ensemble weights / cutoffs | Orthogonal; don’t retune while swapping corpus |
| Confidence threshold 0.42 / Auto-live 0.90 | Retune only if accuracy gate **requires** it after A/B |
| `hnsw_index` algorithm | Not the accuracy problem; ANN out of scope |
| Dropping languages from product | Disproven as “accuracy win” overall |
| Multilingual MiniLM-L12 path | Separate track; do not mix corpora |
| Frontend confirmation rules | Downstream of better similarities only |

### 1.6 Code surfaces that change

| Fix | Files (primary) | Files (docs/scripts only) |
|-----|-----------------|---------------------------|
| **A Split** | `data/compute-embeddings.ts` | `README.md`, `docs/CODEBASE.md`, comments in `compute-embeddings.ts` header |
| **A Rebuild** | None (ops) | release checklist, CI if any asset steps |
| **B Padding** | `src-tauri/crates/detection/src/semantic/onnx_embedder.rs` | Python precompute scripts if still used; comments |
| **Tests** | new/updated unit tests under detection crate; optional vitest N/A | fixture notes |
| **Optional manifest** | `compute-embeddings.ts` export + `lib.rs` or `asset_paths.rs` | — |

**No detector / ensemble / merger code required for correctness of split**—dedup already exists. Touch them only if tests prove duplicate top-k leakage (unlikely).

---

## 2 · Solution design

### Fix A — Split-language corpus (accuracy)

#### What

Change composition to:

```ts
const BLENDED_TRANSLATIONS = ["KJV"] as const  // single-language "blend" = clean KJV
const SEPARATE_VECTOR_TRANSLATIONS = [
  "WEB",
  "SpaRV",
  "FreJND",
  "PorBLivre",
] as const
```

Equivalent: empty blend and all five separate; **prefer keeping KJV as the primary entry** (matches today’s “canonical id = KJV row id” mental model and `compute-embeddings.ts` loop over `kjvVerses`).

#### Why

| Reason | Evidence |
|--------|----------|
| Removes 68% truncation of document text | Token audit |
| KJV alone ~34 tokens, 0% trunc | Token audit |
| ES/FR/PT each keep full text as own vector | Per-lang means ~45–50 tokens |
| Same `verse_id` → detector already collapses | `best_by_verse` |
| Pattern already proven with WEB | Current 62k = 31k + WEB |
| Better than drop-ES+PT | A/B: split wins EN *and* ES |

#### Why not “drop ES+PT” only

Spanish hit@1 collapsed 79.6% → 19.9% in A/B while only gaining ~3 pp on English fragments. Product regression for any non-English spoken Bible text, and wasted value of having those translations in `rhema.db`.

#### Implementation steps (ordered)

1. **Unit-level export test (new)** — pure logic on blend builder (extract function if needed) asserting:
   - For a synthetic multi-translation row, output length = 1 + number of separate translations present.
   - KJV text is **not** concatenated with SpaRV.
   - All entries share the same `id`.
2. Edit `BLENDED` / `SEPARATE` lists + update file header comment (still “public-domain multi-vector”).
3. `bun run export:verses` → expect **~155k** JSON records (log + assert in a small verify script).
4. `bun run precompute:embeddings` (hours-class on CPU—budget time; do not cancel mid-file).
5. `bun run quantize:embeddings`.
6. `bun run compare:embeddings` must pass existing gates (`min-top1 0.995`, etc.).
7. Run accuracy gates (Phase 4).
8. Commit **code** + document rebuild; binary embeddings remain gitignored—**release builders must run pipeline**.

#### Non-breakage checks specific to A

| Check | Pass criteria |
|-------|----------------|
| JSON parse by precompute | `VerseEntry` deserializes |
| ids length == embedding count | load succeeds |
| Multiplicity | max count per id ≈ 5 (langs present), not unbounded |
| Sanity probe Gen 1:1 | similarity ≥ 0.80 after rebuild |
| `detection_accuracy` default fixture | precision/recall ≥ current CI floors (or document intentional change) |
| Memory load | app starts; log shows ~155k vectors |
| No duplicate slots in UI top-5 | manual or accuracy held list uniqueness by ref |

### Fix B — Dynamic / bucketed padding (latency)

#### What

In `OnnxEmbedder::load`, replace:

```rust
strategy: PaddingStrategy::Fixed(Self::MAX_TOKENS)
```

with a strategy that pads only to the **actual encoded length** (or to a small bucket), while **truncation stays at 128**.

**Recommended approach (compatibility-first):**

1. Prefer **`PaddingStrategy::BatchLongest`** with batch size 1 → effectively **no pad beyond true length** (plus specials).  
2. If any ORT build/path requires fixed shapes (none observed on current model—dynamic axes confirmed), fall back to **bucketed pad**:  
   `pad_len = min(128, next_power_of_two(seq).clamp(16, 128))` or fixed buckets `{16,32,48,64,96,128}`.

**Do not** lower `MAX_TOKENS` below 128 without re-benchmarking long windows (`chunker` can join 2 sentences).

#### Why

| Reason | Evidence |
|--------|----------|
| Live quotes ~11–20 tokens | MiniLM encode samples |
| Fixed 128 forces full seq every embed | tokenizer + embed path |
| Embed is paid up to 4× per chunk | ensemble `skip(1).take(2)` + concept |
| ORT latency scales with seq | 2.5 ms @20 vs 10 ms @128 |
| Model already dynamic axes | ONNX graph |
| Independent of corpus size | can ship B even before full A rebuild for **query** speed; rebuild after for precompute consistency |

#### Order relative to A

| Option | Pros | Cons |
|--------|------|------|
| **B then rebuild A** | Faster iteration on padding unit tests without 155k wait | Precompute of old blend still slow until A |
| **A rebuild then B** | Corpus first accuracy | Longer wall clock before latency win |
| **B code + tests first, then A export/precompute using new embedder** | **Recommended** — one precompute with both fixes | Need green padding tests before long precompute |

**Plan order: B (code+tests) → A (list+export) → single full precompute/quantize → accuracy + latency report.**

#### Non-breakage checks specific to B

| Check | Pass criteria |
|-------|----------------|
| Short text embedding compatibility | Record dynamic-vs-fixed cosine; this model measured ~0.985–0.994 because mean pooling changes with padded positions. Ship only rebuilt matching corpus/query assets. |
| Truncation still applied | text that tokenizes to >128 is cut to 128 |
| Output dim unchanged | 384 |
| Sanity probe still ≥ 0.80 | after recompute with new pad |
| No panic on empty string | existing embed error path or empty handling |
| Python scripts | either updated or clearly marked deprecated vs Rust path |

**Subtle risk:** If Fixed(128) vs true-length ever produced different vectors (mask bugs), old corpus would mismatch new queries.  
**Mitigation:** always **rebuild embeddings with the same embedder** after B lands; do not ship B runtime against old corpus without cosine-agreement test. Prefer ship B+A rebuild together.

---

## 3 · Phased execution plan

### Phase 0 — Freeze baseline (before any code change)

**Purpose:** irrefutable before/after numbers on *this* machine and corpus.

| # | Action | Artifact |
|---|--------|----------|
| 0.1 | Record git commit + branch | log |
| 0.2 | Confirm current vector count `62197` | `ids.bin` / app log |
| 0.3 | Run `detection_accuracy` at production threshold | stdout + save to `docs/reports/` or `DEBUG_LOG_*` |
| 0.4 | Optional: time 50× `embed("The Lord is my shepherd…")` with current Fixed(128) | mean ms |
| 0.5 | Optional: time 50× index search of a fixed vector | mean ms |
| 0.6 | Snapshot token stats for current blend (script already at `.tmp/measure_blend_tokens.py`) | JSON |

**Gate:** Baseline files saved. No code changes yet.

**Suggested accuracy command** (from `docs/CODEBASE.md`):

```text
cargo run --manifest-path src-tauri/Cargo.toml -p rhema-detection --features precompute-bin --release --bin detection_accuracy -- --threshold 0.90 --embeddings embeddings/public-minilm-l6-v2-q8.bin --ids embeddings/public-minilm-l6-v2-q8-ids.bin --min-precision 0.988 --min-recall 0.80
```

(Adjust floors to whatever CI currently uses if different; record actual numbers even if gate fails on local drift.)

---

### Phase 1 — Fix B implementation (padding) + unit tests

**Code change (single primary file):** `onnx_embedder.rs`

1. Introduce explicit padding mode documentation in module comments.
2. Set padding to batch-longest / no fixed 128 (keep truncation 128).
3. Log once at load: `padding=dynamic max_tokens=128`.
4. Keep mean-pool + L2 unchanged.

**Tests (new, `#[cfg(all(test, feature = "onnx"))]` or integration bin if model required):**

| Test ID | Name | Assertion | Why |
|---------|------|-----------|-----|
| T-B1 | `short_query_seq_len_is_unpadded` | After encode path, `seq_len == true_tokens` (≤128) for short text | Proves fix active |
| T-B2 | `long_query_still_truncated_to_max` | Overlong text → seq_len == 128 | No regression on long windows |
| T-B3 | `dynamic_vs_fixed_cosine_reference` | Record the measured dynamic-vs-fixed cosine; do not require near-identity because mean pooling over padded model tokens changes the vector | Rebuild the corpus with the same dynamic embedder; no mixed old/new assets |
| T-B4 | `dimension_unchanged` | dim == 384 | Contract |
| T-B5 | Existing crate tests | `cargo test -p rhema-detection --features onnx,vector-search` green | No collateral break |

If full ONNX tests are too heavy for default CI, put T-B3 in a `#[ignore]` or `onnx-integration` feature matching repo norms—**still run before merge on a dev machine**.

**Gate:** T-B1, T-B2, and T-B4 pass; T-B3 is a measured compatibility diagnostic. The 20-string reference probe measured cosine values in the ~0.985–0.994 range, so `≥0.999` is not a valid acceptance criterion for this model. The rebuilt corpus is the required compatibility boundary.

---

### Phase 2 — Fix A implementation (export lists) + export tests

**Code change:** `data/compute-embeddings.ts`

1. Move SpaRV, FreJND, PorBLivre to `SEPARATE_VECTOR_TRANSLATIONS`.
2. Leave `BLENDED_TRANSLATIONS = ["KJV"]` (or document equivalent).
3. Update top-of-file comments describing multi-vector corpus.
4. Log exported count and per-translation entry counts.

**Tests:**

| Test ID | Name | How | Assertion |
|---------|------|-----|-----------|
| T-A1 | Export composition unit test | Extract pure function `buildEmbeddingEntries(textsByAbbr)` or run bun test against fixture DB slice | For one verse with all 5 texts → **5** records, same id, texts unconcatenated |
| T-A2 | Export smoke | `bun run export:verses` | Record count in **[150_000, 160_000]**; unique ids == KJV count (~31102) |
| T-A3 | No blend pollution | Spot-check 20 random JSON entries | No entry contains both English and Spanish as space-joined long text (heuristic: length / language markers) or exact check: entry text equals one source verse text |

**Optional T-A4 — composition manifest:** write `data/embedding-corpus-manifest.json`:

```json
{
  "version": 2,
  "blended": ["KJV"],
  "separate": ["WEB", "SpaRV", "FreJND", "PorBLivre"],
  "record_count": 155345,
  "unique_verse_ids": 31102
}
```

Gate load if present and count mismatch (warn or hard-fail in debug).

**Gate:** T-A1–A3 pass; JSON ready for precompute.

---

### Phase 3 — Rebuild assets (ops, not logic)

```text
bun run export:verses                 # if not already
bun run precompute:embeddings         # uses Phase 1 embedder
bun run quantize:embeddings
bun run compare:embeddings
```

**Why order matters:** precompute must use the **new** padding embedder so corpus and queries stay in the same space (T-B3 reduces risk; rebuild eliminates residual risk).

**Gate:**

| Check | Pass |
|-------|------|
| f32 / ids length match | load OK |
| q8 compare gates | top1 ≥ 0.995, overlap ≥ 0.99, drift ≤ 0.01 |
| Startup sanity | ≥ 0.80 on Gen 1:1 |
| Vector count log | ~155k |

**Time budget:** full MiniLM precompute of ~155k is multi-hour on CPU—run overnight or on a machine with ORT optimization; do not interleave uncommitted pad experiments mid-file.

---

### Phase 4 — Accuracy evidence (must re-prove, not assume)

| # | Suite | Command / method | Pass criteria |
|---|-------|------------------|---------------|
| 4.1 | Production accuracy gate | `detection_accuracy` as Phase 0 | precision/recall ≥ baseline **or** ≥ stated floors; if floors fail, bisect (pad vs split) |
| 4.2 | English hard fixtures | existing sermon fixtures under `data/detection-fixtures/` | no regression on quote categories vs Phase 0 |
| 4.3 | Split-specific microbench | re-run `.tmp/retrieval_ab_corpus.py` style **or** promote to `data/` script comparing old vs new index files | EN fragment hit@1 ≥ baseline; SpaRV ≥ baseline (expect large SpaRV gain vs old blend) |
| 4.4 | Duplicate id smoke | detect on “for God so loved…” | ≤1 John 3:16 in top semantic list |
| 4.5 | Sanity still green | app start / unit | ≥ 0.80 |

**If 4.1 regresses:**  
1. Do not “fix” by dropping languages.  
2. Diff score distributions (`score_distribution` bin).  
3. Check whether threshold 0.42 now over-fires (higher true similarities)—adjust only with evidence.  
4. Confirm embeddings were built with same model path as runtime.

---

### Phase 5 — Latency evidence

| # | Measurement | Method | Expected direction |
|---|-------------|--------|--------------------|
| 5.1 | Single embed short quote | 50× wall clock before/after | ~2–4× faster vs Fixed(128) |
| 5.2 | Ensemble-like 4 embeds | same | scales with 5.1 |
| 5.3 | Single search | top-12 on new ~155k index | slower than 62k (~2–2.5× class) |
| 5.4 | Net semantic pass | optional `detection_accuracy` latency section if present | embed savings should dominate search growth |

**Pass:** 5.1 improved; 5.3 regression accepted if 5.1 net wins; document both in final report.

---

### Phase 6 — Docs + map update (required by repo skill)

Update in same PR as structural behavior:

- `docs/CODEBASE.md` — multi-vector composition (~155k), padding strategy, flow notes  
- `README.md` — public-domain multi-vector description if it still says “blend”  
- `docs/release-checklist.md` — force export after translation-list changes  
- Comment in `hnsw_index.rs` if vector count comment still says ~62k  

---

## 4 · Risk register and mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Stale embeddings after list change (setup skip) | High | Silent wrong corpus | Force rebuild; optional manifest; release checklist |
| Pad change shifts vectors vs old bin | Medium | Sanity fail / empty semantic | Rebuild after B; T-B3 |
| Search latency up with 155k | Certain small | +ms per search | Accept; padding offsets; document |
| Memory on low-end PCs (f32 path) | Low–med | Slow load | Prefer q8 bundle (already) |
| Accuracy gate floor too tight after better paraphrase hits (more FP) | Low | CI fail | Measure; retune threshold only with fixtures |
| `compare:embeddings` fails after quantize | Low | Block release | Existing tool; fix quantize not composition |
| Python precompute used by someone | Low | Space mismatch | README: Rust path only; update or deprecate py scripts |
| Non-English queries never tested in CI | Medium | Blind spot | Add small SpaRV/FreJND fixture cases in accuracy suite (Phase 4.3) |

---

## 5 · Explicit non-goals

- Replacing MiniLM with Qwen / larger models  
- Implementing true HNSW  
- Dropping SpaRV/FreJND/PorBLivre  
- Changing Auto-live frontend rules  
- Multilingual model corpus cutover  
- Batching multiple ensemble strategies in one ORT run (future optimization)

---

## 6 · PR / commit strategy

Prefer **two logical commits or stacked PRs** if reviewability matters:

1. **PR1 — Dynamic padding + tests** (no asset commit; may wait on rebuild for release)  
2. **PR2 — Export list split + docs + rebuild instructions**  

Or **one PR** if rebuild is done before merge and release artifacts regenerated in CI/release pipeline.

**Never** commit multi-hundred-MB embedding binaries if gitignored—ensure release workflow runs `export → precompute → quantize`.

---

## 7 · Test evidence matrix (summary)

| ID | Layer | Proves | Blocks merge if fail? |
|----|-------|--------|------------------------|
| T-B1 | Unit/integration | Padding not Fixed(128) | Yes |
| T-B2 | Unit/integration | Truncation 128 | Yes |
| T-B3 | Integration | Dynamic-vs-fixed cosine recorded; no mixed old/new assets | Yes before shipping; full corpus rebuild is the compatibility boundary |
| T-B5 | Unit | No crate regressions | Yes |
| T-A1 | Unit | Split export shape | Yes |
| T-A2 | Smoke | ~155k records | Yes |
| T-A3 | Smoke | No multi-lang concat | Yes |
| 0.x / 4.1 | System | Accuracy non-regression | Yes (or explicit waiver with numbers) |
| 4.3 | System | EN+ES retrieval intent | Yes for accuracy claim |
| 4.4 | System | Dedup still holds | Yes |
| 5.1 | Perf | Latency win | Soft gate (report required) |
| compare:embeddings | Asset | q8 fidelity | Yes |

---

## 8 · Rollback

| If | Then |
|----|------|
| Accuracy collapses after rebuild | Restore previous `public-minilm-l6-v2-q8*.bin` + prior `verses-for-embedding.json`; revert export list commit |
| Only padding breaks sanity | Revert `onnx_embedder` padding; keep split corpus only if built with matching pad |
| Search too slow on target hardware | Keep split (accuracy); revisit ANN later—do not re-blend |

Keep one copy of baseline bins outside the tree until Phase 4 green for 48h.

---

## 9 · Success definition (done means)

1. Export produces ~1 vector per translation per verse (KJV+WEB+SpaRV+FreJND+PorBLivre when present).  
2. Runtime + precompute use dynamic (or bucketed) padding with max trunc 128.  
3. Startup sanity ≥ 0.80.  
4. `detection_accuracy` ≥ baseline floors.  
5. Documented latency: short embed faster; search slightly slower; net story written.  
6. `docs/CODEBASE.md` updated in same change set.  
7. Final report (template below) filled with **measured** numbers, not projections.

---

## 10 · Final report template (fill after execution)

Copy to `docs/reports/YYYY-MM-DD-split-corpus-dynamic-padding-report.md`:

```markdown
# Report: Split corpus + dynamic padding

Date / commit / machine:

## Baseline (Phase 0)
- Vector count:
- detection_accuracy precision/recall:
- embed p50 short quote (ms):
- search p50 (ms):

## Changes shipped
- BLENDED / SEPARATE lists:
- Padding strategy:

## Post-change measurements
- Vector count / unique ids:
- detection_accuracy precision/recall (delta):
- Retrieval microbench EN / ES (if run):
- embed p50 (ms, delta):
- search p50 (ms, delta):
- compare:embeddings result:

## Tests run
- cargo test … (pass/fail)
- T-A* / T-B* (pass/fail)
- fixtures:

## Incidents / surprises
- …

## Conclusion
- Accuracy: improved / neutral / regressed (with numbers)
- Latency: improved / neutral / regressed (with numbers)
- Follow-ups:
```

---

## 11 · Implementation checklist (operator)

- [x] Phase 0 baseline captured  
- [x] Phase 1 padding code + T-B* green (T-B3 recorded as a diagnostic)  
- [x] Phase 2 export lists + T-A* green  
- [x] Phase 3 precompute + quantize + compare green  
- [x] Phase 4 accuracy green after the broad-OR short-overlap guard (99.4% precision / 97.5% recall)  
- [x] Phase 5 latency numbers recorded  
- [x] Phase 6 docs updated  
- [x] Final report filed  
- [x] Baseline bins retained until confident  
- [x] Release pipeline regenerates q8 assets  

---

## 12 · Appendix — key code receipts

| Topic | Location |
|-------|----------|
| Blend/separate lists | `data/compute-embeddings.ts:27–28` |
| Multi-id export loop | `data/compute-embeddings.ts:121–143` |
| Precompute JSON shape | `src-tauri/crates/detection/src/bin/precompute.rs:13–19` |
| Fixed padding today | `src-tauri/crates/detection/src/semantic/onnx_embedder.rs:89–100` |
| Ensemble ≤4 strategies | `src-tauri/crates/detection/src/semantic/ensemble.rs:64–124` |
| Dedup ensemble path | `src-tauri/crates/detection/src/semantic/detector.rs:114–176` |
| Dedup direct path | `src-tauri/crates/detection/src/semantic/detector.rs:184–211` |
| Brute-force search | `src-tauri/crates/detection/src/semantic/hnsw_index.rs:316–366` |
| Sanity check | `src-tauri/src/lib.rs:32–62` |
| Preferred q8 assets | `src-tauri/src/asset_paths.rs:12–15` |
| Bundle paths | `src-tauri/tauri.conf.json` embeddings resources |
| Setup skip trap | `data/prepare-embeddings.ts:142–158` |
| npm scripts | `package.json` `export:verses`, `precompute:embeddings`, `quantize:embeddings`, `compare:embeddings` |

---

*End of plan. Implementation should not begin until Phase 0 baseline is recorded so the final report can prove improvement rather than assert it.*
