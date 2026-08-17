# Cerebras GPT-OSS-120B AI Ranking Integration Plan

[PLAN VERIFIED: SAFE FOR PRESENTATION]

## Plan metadata

| Field | Value |
| --- | --- |
| Product | SabbathCue |
| Goal | Add Cerebras `gpt-oss-120b` as a selectable, secure AI ranking provider while retaining DeepSeek as the default. |
| Scope | Optional semantic ranking only; the local detector remains the source of candidate verses. |
| User data | A short transcript window plus at most eight local candidate packs are sent only when the selected provider is enabled and configured. |
| Safety invariant | AI may select an existing candidate or abstain. It must never create a verse, replace direct detection, or auto-project a result. |
| Primary deadline | Preserve the existing 1.8-second end-to-end ranking deadline and no-retry behaviour. |
| Rollback | Change the provider to DeepSeek or turn off AI ranking; no migration is irreversible. |

## Triage and confirmed baseline

The current implementation already sends a bounded candidate summary in this form:

```text
VERSE_REFERENCE — VERSE_TEXT
```

That is the right foundation for the improved workflow. The change is to preserve the reference and verse text as explicit typed fields, include them as a labelled evidence pack in both provider prompts, and make the provider selection generic rather than DeepSeek-only.

The AI ranker is an optional tie-breaker for ambiguous semantic candidates. Local direct detection, candidate generation, confidence thresholds, debouncing, and projection safeguards remain unchanged.

## Architecture decision

```text
speech transcript
    -> local verse detection and shortlist (unchanged)
    -> provider-aware ranking gate
    -> selected provider: DeepSeek or Cerebras
    -> strict selection of one existing candidate, or abstain
    -> existing suggestion/projection safeguards (unchanged)
```

### Provider behaviour

| Concern | DeepSeek | Cerebras GPT-OSS-120B |
| --- | --- | --- |
| Default for existing users | Yes | No |
| Transport | Existing streaming one-letter response | Non-streaming JSON Schema response |
| Model | Existing configured DeepSeek model | `gpt-oss-120b` |
| Evidence | Explicit reference plus verse text | Explicit reference plus verse text |
| Accept result | Existing valid candidate letter | Valid schema, candidate letter, and `certainty: "high"` |
| Abstain | `N` or malformed/late output | `N`, uncertain, malformed, invalid, or late output |
| Deadline | 1.8 seconds | 1.8 seconds |
| Retry policy | No retries | No retries |

## Phase 1 — Types, persisted settings, and UI boundary

### Files

- `src/types/ai-ranking.ts`
- `src/stores/settings-store.ts`
- `src/stores/settings-store.test.ts`
- `src/hooks/use-cerebras-key-settings.ts` (new)
- `src/components/settings/sections/AiRankingSection.tsx`
- `src/components/settings/sections/AiRankingSection.test.tsx`

### Changes

1. Replace the lossy `summary`-only candidate shape with a typed candidate pack:

   ```ts
   interface RankingCandidate {
     id: string
     reference: string
     verseText: string
     confidence: number
   }
   ```

   Keep any transient display summary derived locally, not as the only evidence passed to a provider.

2. Generalise persisted ranking state:

   ```ts
   type AiRankingProvider = 'deepseek' | 'cerebras'

   aiRankingEnabled: boolean
   aiRankingProvider: AiRankingProvider
   hasDeepseekApiKey: boolean
   hasCerebrasApiKey: boolean
   ```

3. Migrate existing persisted settings safely:

   - If legacy `deepseekRankingEnabled` exists, use its value for `aiRankingEnabled`.
   - Default `aiRankingProvider` to `deepseek`.
   - Do not remove or invalidate existing DeepSeek credentials.
   - Do not enable Cerebras automatically, even if a key exists.

4. Add a `use-cerebras-key-settings` hook using the same generic key-action pattern as DeepSeek, backed by native commands for set, clear, has, and validate.

5. Update the AI Ranking settings section:

   - Present a provider selector: DeepSeek or Cerebras GPT-OSS-120B.
   - Show only the selected provider's secure-key controls.
   - Disable the ranking toggle when the selected provider has no valid key.
   - Explain the bounded data flow: a short recent phrase and up to eight locally detected reference-and-verse candidates.
   - Removing the active provider key disables ranking. Removing the inactive key does not change the active ranking setting.
   - Preserve existing accessibility labels, keyboard operation, and visible error states.

### Tests

- Migration preserves legacy DeepSeek enabled state and selects DeepSeek by default.
- An unset or invalid provider key disables the corresponding provider path.
- Switching providers does not discard either saved key-presence state.
- Removing an active key disables ranking; removing an inactive key does not.
- The UI renders the provider selector, appropriate credential control, privacy copy, and errors.

