# Evidence log — semantic quotations missing from preview/live (2026-08-23)

## Bug definition (step 1)

```
BUG / TICKET:       semantic-quotes-2026-08-23
SYMPTOM (exact):    2026-08-23 14:01–14:04 SabbathCue Personal.log
                    Spoken:
                      "Testing, testing. One, two, testing."
                      "Okay, let's turn to John chapter 1, verse 8."
                      "And let's turn to Genesis 1, verse 3."
                      "For God so loved the world that He gave His only begotten Son,
                       that who shall ever believe in Him should not perish, but have everlasting life."
                      "John 3, verse 16."
                      "The Lord is my shepherd; I shall not want; He maketh me lie down green pastures."
                      "Now to him who is able to do immeasurably more and know that we ask or imagine
                       according to his power, that is at work within us."
                      "Ephesians chapter 2, verse 4"
                    Log:
                      [DET-DIRECT] Found: John 1:8 (100%)
                      [READING] Started: John 1:8
                      [DET-DIRECT] Found: Genesis 1:3 (100%)
                      [READING] Started: Genesis 1:3
                      [DET-SEMANTIC] Suppressed 5 out-of-scope Bible result(s) while reading Genesis 1
                      [DET-SEMANTIC] Releasing reading scope Genesis 1 (2 consecutive strong hits on 43:3)
                      [DET-SEMANTIC] Found: The Desire of Ages p.419 par.2 (88–89% semantic)
                      [DET-DIRECT] Found: John 3:16 (100%)
                      [READING] Started: John 3:16
                      [DET-SEMANTIC] Suppressed 5 out-of-scope Bible result(s) while reading John 3
                      [DET-SEMANTIC] Releasing reading scope John 3 (... hits on 19:23)
                      then many [DET-TRACE] decision=semantic_none emitted=0 candidates=2..5
                      words=12 on the 146-char John 3:16 final and the 129-char Eph 3:20 final
                      [DET-DIRECT] Found: Ephesians 2:4 (100%) snip="Ephesians chapter"
EXPECTED:           Direct citations John 1:8, Genesis 1:3, John 3:16, Ephesians 2:4
                    on preview/live (citations). Spoken verse quotations John 3:16,
                    Psalm 23:1, Ephesians 3:20 on preview/live (semantic).
ACTUAL:             Direct citations emitted. Semantic Bible quotations never emitted
                    (semantic_none). John 3:16 quote surfaced Desire of Ages p.419 instead.
DELTA:              Hybrid found 1–5 Bible candidates; authorize/finalize emptied them
                    before verse_detections IPC. EGW used a 40-word window and leaked through.
REPRO STATUS:       ALWAYS in captured 14:01–14:04 session
ENVIRONMENT:        Windows, Soniox, autoMode=true, semanticDetectionEnabled=true,
                    semanticConfidenceThreshold=0.7, confidenceThreshold=0.9,
                    aiRankingEnabled=true provider=cerebras, KJV translation_id=1
FIRST OBSERVED:     2026-08-23 14:01 session after 2026-08-21 citation-autolive lexical Reject
LAST KNOWN GOOD:    semantic quotations presented before 2026-08-21 presentation Reject gate
RECENT CHANGES:     2026-08-21 decide_quotation Reject without lexical fire;
                    LIVE_DETECTION_WINDOW_WORDS=12; reading-scope swallows non-request quotes
IN SCOPE:           live Bible window, quote lexical flag, quotation presentation,
                    reading-scope vs genuine out-of-chapter quotations
OUT OF SCOPE:       hymn path; STT provider
DEFINITION OF FIXED: RED tests for this transcript fail on unfixed code then pass;
                     John 3:16 / Psalm 23 / Eph 3:20 quotations authorize preview;
                     Desire of Ages does not replace John 3:16; suite green
```

## B.1 · Reproduction (step 2)

```
STEPS / SCRIPT:
1. Open SabbathCue Personal.log 2026-08-23 14:01:23–14:04:56
2. Replay the spoken utterances through clamp_to_recent_words(12) +
   quote_overlap_confidence + decide_presentation

FAILURE OUTPUT (verbatim):
See B.2 log lines. Unit tests added in the same session capture the
window/overlap/authorization failures against unfixed code.

RELIABILITY: deterministic 1/1 captured session; unit tests N/N once added
```

## B.2 · Evidence captured (step 3)

