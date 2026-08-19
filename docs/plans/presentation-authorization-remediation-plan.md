# Presentation Authorization Remediation Plan

**Status:** proposed completion plan for the current uncommitted implementation  
**Date:** 2026-08-18  
**Scope:** live Bible/EGW detection, AI ranking, preview/live output, queueing, and reading-mode handoff.

## 1. Decision and definition of done

The system must enforce one invariant:

> A detection candidate is evidence only. It cannot preview, go live, queue, or start reading mode unless the single presentation-authority policy explicitly grants that action.

This plan is complete only when all of the following are demonstrated by automated, recorded evidence:

1. The current presentation-authorization work is integrated correctly and all pre-existing behavior that remains in scope has a deliberate, passing contract.
2. The supplied failure classes cannot change visible presentation state:
   - ordinary speech such as `makes one`;
   - chapter-only references;
   - fuzzy-book matches;
   - high-scoring semantic matches without verified lexical evidence;
   - overlapping partial/final echoes of one utterance;
   - indirect requests whose intended verse was not retrieved or grounded.
3. The resulting system is measurably safer and at least as effective: zero forbidden presentation actions in the replay corpus, request retrieval recall at the declared target, preserved explicit-reference recall, and no material latency regression.

“Never happen again” has a precise engineering meaning here: every future change is blocked by an invariant-level, end-to-end replay gate for these policy classes. It does not mean no unknown input can ever be misrecognized.

## 2. Audit of the work already started

The current worktree contains 22 modified files and two new files. It has established several correct foundations:

| Completed foundation | Evidence |
| --- | --- |
| A central policy model exists. | `src-tauri/crates/detection/src/presentation.rs` defines `PresentationDecision`, `DetectionJob`, `PresentationEvidence`, and `PresentationGrant`. |
| Chapter-only and fuzzy-book candidates are denied action authority by the policy. | `presentation.rs` policy tests pass; `direct/detector.rs` adds `is_fuzzy_book`; `stt/detection.rs` now tests that chapter-only candidates do not enter the reading handoff. |
| Fuzzy collision prevention was extended. | `direct/fuzzy.rs` adds `makes` and tests `that makes one wonder`. |
| Request recall was improved for Joseph/pit, mark/beast, and walking-on-water phrasing. | `crates/bible/src/search.rs` adds topic queries; `crates/bible/tests/retrieval_recall.rs` passes all 16 tests. |
| Semantic jobs now retain final/partial metadata. | `commands/stt/detection_jobs.rs` adds `is_final` and `utterance_id`; `tasks.rs` passes them to the worker. |
| The frontend has authorization helpers. | `src/lib/presentation-decision.ts` and its four passing unit tests. |

### Current verification state

| Command | Result on 2026-08-18 | Meaning |
| --- | --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection presentation` | 9 passed | The pure policy’s initial tests pass. |
| `cargo test --manifest-path src-tauri/Cargo.toml -p rhema-bible --test retrieval_recall` | 16 passed | New retrieval phrases are recalled in the fixture database. |
| `cargo test --manifest-path src-tauri/Cargo.toml -p sabbathcue direct_reading_candidates_exclude_chapter_only_handoffs` | 1 passed | The targeted chapter-only handoff test passes. |
| `npm.cmd run test:unit -- src/lib/presentation-decision.test.ts src/lib/verse-detection-workflow.test.ts` | 16 failures / 67 tests | The new contract is not fully integrated. This is a release blocker. |
| `cargo test ... -p sabbathcue ...` | compiles with unused `DetectionJob` import warning | Clippy will fail until the warning is removed. |

### Defects in the current implementation that must be corrected

1. `is_direct_reading_handoff` calls `citation_grant(detection, true)` in `commands/stt/detection_logic.rs`. A partial direct result can therefore pass the reading-mode handoff as if it were final.
2. `run_direct_detection` creates `reading_candidates` from `MergedDetection` before results are authorized. Reading mode can still use a parallel path rather than the emitted grant.
3. `authorize_emitted_results` matches `results[index]` to `detections[index]` after filtering, sorting, truncation, and EGW insertion. That positional relationship is not stable; grants can be attached to the wrong result.
4. The semantic fallback branch uses result confidence as quote coverage. `quote_coverage` must be a measured lexical-coverage field, never a confidence substitute.
5. The live-session calls pass `automation_live_enabled: true` unconditionally. They bypass the user’s Auto Mode and live-output policy.
6. `EvidenceLedger` is a static global. Its state is not explicitly reset on transcription start/stop and is not naturally scoped to one live session.
7. `DetectionResult.authorization` and `.job` are strings at the backend boundary. They permit typos and make the backend/frontend contract weaker than the Rust enum policy.
8. The frontend test helper gives an implicit live authorization to every direct fixture and a suggestion authorization to every semantic fixture. The 16 failing workflow tests show that old test assumptions and the new contract were mixed instead of migrating each scenario deliberately.
9. The existing accuracy benchmark selects candidates but does not replay real presentation effects—preview, live, queue, and reading mode. It previously called the false `James 1:1` candidate “silent” while the live app started reading it.

None of these defects should be papered over by restoring a frontend confidence bypass or by weakening the new authorization policy.

## 3. Target architecture

```text
STT partial/final event
        |
        v
