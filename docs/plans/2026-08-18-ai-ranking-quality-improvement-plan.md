# AI Ranking Quality Improvement Plan

## Status and purpose

**Status:** Foundational implementation complete (Phases 0–3), with the
Lazarus retrieval miss fixed as the first Phase 5 loop. Phase 4 shadow
calibration and broader live retrieval-miss feedback remain opt-in follow-up
work because they require reviewed operator labels and/or a live provider
trial.

### Implementation evidence

This execution delivered the deterministic, provider-safe foundation:

- versioned offline replay fixtures and metrics;
- canonical transcript selection and content-aware, provider-scoped cache
  identity;
- bounded local candidate evidence for both provider payloads;
- a strict Cerebras decision contract with `match_basis` validation;
- a deterministic Lazarus/“come out” retrieval anchor plus a recall regression;
- regression coverage for retrieval, ranking, STT finality, and stale-batch
  safeguards.

No live provider call or automatic policy change was enabled. The existing
local-first and abstention safeguards therefore remain the production policy
until shadow results are reviewed.

This is a follow-up to the Cerebras provider integration plan. Its purpose is
to improve the quality, consistency, and measurability of AI-assisted ranking
of *already retrieved* Bible candidates. It does not turn the model into a
Bible search engine.

## Verified baseline

The current ranker is intentionally bounded:

```text
STT batch
  -> local direct + semantic retrieval
  -> up to 8 semantic candidates
  -> optional provider ranker
  -> existing semantic confirmation and preview safeguards
```

The ranker is invoked only for ambiguous semantic candidates, after a 400 ms
stability delay. Direct references and locally decisive semantic results stay
local. The request contains a transcript, reference, verse text, and local
confidence; a model can select only an offered candidate or abstain.

The pre-implementation design had five material limits (the first four are
addressed by the implementation evidence above; the fifth is addressed by
the replay corpus, while live labelled evaluation remains follow-up work):

1. The ranker cannot select a verse that local retrieval did not include.
2. The cache key contains the provider, transcript, and candidate IDs, but not
   candidate confidence or verse text. A translation or score change can reuse
   a stale result.
3. The request transcript is selected as the longest candidate snippet rather
   than a canonical transcript for the detection batch.
4. Candidate evidence is too thin for reliable scene-level distinctions: the
   model does not receive the local rank score, phrase overlap, or topic/book
   evidence that produced the candidate.
5. DeepSeek and Cerebras have different response contracts, and there is no
   labelled ranking replay set to determine whether a prompt or policy change
   improves accuracy.

## Non-negotiable safety invariants

- AI may choose only a candidate from the current local candidate set, or
  abstain.
- Direct references remain authoritative and never wait on, or are overridden
  by, AI.
- AI output remains subject to the existing semantic confidence, stability,
  stale-batch, and auto-live safeguards.
- No ranking request exceeds 500 transcript characters or eight candidates;
  provider calls retain the 1.8-second deadline and no-retry policy.
- API keys stay in the OS keychain. Logs and persisted feedback must not store
  API keys, raw provider responses, or complete spoken transcripts.
- A retrieval miss is handled by local retrieval improvements, not a model
  hallucinating a Bible reference.

## Phase 0 — Establish a measurable ranking baseline

### Files

- `data/detection-fixtures/ai-ranking-cases.json` (new)
- `src/lib/deepseek-ranker.test.ts`
- `src-tauri/src/commands/deepseek.rs`
- `src-tauri/crates/detection/src/bin/detection_accuracy.rs` or a dedicated
  ranking-replay command (new)

### Work

1. Create a versioned fixture corpus of ambiguous ranking cases. Each case
   records a sanitized transcript, an ordered candidate pack, the expected
   reference or abstention, and a category:

   - exact quotation;
   - paraphrased event/person request;
   - named-book request;
   - intentionally ambiguous request;
   - retrieval miss where abstention is correct;
   - noisy or conversational STT framing;
   - partial-to-final transcript progression.