```
FULL STACK TRACE: n/a (wrong output, not a crash)

OBSERVED STATE AT FAILURE:
- LIVE_DETECTION_WINDOW_WORDS=12, LIVE_EGW_QUOTE_WINDOW_WORDS=40
- Workflow logs words=12 even when PIPELINE final_transcript chars=146 and chars=129
- seq=48 candidates=5 suppressed while reading Genesis 1
- seq=49 release on 43:3 (John 3) then still semantic_none
- seq=57+ emitted only Desire of Ages p.419 par.2 88–89%
- seq=84 final of John 3:16 quote: candidates=5, emitted EGW only
- seq=100 Psalm 23 while reading John 3: suppressed 5, then release on 19:23,
  subsequent seq=101–114 semantic_none candidates=2–5
- seq=36 (14:04:28) Eph 3:20 final chars=129 words=12 candidates=2 semantic_none
- decide_quotation: Reject unless has_lexical_quote AND quote_coverage>=0.56
  AND candidate_margin>=0.02 AND (is_final OR independent_final_count>=2)
- has_quote_evidence requires overlap_confidence >= 0.90
- retain_rejected_bible_results drops Bible Reject; EGW is appended after authorize
  and is exempt

DIFFERENTIAL (failing vs working case):
Working: John 1:8 / Genesis 1:3 / John 3:16 / Ephesians 2:4 via DET-DIRECT
Failing: same session's verse quotations via DET-SEMANTIC
```

## B.3 · Hypothesis log (step 4)

```
H1: LIVE_DETECTION_WINDOW_WORDS=12 drops the distinctive opening of long
    verse quotations so they never reach fire-tier overlap (0.90).
    Predicts: last 12 words of spoken John 3:16 overlap < 0.90; full quote >= 0.90.
    Result: CONFIRMED. Full quote fire-overlap OK. 12-word tail
    "shall ever believe in Him should not perish, but have everlasting life."
    overlap Some(0.7815). Psalm 23 12-word window drops "The Lord" and hybrid
    retrieves nothing.

H2: has_quote_evidence requires overlap >= 0.90, so 0.78/0.86 paraphrases
    get has_lexical_quote=false and decide_quotation Rejects them.
    Predicts: process_hybrid + decide_presentation on Eph 3:20 NIV-like
    wording vs KJV is Reject.
    Result: CONFIRMED. session_ephesians_3_20_paraphrase_is_live_authorized
    left=Reject before the flag change.

H3: candidate_margin 0.0 (tied semantic scores) is part of lexical_ok, so
    two-candidate finals become Reject / semantic_none.
    Predicts: decide_quotation with has_lexical_quote true, coverage 0.86,
    margin 0.0 is Reject.
    Result: CONFIRMED. tied_quotation_candidates_still_preview got Reject.

H4: Reading-scope filter swallows out-of-chapter quotations until a 2-hit
    release, by which the 12-word window has already lost the opening.
    Predicts: log shows Suppressed 5 while reading Genesis 1, then release
    on 43:3, then still semantic_none.
    Result: CONFIRMED as an aggravating timeline. Escape hatch already
    exists (2-hit release + stale pause). Widening the window is enough
    after release; not changing the echo-suppression contract.
```

## B.4 · Isolation / bisection log (step 5)

```
EXPERIMENT 1 — quote_overlap_confidence(full spoken John 3:16, KJV)
  OBSERVED: reaches QUOTE_OVERLAP_FIRE_CONFIDENCE (test passed on unfixed code)
  VERDICT: confirms H1 differential (full text is fine)

EXPERIMENT 2 — same overlap on trailing 12 words
  OBSERVED: Some(0.7815384615384615)
  VERDICT: confirms H1

EXPERIMENT 3 — quotation_grant_for 12-word John 3:16 / Psalm 23 / Eph 3:20
  OBSERVED: John 3:16 Reject; Psalm 23 hybrid empty; Eph 3:20 Reject
  VERDICT: confirms H1+H2

EXPERIMENT 4 — decide_quotation margin 0.0 with lexical quote
  OBSERVED: Reject
  VERDICT: confirms H3

NARROWED TO:
  LIVE_DETECTION_WINDOW_WORDS=12 in detection.rs
  has_quote_evidence overlap>=0.90 in pipeline.rs
  candidate_margin inside lexical_ok in presentation.rs decide_quotation
```

## B.5 · Root cause (step 6)