direct / quote / request retrieval candidate generation
        |
        v
CandidateEvidence { stable key, provenance, finality, measured lexical evidence }
        |
        v
PresentationAuthority (single Rust policy owner)
        |
        +--> Suggestion          -> display card only
        +--> PreviewAuthorized   -> preview only
        +--> ReadingAuthorized   -> preview + citation-only reading mode
        +--> LiveAuthorized      -> permitted live behavior, subject to user live toggle
        +--> Reject              -> no card/action, with diagnostic reason
        |
        v
one `PresentationResult` event
        |
        v
frontend renders the grant; it never derives permission from score/source/auto_queued
```

The policy must run after the final candidate set is known and before any `verse_detections`, reading-mode, queue, preview, or live-output event. No secondary path may re-derive authorization.

## 4. Phase 0 — freeze the baseline and establish the replay corpus

### Files

- New: `data/detection-fixtures/presentation-policy-2026-08-18.json`
- New: `src-tauri/crates/detection/tests/presentation_replay.rs`
- New: `docs/reports/presentation-authorization-baseline-2026-08-18.json`
- Update: `src-tauri/crates/detection/src/bin/detection_accuracy.rs`

### Implementation

Add a replay fixture that models raw provider events, not only final text. Each case must declare input events and the allowed side effects:

```json
{
  "id": "false-direct-makes-one",
  "events": [
    { "kind": "partial", "utteranceId": 101, "text": "that explains or makes one" },
    { "kind": "final", "utteranceId": 101, "text": "that explains or makes one" }
  ],
  "expect": {
    "authorization": ["suggestion", "reject"],
    "preview": null,
    "live": null,
    "queue": [],
    "reading": null
  }
}
```

Include the supplied windows as distinct cases:

- `Genesis one, verse eight` → Genesis 1:8 citation.
- `Joshua chapter one, verse nine` → Joshua 1:9 citation.
- `Paul and Silas singing in prison` → Acts 16:25 request.
- `Jesus walking on water` → Matthew 14:25 or John 6:19 request.
- `Joseph ... thrown into a well` → Genesis 37:24 request.
- `mark of the beast` → Revelation 13:16 or 13:17 request.
- numeric/testing strings → no presentation action.
- `makes one` → no presentation action.
- all four supplied theological/EGW-style passages → no Bible presentation action.
- a verified exact Bible quotation → authorized quote behavior.
- repeated partial plus one final → no duplicate confirmation.
- two different final utterances → only then satisfy two-final confirmation where that policy is used.

Record the current baseline before changing behavior: current action decisions, pass/fail count, direct-reference recall, request recall, precision, p50/p95 latency, and tool version/commit. Store it as JSON, not an editable prose claim.

### Red/green proof

The old branch must fail the replay assertion for the `makes-one` reading side effect. The finished implementation must pass all cases. The test must assert actual side effects, not only a detected reference.

## 5. Phase 1 — finish the typed backend contract

### Files

- `src-tauri/crates/detection/src/presentation.rs`
- `src-tauri/crates/detection/src/types.rs`
- `src-tauri/src/commands/detection/result.rs`
- `src-tauri/src/commands/stt/detection_logic.rs`
- `src-tauri/src/commands/stt/live_session.rs`
- `src-tauri/src/commands/stt/detection_jobs.rs`
- `src-tauri/src/commands/stt/mod.rs`
- `src-tauri/src/commands/stt/tasks.rs`

### Implementation

1. Replace stringly typed decisions at the Rust boundary with serializable enums:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresentationDecision {
    Reject,
    Suggestion,
    PreviewAuthorized,
    ReadingAuthorized,
    LiveAuthorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DetectionJob { Citation, Quotation, Request }
```