2. Seed it with the recent Acts 12:5, Acts 16:25, John 6, and Genesis 2:8
   regressions, then add anonymized real-session examples only after review.

3. Add an offline replay runner that uses captured/mock provider decisions;
   unit tests must never call a live provider or require a key.

4. Report these metrics per provider and overall:

   - top-1 correctness when a reference should be selected;
   - abstention precision and abstention recall;
   - false preview / false auto-live count;
   - candidate-set recall (whether the expected verse was offered at all);
   - median and p95 ranking latency from integration/shadow runs.

### Exit criteria

- Fixture cases run deterministically in CI.
- Baseline metrics are saved with the fixture version.
- Acceptance thresholds are set from the baseline before changing ranking
  policy; no arbitrary confidence-weight tuning is merged first.

## Phase 1 — Correct request identity and canonical batch context

### Files

- `src/lib/deepseek-ranker.ts`
- `src/lib/deepseek-ranker.test.ts`
- `src/lib/verse-detection-workflow.ts`
- `src/lib/verse-detection-workflow.test.ts`

### Work

1. Replace the current ID-only candidate cache fingerprint with a bounded,
   deterministic fingerprint of:

   ```text
   provider + normalized batch transcript +
   candidate id + reference + verse text + rounded confidence + rank score
   ```

   The cached value remains the selected candidate ID, never a provider
   response or free-form rationale.

2. Derive one canonical transcript per detection batch before candidate
   construction. Do not select it merely because it is longest. Prefer the
   normalized common semantic snippet; if the batch contains conflicting
   snippets, use the newest batch context or abstain from ranking rather than
   sending a mixed request.

3. Keep cache scope provider-specific and retain the current generation/stale
   response protection.

### Tests

- Same candidate IDs with changed confidence, verse text, or active
  translation cause a cache miss.
- Reordered candidates with identical evidence resolve to the same candidate
  ID, not an old letter position.
- A conflicting transcript batch never makes a request with an unrelated
  longest snippet.
- A stale response cannot update the AI suggestion or preview after a newer
  batch arrives.

## Phase 2 — Give the model verifiable ranking evidence

### Files

- `src/types/ai-ranking.ts`
- `src/types/detection.ts`
- `src/lib/deepseek-ranker.ts`
- `src-tauri/src/commands/deepseek.rs`
- `src-tauri/crates/detection/src/pipeline.rs`

### Candidate contract

Extend the candidate pack without increasing the candidate cap:

```ts
interface RankingCandidate {
  id: string
  reference: string
  verseText: string
  confidence: number
  evidence: {
    rankScore: number
    namedBookMatch: boolean
    exactPhraseMatch: boolean
    overlapTerms: string[] // capped and sanitized
  }
}
```

Topic-anchor and retrieval-tier fields are intentionally deferred: the
current frontend `DetectionResult` does not carry a stable tier/anchor
provenance field, so the implementation does not manufacture one. People and
event support is represented by the bounded deterministic overlap terms.

### Work

1. Thread only evidence that local retrieval can calculate deterministically.
   Do not ask the model to infer invisible retrieval facts or to generate
   Bible metadata.
2. Keep the evidence compact: at most six overlap terms per candidate, with
   character and finite-value caps enforced at the Tauri boundary.
3. Update both provider payload builders and prompts to compare evidence in
   this order:

   - explicit reference/book;
   - quoted phrase overlap;
   - named people plus described event;
   - local retrieval evidence;
   - abstain when no offered candidate has support.
4. Preserve the separate reference and verse-text fields. Never reduce the
   request to a summary string.

### Tests

- The Paul-and-Silas fixture sends people/event evidence for Acts 16:25.
- A candidate with generic keyword overlap cannot outrank a candidate with
  matching named people and event solely through a confidence tie.
- Evidence caps, character caps, and malformed/unknown tiers fail closed.
- No candidate pack contains a raw API key, unbounded transcript, or a verse
  outside the local set.

## Phase 3 — Use one internal decision contract across providers

### Files

- `src-tauri/src/commands/deepseek.rs`
- `src/lib/deepseek-ranker.ts`
- `src/lib/deepseek-ranker.test.ts`

