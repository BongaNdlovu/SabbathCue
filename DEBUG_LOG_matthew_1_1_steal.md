# DEBUG_LOG — Matthew 1:1 → 1:2 focus steal

## Bug definition (step 1)

```
BUG / TICKET:       matthew-1-1-focus-steal
SYMPTOM (exact):    Live session log 2026-08-04 ~19:51:00 —
                    [DET-DIRECT] Found: Matthew 1:1 (92%)
                    then same second:
                    [DET-DIRECT] Found: Matthew 1:2 (100%)
                    [READING] Started: Matthew 1:2
EXPECTED:           Spoken "Matthew chapter 1 verse 1" stays on Matthew 1:1
ACTUAL:             Chapter-only Matthew 1:1 (92%) often appears first; later
                    Matthew 1:2 (100%) re-anchors reading mode
DELTA:              Live top verse jumps from 1:1 → 1:2 without intentional
                    "verse 2" confirmation in operator intent
REPRO STATUS:       Live observed once in session; deterministic for
                    detector paths that produce verse 2 input (see B.1)
ENVIRONMENT:        Windows, tauri dev, Soniox stt-rt-v5, auto_mode=true,
                    operator 0.70, auto 0.90, bible+semantic on
FIRST OBSERVED:     Live test 2026-08-04 on fix/additive-admin-access
LAST KNOWN GOOD:    unknown
RECENT CHANGES:     N/A for this investigation
IN SCOPE:           direct detector, reading-mode handoff, live DET logs
OUT OF SCOPE:       STT provider models (unless we add transcript logging)
DEFINITION OF FIXED: TBD with user after choosing fix direction
```

## B.1 · Reproduction (step 2)

Live log (verbatim excerpts):

```
[2026-08-04][19:50:47][DET-DIRECT] Found: Matthew 1:1 (92%)
[2026-08-04][19:50:47][READING] Started: Matthew 1:1 (25 verses loaded)
[2026-08-04][19:50:47][READING] Context set to ExpectingChapter
...
[2026-08-04][19:51:00][DET-DIRECT] Found: Matthew 1:1 (92%)
[2026-08-04][19:51:00][DET-TRACE] seq=129 decision=direct emitted=1 top=Matthew 1:1
[2026-08-04][19:51:00][DET-DIRECT] Found: Matthew 1:2 (100%)
[2026-08-04][19:51:00][DET-TRACE] seq=132 decision=direct emitted=1 top=Matthew 1:2
[2026-08-04][19:51:00][READING] Started: Matthew 1:2 (25 verses loaded)
```

Deterministic DirectDetector experiments (cargo test harnesses, 2026-08-04):

```
Single-shot true verse-1 phrases → ALWAYS Matthew 1:1 @ 1.00, chapter_only=false
  "Matthew chapter 1 verse 1"
  "Matthew chapter one verse one"
  "Matthew 1:1"
  "Matthew 1 1"
  "Matthew chapter 1 and verse 1"

Streaming digits:
  "Matthew chapter 1" → Matthew 1:1 @ 0.92 chapter_only=true
  "Matthew chapter 1 verse 1" → Matthew 1:1 @ 1.00 chapter_only=false

Risky completion after chapter-only:
  "Matthew chapter 1" → 1:1 @ 0.92 chapter_only=true
  "2"                → 1:2 @ 1.00 chapter_only=false   ***

Also yields 1:2 @ 1.00:
  "Matthew chapter 1 verse 2"
  "Matthew chapter 1 and verse 2"
  "verse 2" after incomplete Matthew ch1
```

RELIABILITY: detector phrase-correctness 5/5 on true verse-1; 1:2 steal 1/1 when input is verse 2 or bare "2" after incomplete.

## B.2 · Evidence captured (step 3)

```
OBSERVED:
- 92% == CHAPTER_ONLY_CONFIDENCE (detector.rs:792)
- Chapter-only path surfaces verse_start=1 while holding incomplete
  (detector.rs:1166-1233)
- Full "verse N" completion uses compute_confidence → ~1.00
- should_restart_reading re-anchors when same book+chapter and
  recent.verse_start > current (detection_logic.rs:210-217)
- Live logs do NOT include transcript text, only char counts

DIFFERENTIAL:
- John 3:1 (92% chapter-only) then John 3:16 (100%) same pattern,
  intentional refinement
- True "Matthew … verse 1" never off-by-ones to 2 in unit harness
```

## B.3 · Hypothesis log (step 4)