2. Replace the positional `results[index]`/`detections[index]` association with one carrier object used from generation to emission:

```rust
struct CandidateForPresentation {
    key: CandidateKey, // source + content type + canonical book/chapter/verse or EGW id
    result: DetectionResult,
    evidence: PresentationEvidence,
}
```

All filtering, sorting, EGW insertion, deduplication, and capping operate on this object. The policy grant is calculated per object after final candidate ordering. A candidate without an evidence record is downgraded to `Suggestion`/`Reject`; it is never granted by a fallback.

3. Carry actual lexical evidence into both `Detection` and `DetectionResult`:

```rust
pub has_lexical_quote: bool,
pub quote_coverage: f64,
pub candidate_margin: f64,
pub utterance_id: Option<u64>,
pub is_final_utterance: bool,
```

Set `quote_coverage` only from `exact_quote_confidence`/overlap analysis. Do not use rank score or aggregate confidence as a substitute.

4. Introduce a typed policy input that includes the synchronized user policy:

```rust
pub struct PresentationPolicy {
    pub auto_mode: bool,
    pub semantic_enabled: bool,
    pub direct_threshold: f64,
    pub semantic_threshold: f64,
    pub live_output_enabled: bool,
}
```

Synchronize this via `update_detection_settings`; remove every hard-coded `automation_live_enabled: true`. The policy decides authorization, while the frontend’s live toggle remains the final user-controlled live-output switch.

5. Scope `EvidenceLedger` to the active STT session state. Reset it at transcription start and stop; record only final logical utterance IDs. Partials contribute no confirmation count.

6. Make grants conservative:

| Job | Required evidence | Maximum automatic action |
| --- | --- | --- |
| Citation | canonical/allowed alias book match, complete chapter+verse grammar, non-fuzzy, final utterance, threshold met | reading + preview; live only when Auto Mode and live output are enabled |
| Quotation | final utterance, measured distinctive lexical span/coverage, candidate margin, threshold | preview/live; never starts reading mode |
| Request | final explicit request intent, grounded retrieval proof, decisive margin; if AI is enabled it must return high certainty from offered candidates | preview only; never starts reading mode or auto-queues |
| Chapter-only/fuzzy/ungrounded | any | suggestion or reject only |

7. Delete or route through the policy every pre-existing permission shortcut:

- `is_direct_reading_handoff` must consume an already issued `PresentationGrant`, not call `citation_grant(..., true)`.
- `run_direct_detection` must authorize before generating `DirectReadingCandidate` values.
- `check_reading_mode` accepts only `ReadingAuthorized` citation results.
- The backend must never emit a reading start for a semantic, request, chapter-only, partial, or fuzzy candidate.

### Tests

- All `presentation.rs` policy combinations: citation, quote, request, fuzzy, chapter-only, partial, low-score, ambiguous, Auto Mode on/off, live output on/off.
- Candidate-key association survives filter/reorder/EGW-insert/cap operations.
- A partial `John 3:16` is a suggestion and does not start reading; the corresponding final can do so.
- A fuzzy `Filipians chapter 4 verse 13` is shown as a suggestion/manual correction, never automatic reading/live.
- Ledger resets across session boundaries and does not count two provider updates from one utterance twice.
- `cargo clippy -- -D warnings` has zero warnings.

## 6. Phase 2 — isolate request retrieval from sermon quotation retrieval

### Files

- New: `src-tauri/crates/bible/src/request_retrieval.rs`
- `src-tauri/crates/bible/src/search.rs`
- `src-tauri/crates/bible/tests/retrieval_recall.rs`
- `src-tauri/crates/detection/src/pipeline.rs`
- `src-tauri/src/commands/stt/live_session.rs`
- `src-tauri/src/commands/deepseek.rs`
- `src/lib/deepseek-ranker.ts`

### Implementation

Move the current phrase-specific `build_topic_phrase_queries` additions into a named request-retrieval stage. It must return provenance rather than merely append BM25 strings:

