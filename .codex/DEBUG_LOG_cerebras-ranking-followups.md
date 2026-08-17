# Debug log — Cerebras ranking follow-ups

## Bug definition (step 1)

```text
BUG / TICKET:       cerebras-ranking-followups
SYMPTOM (exact):    parse_cerebras_response accepts the first character of a multi-character
                    high-certainty choice; changing provider does not clear enabled state;
                    provider documentation has stale DeepSeek-only references; no workflow-level
                    Paul-and-Silas regression test was added.
EXPECTED:           Accept only an exact offered letter; require an explicit active-provider
                    opt-in; provider documentation reflects both providers; workflow test covers
                    the Acts 16:25 candidate path.
ACTUAL:             Current implementation differs as described above.
DELTA:              The saved integration plan's output-validation, safe-toggle, documentation,
                    and workflow-test requirements are not all met.
REPRO STATUS:       NOT YET RUN
ENVIRONMENT:        Windows, SabbathCue workspace, current uncommitted Cerebras integration.
FIRST OBSERVED:     2026-08-17 review.
LAST KNOWN GOOD:    Not applicable: regressions introduced with the uncommitted integration.
RECENT CHANGES:     Cerebras GPT-OSS-120B provider integration.
IN SCOPE (may modify): src-tauri ranking parser/tests; AI-ranking settings/UI/tests;
                        workflow tests; README; docs/CODEBASE.md.
OUT OF SCOPE:       Provider networking model, credentials, detection algorithm, live API calls.
DEFINITION OF FIXED: Exact-choice and provider-opt-in regression tests are RED then GREEN;
                       workflow candidate fixture passes; docs are current; focused checks pass.
```

## Reproduction (step 2) and root cause (step 6)

```text
Rust regression (RED):
assertion `left == right` failed
  left: Some("44:16:25")
 right: None

Settings regression (RED):
AssertionError: expected true to be false
Expected: false
Received: true

ROOT CAUSE: `parse_cerebras_response` derives its result from the first character of an
untrusted choice string, and `setAiRankingProvider` changes only the provider field without
clearing the prior provider's enabled state.
CAUSE→SYMPTOM EVIDENCE: The Rust test passed `"A extra"` and received candidate A; the UI
test selected Cerebras with ranking enabled and observed `aiRankingEnabled === true`.
```

## Regression tests (step 7 → 8)

```text
RED:
parse_cerebras_response_abstains_on_multi_character_choice
left: Some("44:16:25")
right: None

requires a fresh opt-in after changing ranking provider
Expected: false
Received: true

GREEN:
parse_cerebras_response_abstains_on_multi_character_choice ... ok
requires a fresh opt-in after changing ranking provider ... passed
passes the Paul-and-Silas candidate batch to the configured Cerebras gate ... passed
```

## Verification and root-cause writeup (steps 8–9)

```text
VERIFICATION:
- Focused frontend tests: 133 passed.
- Rust command tests: 20 passed.
- cargo clippy --all-targets -- -D warnings: passed.
- npm.cmd run typecheck: passed.
- npm.cmd run lint: passed.
- npm.cmd run build: passed.
- git diff --check: passed with no whitespace errors.

SIBLING GREP:
- The remaining `chars().next()` calls are guarded by an exact one-character count
  in the Cerebras parser or intentionally process the single-letter DeepSeek stream.
- The stale README overview and CODEBASE settings-table references were updated.

ROOT-CAUSE WRITEUP — cerebras-ranking-followups — 2026-08-17
1. SYMPTOM: malformed structured output could select an offered candidate, and a provider
   change retained an earlier opt-in state.
2. ROOT CAUSE: the parser used the first character of an unvalidated string, and the provider
   setter did not reset the enabled flags when the provider changed.
3. MECHANISM: an external response of `A extra` became A; a selected new provider inherited
   the previous provider's enabled state.
4. HOW FOUND: deterministic regression tests captured both observed values before the fix.
5. THE FIX: require exactly one trimmed choice character and clear enabled state only when the
   provider changes.
6. VERIFICATION: RED-to-GREEN regression tests and the focused verification commands passed.
7. PREVENTION: parser, UI, and workflow regression tests now cover the safety contract.

Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression test (red → green) attached:    YES
Original reproduction now passes:          YES
Definition of fixed verified:              YES
```