## Phase 2 — Native secure storage and Cerebras boundary

### Files

- `src-tauri/src/commands/secrets.rs`
- `src-tauri/src/commands/deepseek.rs`
- `src-tauri/src/lib.rs`

### Secure key management

1. Add `cerebras_api_key` set, clear, and presence commands to the existing OS-keychain implementation. The value must never be stored in Zustand, localStorage, logs, fixtures, or source control.
2. Add an equivalent mock-keychain test suite to prove key lifecycle behaviour without using a real key.
3. Add `validate_cerebras_api_key`, retrieving the key only in Rust and making a bounded, authenticated validation request.
4. Map actionable validation outcomes without exposing sensitive response bodies:

   - missing key;
   - invalid/unauthorized key;
   - billing or account restriction;
   - rate limited;
   - provider unavailable or network failure;
   - deadline exceeded.

### Ranking command

1. Keep `rank_detection_candidates` as the stable Tauri command name and add an `provider` request field. The Rust boundary dispatches internally to DeepSeek or Cerebras.
2. Validate before sending a network request:

   - provider is recognised;
   - transcript is non-empty and at or below its existing cap;
   - candidate count is between one and eight;
   - every candidate has an allowed identifier, bounded reference and verse text, and finite confidence;
   - a provider key exists.
3. Retain the global 1.8-second timeout, no retry policy, candidate-ID-only logs, and abstain-on-failure contract.

### Cerebras request contract

Send a non-streaming request to:

```text
POST https://api.cerebras.ai/v1/chat/completions
Authorization: Bearer <keychain-only credential>
model: gpt-oss-120b
```

Use a fixed `developer` instruction and a user payload with the transcript and labelled candidate packs:

```text
Candidate A
Reference: Acts 16:25
Verse: About midnight Paul and Silas were praying and singing hymns to God...
Confidence: 82%
```

The instruction must require the model to compare the phrase to both the reference and verse, select only an offered letter, and abstain whenever the evidence is not clear.

Use strict JSON Schema output:

```json
{
  "choice": "A | B | C | D | E | F | G | H | N",
  "certainty": "high | uncertain"
}
```

Every schema object includes `additionalProperties: false`. Set `reasoning_effort: "low"` and `reasoning_format: "hidden"` to favour the live-speech deadline. A result is accepted only if the schema parses, the letter is currently offered, and certainty is `high`; every other result becomes an abstention.

### Native tests

- Request builder uses the configured endpoint, bearer authentication, `gpt-oss-120b`, bounded transcript/candidates, hidden low-effort reasoning, and strict schema.
- The prompt includes reference and verse as separate labelled evidence.
- Valid high-certainty selection resolves only to an offered candidate.
- Uncertain, unknown-letter, malformed JSON, schema mismatch, empty response, timeout, request error, and validation error abstain safely.
- Cerebras and DeepSeek error/circuit-breaker state is isolated.
- Neither logs nor error messages contain the API key, verse payload, or raw provider response.

## Phase 3 — Frontend ranking workflow and resilience

### Files

- `src/lib/deepseek-ranker.ts`
- `src/lib/deepseek-ranker.test.ts`
- `src/lib/verse-detection-workflow.ts`
- `src/lib/verse-detection-workflow.test.ts`

### Changes

1. Rename internal concepts that are only named for DeepSeek where this improves clarity, while preserving public command compatibility where valuable. For example, use a provider-neutral ranking gate in the workflow.
2. Construct candidate packs from the detected verse reference and full bounded verse text. Retain the existing transcript, candidate-count, and payload caps.
3. Make the gate depend on:

   - `aiRankingEnabled`;
   - selected provider;
   - selected provider key presence;
   - existing confidence threshold and direct-detection suppression rules.

4. Partition stability cache and circuit breaker by provider. A DeepSeek provider failure must not block Cerebras, and vice versa.
5. Preserve the current local-first behaviour:

   - no network call for direct/high-confidence detection;
   - no network call without a configured selected-provider key;
   - no change to suggestion-only or operator approval behaviour;
   - a ranker failure, timeout, or abstention leaves local detection as-is.

### Tests

- A phrase such as “Paul and Silas singing in prison” sends `Acts 16:25` with both reference and verse text in its candidate pack.
- A selected winner always originates from the current local candidate set.
- A locally confident direct match never calls either provider.
- Switching from DeepSeek to Cerebras invokes the same ranking command with the requested provider.
- Missing selected-provider key, circuit-open state, duplicate/stale scheduling, late response, parse failure, uncertain result, or deadline expiry cause no unsafe state change.
- Cache and breaker keys do not cross-contaminate providers.