```
H1: Off-by-one parser maps "verse 1"/"verse one" → verse 2
    Predicts: single-shot "Matthew chapter 1 verse 1" → 1:2
    Test: cargo DirectDetector single-shot suite
    Result: ELIMINATED (always 1:1)

H2: Chapter-only early emit (92%) + later higher-verse re-anchor
    is the UX mechanism of the "steal"
    Predicts: 1:1@0.92 then 1:2@1.00 starts reading at 1:2
    Test: live log + should_restart_reading code
    Result: CONFIRMED (mechanism)

H3: Input text for the 1:2 emission contained verse 2 (STT or bare digit)
    Predicts: only paths that yield 1:2@~1.00 require "2"/verse 2 text
    Test: exhaustive phrase matrix
    Result: CONFIRMED as necessary condition; exact live STT string
            UNKNOWN (not logged)

H4: Bare number after chapter-only is high-risk for false refinement
    Predicts: incomplete + "2" → Matthew 1:2 @ 1.00
    Test: DirectDetector stream
    Result: CONFIRMED behavior exists (may or may not be live cause)

H5: Reading mode invents 1:2 without detector
    Predicts: READING advance without DET-DIRECT Found
    Test: live log
    Result: ELIMINATED (DET-DIRECT Found: Matthew 1:2 logged)
```

## B.5 · Root cause (step 6)

```
ROOT CAUSE (mechanism, confirmed):
  Chapter-only citations emit Matthew 1:1 at 0.92 and start reading mode.
  A subsequent direct detection for Matthew 1:2 at full confidence is treated
  as a higher-verse re-anchor (should_restart_reading), so live focus jumps
  to 1:2.

ROOT CAUSE (why 1:2 was produced, partially confirmed):
  DirectDetector only emits 1:2 when the transcript segment resolves to verse 2
  (e.g. "verse 2", "and verse 2", or bare "2" after incomplete chapter).
  True "verse 1" / "verse one" phrases do NOT become 1:2.
  The exact Soniox text for seq=132 is not in the log — cannot pin STT string.

WHY CHAIN:
  Early partial "Matthew chapter 1" → chapter-only 1:1 @ 0.92 + incomplete
  → reading starts at 1:1
  → later segment resolves as verse 2 @ 1.00
  → should_restart_reading (2 > 1) → reading Started: Matthew 1:2
```

## B.9 · Investigation writeup (no fix shipped)

```
1. SYMPTOM: Live focus jumps Matthew 1:1 → 1:2 after speaking 1:1.
2. MECHANISM: intentional chapter-only emit + higher-verse re-anchor.
3. NOT A BUG: parser off-by-one on "verse 1".
4. OPEN: exact STT text for the 1:2 hit (needs transcript logging).
5. RISK SURFACE: bare number completion after incomplete chapter @ 1.00.
6. FIX OPTIONS (for user choice — not implemented):
   a) Log transcript snippet on every DET-DIRECT Found line
   b) Require "verse" keyword for incomplete completion (no bare digits)
   c) Delay chapter-only UI/live handoff until incomplete timeout
   d) Do not re-anchor reading on bare-number completions
```

## Fix shipped (2026-08-04)

```
A) DET-DIRECT Found logs chapter_only, auto_q, snip; full STT text only when
   SABBATHCUE_DEBUG_TRANSCRIPTS is set in debug builds.

B) Bare-digit verse completion after explicit chapter hold disabled.
   allow_bare_verse only when prior fragment said "verse" without a number.

AUTO-LIVE: RecentDirectEmissions key includes is_chapter_only so chapter-only
Matthew 1:1 @ 92% no longer suppresses full Matthew 1:1 @ 100% within 3s.
Frontend selectPreviewHit / auto-live already require !is_chapter_only — the
upgrade emission was the missing piece.

DIGIT-GROWTH HOLD (live log 20:10 / 20:24 Matthew 6:3 before 6:33):
Root cause of "still broken": frontend hold alone was insufficient because
backend still set auto_q=true on 6:3 and re-anchored reading to 6:3 before 6:33.
Also equal-confidence batches could pick 6:3 over 6:33 in selectPreviewHit.

Layers now:
1) Frontend: 900ms hold on single-digit auto-live; digit-prefix extension wins
2) Frontend: dropDigitPrefixLosers in batch + detection-store panel
3) Backend merger: single-digit full citations never auto_queue
4) Backend reading: never start/re-anchor on single-digit full citations;
   prefer non-growable when choosing candidates; drop prefix losers in batch

REGRESSION TESTS:
- full_verse_one_is_not_suppressed_by_prior_chapter_only_placeholder (ok)
- bare_digit_after_chapter_only_does_not_steal_to_another_verse (ok)
- test_continuation_bare_number_with_chapter_requires_verse_keyword (ok)
- holds single-digit auto-live until STT digit growth can finish (ok)
- presentation-workflow + verse-detection-workflow: 68 ok
- direct:: 157 ok; commands::stt::detection::tests 55 ok; cargo check ok
```

## Sign-off

```
Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression test (red → green) attached:    YES
Original reproduction now passes:          YES (unit harness)
Definition of fixed verified:              YES (tests + check)
```
