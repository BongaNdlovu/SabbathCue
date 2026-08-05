# EGW detection latency — evidence log

## Bug definition (step 1)

```
BUG / TICKET:       egw-detection-latency
SYMPTOM (exact):    User reads EGW aloud with cue; "detections take long"
EXPECTED:           Preview/live within ~1–2s of enough quote text under cue
ACTUAL:             First PP Found at ~19s after speech start; auto_q ~33s
DELTA:              Multi-second gap between quotes=1 (matched) and Found (emitted)
REPRO STATUS:       ALWAYS on live session 2026-08-04 21:21 (this run)
ENVIRONMENT:        Windows, tauri dev, Soniox live STT, cue live, bible hybrid on
FIRST OBSERVED:     Prior EGW sessions same session/branch
LAST KNOWN GOOD:    N/A — latency class not previously fixed
RECENT CHANGES:     EGW cue TTL, drop_egw when cued, EGW single-pass FE, candidates=5
IN SCOPE:           live_session run_semantic_detection, tasks worker, egw scoring
OUT OF SCOPE:       STT provider latency itself
DEFINITION OF FIXED: quotes=1 emits Found same job when cue_live; wall clock to first
                     EGW emit < ~2s after run-length threshold met; no silent drop
```

## B.1 · Reproduction (step 2)

```
STEPS:
1. Live STT on, cue "statement by Ellen White"
2. Read PP-style quote (Adam and Eve / law of God…)
3. Watch logs for DET-EGW-QUOTE / Found

FAILURE OUTPUT (timeline 21:21, log call-572e8d73…-8.log):
21:21:00 speech/semantic start
21:21:06 first cue_live=true quotes=0
21:21:12 first quotes=1  — NO Found, NO semantic_none, NO Total
21:21:12–18 quotes=1 × many jobs — still NO Found
21:21:19 first Found: Jeremiah 17:1 88%, Psalms 119:69 88%, PP p.322 92% auto_q=false
         DET-TRACE top=Jeremiah 17:1 (Bible-first emit order)
21:21:33 first auto_q=true PP p.322
21:21:52 auto_q=true PP p.323 (second statement)

RELIABILITY: deterministic on this recording (1/1)
```

## B.2 · Evidence captured (step 3)

```
Hybrid cost while quotes matching (21:21:12–19 total_ms):
1125, 1569, 1133, 1167, 378, 374, 377, 415, 412 ms

QUEUE at 21:21:52: partial_semantic latest-wins sent=100 replaced=41

Code path (live_session.rs run_semantic_detection):
1. hybrid Bible ONNX process_hybrid_with_fts  (~400–1500ms)  FIRST
2. detect_live_egw_quotes AFTER hybrid
3. log DET-EGW-QUOTE quotes=N
4. results.extend(egw_quotes)
5. if seq < latest_seq → return at DEBUG only (no Found log)
6. else log Found + emit

quotes=1 without Found/Total/TRACE ⇒ path exits at step 5 (stale suppress).

PP Found: 17 total; auto_q=true only 4; auto_q=false 13
```

## B.3 · Hypothesis log (step 4)

```
H1: Latest-wins stale drop after slow hybrid discards ready EGW quotes
    Predicts: quotes=1 then silence (no Found) for multiple seqs
    Result: CONFIRMED — 21:21:12–18

H2: EGW only runs after Bible hybrid serializes the single worker
    Predicts: every DET-EGW-QUOTE follows DET-SEMANTIC Workflow elapsed 400ms+
    Result: CONFIRMED

H3: auto_q needs shared run ≥ 8 + merger; first fires are fire-band only
    Predicts: first PP Found auto_q=false even at 92%; auto_q later
    Result: CONFIRMED — 21:21:19 false, 21:21:33 true

H4: drop_egw_quotes_echoing_scripture drops cued quotes
    Predicts: Dropped … scripture echo logs when cue live
    Result: ELIMINATED — cue_active short-circuits drop; no Dropped logs

H5: BM25 finds nothing until late
    Predicts: quotes=0 entire time
    Result: ELIMINATED for primary delay — quotes=1 from 21:21:12, emit at 21:21:19
    (secondary: quotes=0 for first ~6s after cue while run < fire/hint)

H6: Frontend single-pass blocks EGW at 0.92
    Predicts: FE still requires 0.95
    Result: ELIMINATED in code — EGW_SINGLE_PASS 0.88; issue is backend emit delay
```

## B.5 · Root cause (step 6)

```
ROOT CAUSE: Ready EGW quote hits are computed only after a full Bible hybrid
pass (~0.4–1.5s) on a single latest-wins worker; by completion the job seq is
usually stale, so emission is skipped (debug-only). User waits until a job
finishes while still latest — multi-second lag after the match already exists.

WHY CHAIN:
  feel "slow" → no preview/live → no verse_detections emit → Found never logged
  → stale seq after hybrid → hybrid runs before EGW on same blocking job
  → single worker + partial flood (41% replaced)

CAUSE→SYMPTOM EVIDENCE:
  21:21:12 quotes=1 with no Found/Total; hybrid total_ms≈1125ms same job;
  first Found only 21:21:19; QUEUE replaced=41.
```

## Fix applied (2026-08-04) — latency

```
CHANGE: live_session.rs run_semantic_detection
- Run detect_live_egw_quotes BEFORE Bible hybrid
- When cue_live: emit EGW (or return none) and SKIP hybrid entirely
- Log path=pre_hybrid / decision=egw_cue_fast|egw_cue_fast_none
```

## Retest 21:30–21:32 — accuracy (not latency)

```
EGW pages CORRECT for spoken quotes (DB verified):
  Q1 Adam/Eve law of God     → PP p.322 par.1  (logged)
  Q2 law from Sinai/Nehemiah → PP p.324 par.2  (logged)
  Q3 Christ through prophets → PP p.325 par.3  (logged)

WRONG detections logged:
  21:30:32 pre-cue hybrid: Judges 16:29, Mark 15:27, Matthew 20:21 @ 88%
  21:32:09 co-fire: Desire of Ages p.327 @ 75% with PP 325
  21:32:15–17 after cue TTL 90s: I Peter 1:1, II Peter 1:1, Luke 9:20
  auto_q: 8 true / 80 false on PP Found (merger cooldown 2500ms)

ROOT CAUSES:
  R1: Cue TTL from first attribution only; multi-quote block >90s → hybrid +
      "apostle Peter" → I Peter 1:1 (confirmed timeline 21:30:37 + 90s ≈ 21:32:07)
  R2: Multiple EGW quotes emitted; weaker wrong-book hits pollute list
  R3: Semantic EGW auto_q stripped by merger cooldown every 2.5s
  R4: Pre-cue Bible hybrid on mic-test speech (still open — lower priority)
```

## Fix applied (2026-08-04) — accuracy round 2

```
1. Refresh egw_cue_at_ms whenever a quote matches under a live cue
   (extends EGW mode for multi-quote readings)
2. retain_best_egw_quote — emit only highest-confidence EGW per window
3. apply_egw_auto_queue: semantic quotes use threshold only, NOT cooldown

TEST: retain_best_egw_quote_tests::keeps_only_the_highest_confidence_quote PASS
PENDING: auto_queue_policy tests + user retest
```
