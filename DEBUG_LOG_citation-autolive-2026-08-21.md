# Evidence log — citation completeness, reading hijack, semantic false cards, Auto Live

## Bug definition (step 1)

```
BUG / TICKET:       citation-autolive-2026-08-21
SYMPTOM (exact):    2026-08-21 19:36–19:37 SabbathCue Personal.log:
                    - [READING] Started: Genesis 3:15 then Chapter change command detected: chapter 1 verse 15 then Started: Genesis 1:15
                    - [DET-DIRECT] Found: Genesis 3:1 (92%) chapter_only=true snip="Genesis three"
                    - [DET-DIRECT] Found: John 1:1 from snip="John chapter" before verse finished
                    - [DET-SEMANTIC] Found: Genesis 37:17 / I Samuel 11:14 / I Samuel 9:9 (88% semantic) for transcript_chars=20 ("Let's go to Genesis…")
                    - [DET-DIRECT] Found: Genesis 4:8 from snip="Genesis four,"
                    - Auto Live on: wrong verses could commit live via handleReadingAdvance(autoLive: true)
EXPECTED:           Complete book+chapter+verse only.
                    Genesis 3:15 / John 1:1 / Genesis 4:8 may preview, read, and (Auto Live ON) go live.
                    Incomplete citations emit nothing.
                    Leading "One, two" must not retarget reading to Genesis 1:15.
                    Leaving a chapter requires the book name.
                    Semantic emits only high-overlap verse quotations, not command/topical guesses.
                    Auto Live OFF: preview + reading, never live.
ACTUAL:             Incomplete and noise-prefixed utterances emit/live wrong verses; 88% semantic cards emit after Cerebras abstain.
DELTA:              Wrong verse identity on live/preview/detections; semantic cards that must not exist.
REPRO STATUS:       ALWAYS in the captured log; tests will replay the utterances.
ENVIRONMENT:        Windows, SabbathCue v0.1.9 session, Speechmatics after Soniox, autoMode=true, Auto Live ON, semantic 0.80, Cerebras ranking.
FIRST OBSERVED:     2026-08-21 19:36 user session
LAST KNOWN GOOD:    unknown for this contract (new operator rules)
IN SCOPE:           detection presentation policy, reading-mode chapter change, semantic emit gate, verse-detection-workflow Auto Live for Bible
OUT OF SCOPE:       hymn-voice-control.ts and hymn path
DEFINITION OF FIXED: failing tests for this transcript contract go red then green; hymn tests unchanged; one quote keep-alive still emits.
```

## B.3 · Hypothesis log (step 4)

```
H1: extract_chapter_and_verse number-first path parses leading spoken "One" + later "verse 15" as chapter 1 verse 15 while reading Genesis 3.
    Predicts: reading_mode check on "One, two and turn to Genesis three, verse 15" after start on 3:15 yields Genesis 1:15.
H2: Chapter-only / incomplete direct detections are emitted and can start reading or preview.
    Predicts: detector.detect("Genesis three") emits Genesis 3:1 chapter_only; "John chapter" emits John 1:1; "Genesis four," emits Genesis 4:8.
H3: Semantic hybrid emits 88% fuzzy topical hits for citation-command speech without verse-text overlap.
    Predicts: "Let's go to Genesis for this eight" emits Genesis 37:17 etc.
H4: handleReadingAdvance always passes autoLive: true, so Auto Live ON commits hijacked verses live; Auto Live OFF still starts reading (allowed) but must not commit live.
    Predicts: workflow with readingModeAutoLive false still previews/reads but does not commitLiveItem with makeLive.
    Result: CONFIRMED for Auto Live ON hijack (log READING Started Genesis 1:15). Auto Live OFF already covered by existing frontend tests; no code change needed there.
```

## B.5 · Root cause

```
ROOT CAUSE: extract_chapter_and_verse number-first parsing treated leading spoken "One" plus later "verse 15" as Genesis 1:15 after reading started on Genesis 3:15; chapter-only detections were emitted as cards; semantic quotations without lexical overlap were Suggestion and still emitted.
WHY CHAIN: STT final "One, two and turn to Genesis three, verse 15" → direct hit 3:15 starts reading → check_chapter_command on the same utterance → chapter 1 verse 15 → handleReadingAdvance autoLive true → live output.
CAUSE→SYMPTOM EVIDENCE: log line "[READING] Chapter change command detected: chapter 1 verse 15" immediately after "[READING] Started: Genesis 3:15". RED test testing_one_two_does_not_hijack_genesis_3_15_to_1_15 observed Some(ChapterChange { new_chapter: 1, start_verse: Some(15) }).
```

## B.8 · Regression test

```
RED: incomplete_citations_from_2026_08_21_session_emit_nothing panicked with "Genesis three" emitting ["Genesis 3:1"].
RED: testing_one_two_does_not_hijack_genesis_3_15_to_1_15 panicked with ChapterChange chapter 1 verse 15.
RED: leaving_chapter_requires_book_name panicked because "chapter 4 verse 8" still navigated.
RED: chapter_only_is_rejected / high_embedding_without_lexical_quote_is_rejected were Suggestion not Reject.
GREEN: cargo test -p rhema-detection --lib → 382 passed.
GREEN: cargo test -p rhema-detection --test presentation_replay → 3 passed.
GREEN: npm run test:unit -- verse-detection-workflow + presentation-decision → 71 passed.
GREEN: hymn-voice-control.test.ts → 14 passed.
```

## B.9 · Root-cause writeup

```
ROOT-CAUSE WRITEUP — citation-autolive-2026-08-21 — 2026-08-21
1. SYMPTOM: Session log sent Genesis 1:15 live after Genesis 3:15, emitted Genesis 3:1 / John 1:1 from incomplete speech, and showed 88% Genesis 37:17 / 1 Samuel cards for "Let's go to Genesis for this eight".
2. ROOT CAUSE: Reading-mode chapter navigation parsed the first number in the utterance; incomplete citations were emitted as cards; non-quote semantic hits were Suggestion and still forwarded to the UI.
3. MECHANISM: Number-first extract_chapter_and_verse + reading start on the same final + Auto Live ON committed the hijack. Detector pushed chapter-only verse-1 cards. decide_quotation without lexical quote returned Suggestion, and verse_detections still emitted those results.
4. HOW IT WAS FOUND: SabbathCue Personal.log 19:36–19:37 plus RED tests on the exact utterances.
5. THE FIX: Do not emit incomplete citations; require the current book name to leave a chapter; Reject quotations without lexical overlap and drop Reject bible results before IPC; frontend also drops reject / suggestion quotations without lexical quote. Hymn path untouched. Auto Live OFF already preview+read/not live.
6. VERIFICATION: 382 rhema-detection lib tests, presentation_replay, 71 frontend workflow tests, 14 hymn tests, clippy -D warnings on rhema-detection and sabbathcue lib.
7. PREVENTION: Transcript contract tests in detector.rs, reading_mode.rs, presentation.rs, presentation-policy fixture, verse-detection-workflow.test.ts.
```

## Sign-off

```
Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression test (red → green) attached:    YES
Original reproduction now passes:          YES (deterministic utterance tests)
Definition of fixed verified:              YES
```