```rust
pub struct RequestRetrievalPlan {
    pub intent: RequestIntent,
    pub required_entities: Vec<AnchorSet>,
    pub required_event_terms: Vec<AnchorSet>,
    pub query_expansions: Vec<String>,
}

pub struct GroundedRequestCandidate {
    pub candidate: Bm25Result,
    pub entity_event_coverage: f64,
    pub matched_anchors: Vec<String>,
}
```

Use a curated, data-backed scene/event index for durable synonyms:

```json
{
  "id": "joseph-pit",
  "entity": ["joseph"],
  "event": ["pit", "well", "cistern", "cast him"],
  "references": ["Genesis 37:24"]
}
```

Add scenes for the verified user requests first, but make the index data-driven rather than adding another independent `if` chain for every future complaint. Generic requests remain suggestions unless their named people/events are grounded in a retrieved candidate. AI receives this provenance and can only select a retrieved, grounded candidate; it cannot invent a verse.

### Tests

- Unit test each request plan’s aliases, anchor requirements, and canonical references.
- Retrieval recall tests for Paul/Silas, Joseph/pit/well/cistern, mark/beast, Jesus/walking/sea/water, plus a negative ambiguous `a verse about prison` case.
- Candidate grounding test: a high vector score that lacks Joseph/pit anchors cannot be request-authorized.
- AI contract test: no high-certainty model response can authorize a candidate absent from the grounded shortlist.

## 7. Phase 3 — make the frontend a strict consumer of grants

### Files

- `src/types/detection.ts`
- `src/lib/presentation-decision.ts`
- `src/lib/presentation-decision.test.ts`
- `src/lib/verse-detection-workflow.ts`
- `src/lib/verse-detection-workflow.test.ts`
- `src/lib/presentation-workflow.ts`
- `src/lib/presentation-workflow.test.ts`

### Implementation

1. Make `authorization` and `job` required in the TypeScript wire type. Keep a temporary parser at the Tauri boundary that treats missing fields as `reject`; do not infer default authority from `source === 'direct'`.
2. Remove frontend confidence thresholds as authorization decisions. The frontend may use confidence only to sort suggestion cards.
3. `selectPreviewHit` considers only `mayPreview(detection)` candidates. `previewVerseAndMaybeAutoLive` receives both `autoLive: mayGoLive(detection)` and `startReading: mayStartReading(detection)`.
4. Queueing accepts only `mayAutoQueue(detection)`. Requests and quotations never auto-queue under the citation policy.
5. Remove the legacy frontend semantic confirmation map once backend evidence has become authoritative. Do not keep two confirmation engines. The frontend retains only stale-batch cancellation and rendering logic.
6. Update every existing test fixture to state authorization deliberately. Never have `makeDetection` silently manufacture a live grant.

### Tests

- Convert all 16 currently failing workflow tests to explicit, correct authorization fixtures.
- Add a test that a missing/unknown authorization is rejected, including an old backend event.
- Add a test that a live-authorized quotation may go live but does not call `set_reading_mode_reference`.
- Add a test that preview-authorized request shows the verse but neither starts reading nor goes live.
- Add a test that a direct citation below configured policy threshold remains suggestion-only.
- Retain all existing EGW arbitration behavior with explicit EGW grants, rather than letting Bible defaults leak into EGW cases.

## 8. Phase 4 — end-to-end action replay and diagnostic evidence

### Files

- New: `src-tauri/crates/detection/tests/presentation_replay.rs`
- New: `src/lib/presentation-action-replay.test.ts`
- New: `scripts/run-presentation-evidence.mjs`
- New generated artifact: `docs/reports/presentation-authorization-verification.json`
- `src-tauri/src/commands/stt/utils.rs`
- `src-tauri/src/commands/stt/live_session.rs`

### Implementation

1. Build a test-only adapter that consumes fixture STT events and records these effects:

```rust
struct PresentationEffects {
    suggestions: Vec<CandidateKey>,
    preview: Option<CandidateKey>,
    live: Option<CandidateKey>,
    queue: Vec<CandidateKey>,
    reading: Option<CandidateKey>,
}
```

The adapter must call the same policy and handoff functions used by production, not a benchmark-only selector.

2. Add a matching frontend contract replay that feeds authorized Tauri payloads into `scheduleVerseDetections` and spies on preview, live, queue, and `set_reading_mode_reference` invocations.

