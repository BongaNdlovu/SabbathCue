# Evidence log — reading-scope swallows requests; Acts 15:40 ties prison scene at 86%

## Bug definition (step 1)

```
BUG / TICKET:       live-request-scope + acts-15-40-ranking
SYMPTOM (exact):    2026-08-21 20:54 SabbathCue Personal.log
                    [DET-SEMANTIC] Suppressed 5 out-of-scope Bible result(s) while reading Genesis 3
                    [DET-TRACE] seq=37 decision=semantic_none emitted=0 fts_hits=10 candidates=5
                    later:
                    [DET-SEMANTIC] Found: Acts 15:40 (86% semantic)
                    [DET-SEMANTIC] Found: Acts 16:25 (86% semantic)
                    [DET-SEMANTIC] Found: Acts 16:29 (76% semantic)
                    top=Acts 15:40 (86%)
EXPECTED:           A verse request ("go to the verse that talks about Paul and Silas singing in a prison")
                    must emit while reading Genesis 3. Acts 16:25 (midnight / sang / prisoners) must outrank
                    Acts 15:40 (Paul chose Silas). Acts 15:40 must not sit in the 86% band.
ACTUAL:             Request hits are dropped by the Genesis 3 reading-scope filter. After scope release,
                    both 15:40 and 16:25 emit at 86% and 15:40 sorts first.
DELTA:              suppression of request hits; name-only companion verse tied with the prison scene.
REPRO STATUS:       ALWAYS (session log 20:53–20:54)
ENVIRONMENT:        Windows, bun tauri dev, Soniox, auto_mode=true, semantic_threshold=0.70, KJV id=1
FIRST OBSERVED:     2026-08-21 20:54 after Genesis 3:1 auto-live
IN SCOPE:           live_session reading-scope filter; pipeline distinctive coverage / overlap boost
OUT OF SCOPE:       hymn path; Auto Live citation policy
DEFINITION OF FIXED: RED tests fail on unfixed code; request hits survive reading scope; 16:25 outranks
                     15:40; 15:40 confidence < 0.80 on the prison-singing request.
```

## B.2 · Evidence captured (step 3)

```
SabbathCue Personal.log 75045: Suppressed 5 out-of-scope while reading Genesis 3 (seq=37, candidates=5, emitted=0)
75135: Releasing reading scope Genesis 3 (... 2 repeated out-of-scope hits on 44:16) then emitted=0
75221-75224: Acts 15:40 86%, Acts 16:25 86%, Acts 16:29 76%, top=Acts 15:40
75241-75242: CEREBRAS selected=44:16:25 among 44:15:40,44:16:25,44:16:29
```

## B.5 · Root cause (step 6)

```
ROOT CAUSE 1: filter_semantic_results_to_reading_scope drops every out-of-chapter Bible hit
              while reading is active. Verse requests are not citations, so Acts 16:25 is
              swallowed until the two-hit release streak pauses Genesis 3.
ROOT CAUSE 2: distinctive coverage compared exact tokens, so singing≠sang and prison≠prisoners.
              Both 15:40 and 16:25 only matched Paul+Silas. FTS+vector overlap then added +0.10
              to both (76% → 86%). Equal scores sort 15:40 first.
```

## B.8 · Regression test (step 7 → 8)

```
RED:  prison_singing_request_covers... coverage=0.333
      paul_and_silas_singing... ranked Acts 15:40 first
      reading_scope_filter_keeps_out_of_chapter... would drop Acts 16:25
GREEN: rhema-detection --lib 385 passed; sabbathcue reading_scope 9 passed; finalize 4 passed
```

## B.9 · Root-cause writeup (step 9)

```
ROOT-CAUSE WRITEUP — live-request-scope + acts-15-40-ranking — 2026-08-21
1. SYMPTOM: Request hits vanished while Genesis 3 was live; later Acts 15:40 tied 16:25 at 86%.
2. ROOT CAUSE: Reading-scope filter treated requests as quotation echoes. Coverage did not stem
   inflections, so the companion-choice verse inherited the same name-only score and overlap boost.
3. MECHANISM: Genesis 3:1 started reading → semantic Acts hits suppressed → after release both
   verses got phrase/vector corroboration on "Paul and Silas" → +10% boost → 15:40 sorted first.
4. HOW IT WAS FOUND: Session log seq=37/114 plus a coverage unit test showing 0.33 vs 1.0 after stemming.
5. THE FIX: Skip the chapter filter for looks_like_verse_request. Stem event terms (sang→sing,
   prison≈prisoner) and withhold overlap boost unless coverage ≥ 0.75.
6. VERIFICATION: RED then GREEN tests; clippy -D warnings clean on rhema-detection and sabbathcue.
7. PREVENTION: Tests lock ranking, coverage, overlap-boost, and request-vs-quotation scope.
```

## Sign-off

```
Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression test (red → green) attached:    YES
Original reproduction now passes:          YES (unit tests of the log cases)
Definition of fixed verified:              YES
```
