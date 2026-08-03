# Plain-language report: what we checked after the verse-matching upgrade

**Date:** 2026-08-03  
**Audience:** non-engineers and busy operators  
**Technical twin:** `docs/reports/2026-08-03-post-split-footguns-verification.md`

---

## In one minute

We recently improved how the app stores Bible verse “fingerprints” so matching works better (especially English and Spanish). That part looks successful.

But a few **safety nets and old files** are still around in ways that can hide problems:

1. If the new data files go missing, the app can **quietly fall back** to an old English-only set and still look “healthy.”  
2. A **diagnostic tool** still points at an old file the real app no longer uses.  
3. **Old files** (~144 MB) are still on disk.  
4. We have **not yet re-tuned** matching thresholds after the upgrade, or broken down the remaining ~90 ms of detection time.  
5. The scripts that **proved** the upgrade worked live in a temporary folder that git ignores—easy to lose.

**Good news:** When the new files are present (they are on this machine: ~155k fingerprints), the app prefers them. Day-to-day use should be on the new corpus. The risk is mostly **failure paths and future changes**, not “the app is always on junk data right now.”

---

## What the upgrade did (plain English)

| Before | After |
|--------|--------|
| Many verses stored as one long mix of English + Spanish + French + Portuguese | Each language gets its own fingerprint for the same verse |
| Short spoken lines forced through a long padded AI step | Short lines process faster |

**Results we measured earlier (and still trust):**

- English partial-match quality improved (about 91% → 97% in a controlled test).  
- Spanish match quality improved a lot (about 80% → almost 100% in that same style of test).  
- Official accuracy gate after a related bugfix: about **99.4% precision / 97.5% recall**.  
- Full detection time only improved a little (about **101 ms → 95 ms**), even though the AI fingerprint step alone got much faster.

---

## What we rechecked today (so this isn’t stale opinion)

We re-opened the code and re-measured the files on disk. Nothing important has changed since the technical audit.

| Check | Result today |
|--------|----------------|
| New public corpus size | **155,345** fingerprints (31,102 unique verse IDs) |
| Old English-only file | Still present; **31,102** fingerprints; from June |
| Can the app still load that old file as a last resort? | **Yes** — still in the code |
| Does a basic “is this healthy?” test catch the old file as bad? | **No** — Genesis 1:1 still scores ~**0.99** (pass needs only 0.80) |
| Diagnostic tool default file | Still the **June multi-English** file the app doesn’t load |
| Old dead files on disk | Still ~**144 MB** |
| “Composition ID card” for the corpus | **Still missing** |
| Proof scripts for the upgrade | **Still only under `.tmp/`** (gitignored) |
| Matching score cutoffs | **Unchanged** at the old numbers |
| Can synonym-only matches pass the ensemble gate alone? | **No** — math max 0.30 vs needed 0.42 |

So the earlier findings **still stand**.

---

## Issues, in ordinary words

### 1. Old backup corpus can hide a real failure (important)

**What happens:** The app looks for the new files first. If those fail or are missing, it can load an older English-only set from June and continue as if everything is fine.

**Why that’s bad:** You lose the multi-language / split upgrade without a clear failure. Live matching quality can quietly drop.

**What to do:** Remove that fallback, or make the app refuse to start semantic matching until the new files are present.

---

### 2. Misleading instructions for rebuilding data (small but real)

Some code comments still say “must match the old Python scripts,” but those scripts are marked deprecated and still use an old padding style.  
**Risk:** Someone rebuilds data the old way and breaks live search.  
**What to do:** Point all docs at the official Rust rebuild command only.

---

### 3. Health-check tool aims at the wrong target (important for engineers)

The `live_probe` tool, if run with defaults, checks an old June index—not the file the app actually loads.  
**Risk:** “Looks fine in the probe, broken in the app” (or the reverse).  
**What to do:** Point defaults at the real public index, or force the user to pass paths.

---

### 4. Leftover old files (~144 MB)

Three old 31k corpora sit on disk. They’re not needed for the current app path once #1 and #3 are fixed. Safe to delete after that.

---

### 5. Matching thresholds may need a second look (not proven yet)

After the upgrade, good matches tend to score higher. Old “how strong is strong enough?” numbers may now be too loose in theory.  
We have **not** re-run a full score distribution study in this recheck.  
**What to do:** Measure first, then retune carefully—don’t guess.

---

### 6. The “ensemble” has a hidden rule

The multi-strategy matcher (original wording + synonyms + themes) **cannot surface a verse that the original wording search completely missed**, no matter how strong the synonym hit is. Synonyms only help corroborate.  
That may be intentional—but it should be an explicit product choice, not a silent math accident.

---

### 7. Most of the remaining delay is still a mystery

The fingerprint step got much faster; overall detection only got a little faster. Something else still eats most of the ~95 ms. We haven’t timed the search and text-search pieces separately yet.  
**What to do:** Add simple timers before more optimization.

---

### 8–9. Proof and safety paperwork missing

- No small “ID card” file that records *what* corpus was built (languages, count, build rules).  
- The scripts that proved the upgrade work live in a temporary ignored folder.

**What to do:** Save those scripts in a real project folder; add a simple manifest so a wrong or old corpus can’t look “fine.”

---

## What is fine

- With the **new public files present**, the app is designed to prefer them.  
- Official accuracy numbers after the follow-up fix look **good** (99.4% / 97.5% in the written report).  
- The upgrade direction (split languages + faster short embeds) is still the right story.

---

## What to do next (simple priority list)

| Priority | Action | Why |
|----------|--------|-----|
| **1 — Now** | Stop silent fall-back to the old English-only index | Prevents “green but degraded” |
| **1 — Now** | Fix the diagnostic tool’s default paths | Stop false health checks |
| **1 — Soon** | Clean comments/scripts; delete dead files | Avoid rebuild mistakes and clutter |
| **2 — Next** | Time where the remaining ~90 ms goes | Optimize the real bottleneck |
| **2 — Next** | Save the proof scripts + add a corpus “ID card” | Next change is verifiable |
| **3 — After measure** | Revisit score thresholds and ensemble rules | Avoid guessing |

---

## Bottom line for an ordinary user

**You’re not necessarily broken today**—if the new verse index is installed, you’re on the good path.  

**You are under-protected for tomorrow:** missing files, wrong diagnostics, and old backups can make a regression look healthy. Fix the safety nets first; then measure remaining slowness and score rules with eyes open.

---

*Recheck performed 2026-08-03 against live code and local `embeddings/` files. Numbers for accuracy/latency e2e come from the earlier written engineering report and were not re-run as a full accuracy suite in this pass; structural risks were re-confirmed by code + file measurement.*