3. Add an operator-opt-in, local-only diagnostic mode for a bounded session. By default record only utterance ID, candidate IDs, job, evidence booleans/coverage, authorization, and action. A separately consented debug mode can retain short transcript snippets locally for a limited time. Release logging stays privacy-preserving.

4. Generate the verification JSON in CI from the real replay harness. It records fixture hash, commit, platform, test counts, false-action count, direct recall, request recall, and latency percentiles.

### Mandatory acceptance tests

| Test class | Required assertion |
| --- | --- |
| Current-change success | All existing focused backend/frontend tests pass and `git diff --check` is clean. |
| False-direct regression | `makes one` yields no preview/live/queue/reading action. |
| Chapter-only regression | `Joshua chapter one` yields at most a suggestion/context; no output or reading start. |
| Fuzzy regression | fuzzy `Filipians` cannot create automatic presentation state. |
| Semantic regression | unrelated 98% semantic candidate with no lexical evidence yields a suggestion only. |
| Finality regression | partial plus final from one utterance cannot act as two confirmations. |
| Request regression | each named request either resolves to the expected canonical verse set or stays suggestion-only; it never opens an unrelated chapter. |
| User-output regression | Theology/EGW passages from the follow-up transcript stay silent for Bible presentation. |
| Explicit-reference preservation | Genesis 1:8 and Joshua 1:9 still preview/live/read according to configured automation. |
| AI safety | a ranker selection outside the current grounded shortlist is rejected. |

## 9. Phase 5 — objective quality gates

Run the following after all focused red-to-green tests pass:

```powershell
git diff --check
npm.cmd run test:unit -- src/lib/presentation-decision.test.ts src/lib/verse-detection-workflow.test.ts src/lib/presentation-workflow.test.ts src/lib/presentation-action-replay.test.ts
cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection presentation
cargo test --manifest-path src-tauri/Cargo.toml -p rhema-detection --test presentation_replay
cargo test --manifest-path src-tauri/Cargo.toml -p rhema-bible --test retrieval_recall
cargo test --manifest-path src-tauri/Cargo.toml -p sabbathcue
cargo clippy --manifest-path src-tauri/Cargo.toml -p sabbathcue --all-targets -- -D warnings
npm.cmd run typecheck
npm.cmd run lint
node scripts/run-presentation-evidence.mjs
```

The evidence report must meet these gates:

| Metric | Gate |
| --- | --- |
| Forbidden preview/live/queue/reading actions in permanent silent fixtures | 0 |
| `makes one`, chapter-only, fuzzy-book, request, and semantic policy replay failures | 0 |
| Explicit supplied references | 2/2 canonical matches |
| Supplied named requests | 4/4 expected canonical set retrieved; authorization only with grounding |
| Current fixture corpus direct-reference recall | no decrease from recorded baseline |
| Current fixture corpus false automatic actions | lower than baseline; target 0 |
| p95 detection decision latency | no more than 10% above recorded baseline without documented approval |
| Rust/TypeScript warnings | 0 |

The CI workflow must upload the JSON evidence artifact and fail when any gate fails. That report is the recorded proof that the new code is safer, not an assertion based on test names.

## 10. Delivery sequence and commit boundaries

1. Commit the replay corpus and its red test independently. It documents the real defects before policy integration changes behavior.
2. Commit the typed policy carrier and backend-only action handoff; run Rust policy/replay tests.
3. Commit request-retrieval provenance and its recall tests.
4. Commit the frontend contract migration; resolve all 16 existing workflow failures without weakening assertions.
5. Commit diagnostics/evidence script and CI gate; attach the generated verification JSON.
6. Update `docs/CODEBASE.md` in the final implementation commit with the new single-owner authorization flow and test command receipts.

## 11. Evidence sources for this plan

All findings were inspected locally on 2026-08-18:

- Current worktree diff: 22 modified files, 2 new files.
- `src-tauri/crates/detection/src/presentation.rs`.
- `src-tauri/src/commands/stt/detection_logic.rs` and `live_session.rs`.
- `src/lib/verse-detection-workflow.ts` and its failing focused suite.
- `src-tauri/crates/bible/src/search.rs` and passing `retrieval_recall` suite.
- Runtime log: `C:\Users\fanel\AppData\Local\com.bongandlovu.sabbathcue.personal\logs\SabbathCue Personal.log`, entries from 2026-08-18 19:21–19:25.

No implementation code is changed by this plan.