## Phase 4 — Documentation and verification

### Files

- `README.md`
- `docs/CODEBASE.md`
- `docs/procurement/privacy-data-flow.md`
- `docs/procurement/security-overview.md`

### Documentation changes

1. Describe AI ranking as an optional, user-selected provider feature: DeepSeek or Cerebras GPT-OSS-120B.
2. Document the exact bounded data flow and the local-first/abstention guardrails.
3. Document native OS-keychain storage and the absence of credential storage in application settings.
4. Update the architecture map, supported configuration, privacy, and security material. Do not promise availability, unlimited capacity, zero cost, or model accuracy.

### Automated verification

Run focused tests first, then project checks:

```powershell
npm.cmd run test:unit -- src/lib/deepseek-ranker.test.ts src/lib/verse-detection-workflow.test.ts src/stores/settings-store.test.ts src/components/settings/sections/AiRankingSection.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml -p sabbathcue commands::deepseek
npm.cmd run typecheck
npm.cmd run lint
cargo clippy --manifest-path src-tauri/Cargo.toml -p sabbathcue --all-targets -- -D warnings
npm.cmd run build
```

### Manual acceptance pass

1. Open Settings → AI Ranking.
2. Add the Cerebras key through the secure UI; confirm it is reported as configured without ever displaying it.
3. Select Cerebras GPT-OSS-120B, enable ranking, and validate the key.
4. Feed the local fixture phrase “Paul and Silas singing in prison” with Acts 16:25 present in the candidate shortlist.
5. Confirm the request is optional, timely, and either selects Acts 16:25 only when high-certainty or abstains without disrupting local detection.
6. Switch back to DeepSeek and verify the existing workflow still works independently.
7. Remove the Cerebras key and verify the active ranking toggle turns off; restore it only through the secure UI if desired.

## Failure-mode and rollback matrix

| Condition | Safe behaviour | User recovery |
| --- | --- | --- |
| No provider key | Do not invoke the ranker | Add a key for the selected provider or disable ranking. |
| Invalid key / 401 | Disable only the selected provider path after validation failure | Replace key in secure settings. |
| Rate limit / account restriction | Abstain, retain local result, apply provider-specific breaker | Wait or switch provider. |
| Timeout / network failure | Abstain within deadline; no retry | Local detection continues; retry on a later phrase. |
| Malformed model output | Abstain | No user action needed. |
| Model chooses unavailable candidate | Reject and abstain | No user action needed. |
| Low certainty | Abstain | No user action needed. |
| Provider service regression | Select DeepSeek or disable AI ranking | No data migration required. |
| Cerebras integration rollback | Stop selecting Cerebras or remove the provider branch | Existing DeepSeek path and settings continue to work. |

## Adversarial audit

1. **Bad transcript:** capped, validated, and treated as untrusted input.
2. **Prompt injection in speech:** fixed developer instructions, closed candidate set, strict schema, and model output cannot execute actions.
3. **Wrong verse:** selection is limited to already locally detected candidate IDs; AI cannot invent a reference.
4. **Ambiguity:** `certainty: uncertain`, `N`, malformed output, or failure always abstains.
5. **Slow response:** a single global 1.8-second deadline and no retries protect live operation.
6. **Provider outage:** circuit breaker is isolated per provider and local detection remains available.
7. **Secret leakage:** credentials remain in the OS keychain; logs omit keys and raw payloads/responses.
8. **Regression:** DeepSeek remains default and is exercised by the existing path and dedicated regression tests.

## External references used for the design

- [Cerebras authentication](https://inference-docs.cerebras.ai/api-reference/authentication)
- [Cerebras public models](https://inference-docs.cerebras.ai/api-reference/models/public-models)
- [Cerebras structured outputs](https://inference-docs.cerebras.ai/capabilities/structured-outputs)
- [Cerebras reasoning controls](https://inference-docs.cerebras.ai/capabilities/reasoning)
- [Cerebras errors](https://inference-docs.cerebras.ai/support/error)
- [Cerebras rate limits](https://inference-docs.cerebras.ai/support/rate-limits)
- [OpenAI: introducing GPT-OSS](https://openai.com/index/introducing-gpt-oss/)

## Completion criteria

The implementation is ready when all automated checks pass, the manual acceptance pass succeeds, Cerebras and DeepSeek credentials are independently secured and selectable, and an uncertain/failed Cerebras result demonstrably leaves the existing local detection and operator safeguards intact.