### Work

1. Normalize each provider response into an internal `RankingDecision`:

   ```text
   choice: offered candidate ID | abstain
   certainty: high | uncertain
   match_basis: reference | quote | people_event | thematic | none
   ```

2. Cerebras keeps strict JSON Schema. Add `match_basis` as a closed enum for
   diagnostics; it is not trusted as proof and an invalid value becomes an
   abstention.
3. Keep DeepSeek's streaming one-letter path until a structured-output
   alternative demonstrates equal or better p95 latency in replay. Normalize
   the current letter into the same internal decision with an unknown basis.
4. Keep choice validation strict: an unoffered ID, an uncertain decision,
   malformed response, timeout, or provider error always becomes abstention.

### Tests

- Both providers yield the same accepted/abstained internal states for valid,
  uncertain, malformed, late, and unoffered choices.
- A diagnostic basis can never bypass certainty, candidate-set validation, or
  local workflow gates.

## Phase 4 — Calibrate policy in shadow mode before changing auto behaviour

### Files

- `src/lib/verse-detection-workflow.ts`
- `src/lib/detection-feedback.ts`
- `src/lib/workflow-trace.ts`
- ranking replay fixture and runner from Phase 0

### Work

1. Add an opt-in shadow mode: obtain an AI decision, record a privacy-safe
   decision fingerprint and outcome, but do not let it alter preview or queue
   selection.
2. Compare local-first and AI-reranked outcomes against the labelled replay
   corpus and reviewed operator feedback.
3. Only after the data supports it, introduce a bounded deterministic fusion
   policy. The model may break a local ambiguity; it must not overcome a
   decisive local margin, direct reference, confidence floor, or stability
   requirement.
4. Roll out by provider independently. A provider whose false-selection rate
   regresses remains suggestion-only or is disabled without affecting local
   detection.

### Exit criteria

- The proposed policy improves correct selections on the held-out fixture
  corpus without increasing false previews or false auto-live events.
- p95 response time remains within the existing 1.8-second deadline.
- Provider failure, circuit-open, and abstention paths leave local behaviour
  unchanged.

## Phase 5 — Retrieval-miss loop

AI ranking must not be used to paper over missing candidates. For every
fixture where the expected verse is absent from the candidate set:

1. Classify the miss as conversational noise, book-name issue, person/event
   anchor gap, translation vocabulary mismatch, or vector/FTS disagreement.
2. Add a narrow deterministic retrieval improvement and a recall regression
   test in `src-tauri/crates/bible/tests/retrieval_recall.rs` or the relevant
   pipeline test.
3. Re-run the ranking corpus only after the expected verse is present in the
   shortlist. The ranker should abstain when the candidate set remains wrong.

The first applied loop classifies “Lazarus, come out” as a name/event anchor,
adds a narrow `Lazarus` topic query, and verifies John 11:43 through the real
SQLite FTS recall test. The replay corpus retains a deliberately absent
candidate case to ensure the ranker still abstains when retrieval is genuinely
wrong.

## Verification sequence

Run focused checks during each phase, then the complete suites before merge:

```powershell
npm.cmd run test:unit -- src/lib/deepseek-ranker.test.ts src/lib/verse-detection-workflow.test.ts
cargo test --manifest-path src-tauri/Cargo.toml -p sabbathcue commands::deepseek
cargo test --manifest-path src-tauri/Cargo.toml --workspace --all-targets
npm.cmd run test:unit
npm.cmd run typecheck
npm.cmd run lint
npm.cmd run build
npm.cmd run test:e2e
```

If Docker is available, also run `npm.cmd run test:db`. Every live-provider
trial is manual and opt-in; it is never part of CI.

## Rollback

- Disable shadow mode or AI ranking: local retrieval continues unchanged.
- Disable one provider: the other provider and local-first path remain intact.
- Revert a candidate-evidence field behind a compatible optional schema; old
  clients should safely omit unknown evidence and receive abstention rather
  than a guessed result.