```
ROOT CAUSE: Three gates together deleted spoken verse quotations after hybrid
had already found them.
1. The live Bible search window kept only 12 trailing words, so John 3:16 and
   Psalm 23 lost their identity and could not reach fire-tier overlap.
2. has_lexical_quote was set only when overlap >= 0.90, so the 0.78 John 3:16
   tail and 0.86 Ephesians 3:20 paraphrase were stamped Reject.
3. A 0.00 confidence margin between two semantic candidates used the same
   Reject path, so the Eph 3:20 final (candidates=2) emitted nothing.
WHY CHAIN: spoken quote → clamp 12 words → overlap 0.78 / None →
has_lexical_quote false or margin 0 → decide_quotation Reject →
retain_rejected_bible_results drops Bible → EGW 40-word window emits
Desire of Ages p.419 instead of John 3:16.
CAUSE→SYMPTOM EVIDENCE: log words=12 on 146-char and 129-char finals;
RED overlap Some(0.7815); RED grant Reject; RED Psalm 23 window without "Lord".
```

## B.8 · Regression test (step 7 → 8)

```
TEST CODE: pipeline session_* quotation grants; presentation tied_quotation;
           sabbathcue session_john_316 / session_psalm_23 window tests

RED (before fix):
  session_john_316_twelve_word_live_window_still_reaches_fire_overlap
    Some(0.7815384615384615) not fire
  session_john_316_twelve_word_window_is_live_authorized
    left: Reject  right: LiveAuthorized
  session_psalm_23_twelve_word_window_is_live_authorized
    hybrid must retrieve ... "shepherd; I shall not want; He maketh me lie down green pastures."
  session_ephesians_3_20_paraphrase_is_live_authorized
    left: Reject  right: LiveAuthorized
  tied_quotation_candidates_still_preview
    got Reject
  session_john_316_quotation_fits_in_live_bible_window
    got "shall ever believe in Him should not perish, but have everlasting life."

GREEN (after fix):
  cargo test -p rhema-detection --lib → 406 passed
  cargo test -p rhema-detection --test presentation_replay → 3 passed
  cargo test -p sabbathcue --lib detection → 144 passed
  cargo clippy -p rhema-detection -p sabbathcue --lib --tests -- -D warnings → exit 0
```

## B.6 · Fix diff (step 8)

```
LIVE_DETECTION_WINDOW_WORDS 12 → 40 (verse-length, same as EGW)
has_quote_evidence: overlap_confidence.is_some() instead of >= 0.90
decide_quotation: margin gates Live vs Preview, not Reject
```

## B.7 · Verification output (step 8)

```
ORIGINAL REPRO RE-RUN: session quotation unit tests now LiveAuthorized / window keeps opening
FULL SUITE: rhema-detection 406 passed; sabbathcue detection 144 passed; presentation_replay 3 passed
TYPE-CHECK / LINT: clippy -D warnings exit 0; rustfmt --check exit 0
SIBLING GREP: LIVE_DETECTION_WINDOW_WORDS=12 only remains in this evidence log
INTERMITTENT: n/a (deterministic)
```

## B.9 · Root-cause writeup (step 9)

```
ROOT-CAUSE WRITEUP — semantic-quotes-2026-08-23 — 2026-08-23
1. SYMPTOM: Spoken John 3:16, Psalm 23, and Ephesians 3:20 never reached
   preview/live. Direct citations in the same session worked. Desire of Ages
   p.419 replaced John 3:16.
2. ROOT CAUSE: The 12-word Bible window plus a fire-tier lexical flag plus a
   zero-margin Reject gate dropped verified quotations after hybrid retrieval.
3. MECHANISM: Finals of 27–28 word quotes were searched as 12 words. Overlap
   0.78/0.86 did not set has_lexical_quote. decide_quotation Rejected them.
   EGW used 40 words and leaked through because authorize runs before EGW append.
4. HOW IT WAS FOUND: 14:01–14:04 log (words=12, semantic_none, DA p.419) plus
   RED overlap 0.7815 on the exact tail.
5. THE FIX: 40-word Bible window; lexical quote = any verified overlap;
   tied margin previews instead of vanishing.
6. VERIFICATION: RED then GREEN session tests; 406+144+3 tests; clippy clean.
7. PREVENTION: Session-utterance tests lock the window, overlap flag, and
   margin policy. Reading-scope echo suppression is unchanged.
```

## Sign-off

```
Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression test (red → green) attached:    YES
Original reproduction now passes:          YES (unit replay of the session utterances)
Definition of fixed verified:              YES
```
