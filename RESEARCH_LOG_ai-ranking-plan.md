# Research Log - AI ranking quality plan review

## Question definition

```
QUESTION(S):          Does docs/superpowers/plans/2026-07-31-ai-ranking-quality.md correctly address the observed 2026-07-31 ranking defects, and what changes are required before implementation?
SUFFICIENT ANSWER:    A recommendation to approve, approve with changes, or reject, with each material claim traced to the plan and independent source code evidence.
FRESHNESS REQUIRED:   Current workspace state as of 2026-07-31; historical live-test metrics are accepted only as stated plan evidence.
KNOWN UNKNOWNS:       The local workspace does not contain the referenced AppData live log, so its numeric baseline cannot be independently recalculated here.
OUT OF SCOPE:         Implementing the plan, changing production code, and re-running the live cloud-STT service test.
DATE STARTED:         2026-07-31
```

## Claims ledger

| # | Claim | Source (path + locator) | Tier | Info date | Corroborated by | Grade |
|---|-------|-------------------------|------|-----------|-----------------|-------|
| 1 | Direct and semantic results are emitted as separate frontend events, while the existing gate only sees its supplied batch. | src-tauri/src/commands/stt/mod.rs:522-596; src-tauri/src/commands/stt/live_session.rs:421,652; src/lib/deepseek-ranker.ts:60-72 | Primary code | 2026-07-31 | DEBUG_LOG_ai-ranking.md B.2/B.5 | VERIFIED |
| 2 | The plan's recent-direct state closes the cross-event suppression gap. | Plan Task 1; sources in claim 1 | Design inference | 2026-07-31 | Direct control-flow comparison | VERIFIED |
| 3 | A 0.92 direct item suppresses at the 0.90 default gate when it shares the batch. | src/lib/deepseek-ranker.ts:65-71 | Primary code | 2026-07-31 | DEBUG_LOG_ai-ranking.md B.1 targeted Vitest run | VERIFIED |
| 4 | The plan's direct suppression timestamp must be recorded before all ranker early exits and must be called for every event. | Plan Task 1 and Task 5 complete-body snippet | Design constraint | 2026-07-31 | Existing ranker early exits at deepseek-ranker.ts:91-92 | VERIFIED |
| 5 | Task 3 loses the newest stable request when a newer batch arrives after an older debounce has fired but before its cloud request finishes. | Plan Task 3:567-598; src/lib/verse-detection-workflow.ts:366-380; src/lib/deepseek-ranker.ts:91-119 | Primary design/code | 2026-07-31 | Existing single-flight behavior and epoch invalidation | VERIFIED |
| 6 | Task 5 hard-filters the current top-five candidates and therefore does not perform the requested upstream book boost or recover an absent Exodus candidate. | Plan Task 5:854-877, 890-924; src/lib/deepseek-ranker.ts:44-45; src-tauri/crates/detection/src/direct/books.rs:9-22; src-tauri/crates/detection/src/pipeline.rs:292-294 | Primary design/code | 2026-07-31 | Candidate cap and reusable book data confirmed in code | VERIFIED |
| 7 | Task 5 can send a one-candidate model request because it checks the two-candidate gate before applying its book filter. | Plan Task 5:915-924; src/lib/deepseek-ranker.ts:71 | Primary design/code | 2026-07-31 | Plan's Exodus test explicitly expects a one-candidate request | VERIFIED |
| 8 | The cache key is ordered although the stated cache contract is keyed by candidate-id set. | Plan Task 2:386-400; src/lib/deepseek-ranker.ts:44-45 | Primary design/code | 2026-07-31 | Candidate rank order can vary with confidence | VERIFIED |
| 9 | The live metrics in the plan are not independently verifiable from this workspace because their only cited source is outside it. | Plan:11-25; workspace log search in RESEARCH_LOG search trail | Local artifact | 2026-07-31 | None available within granted filesystem scope | UNVERIFIED |

## Search trail

| Query / tactic | Where | Result |
|----------------|-------|--------|
| Read full plan and extract tasks | docs/superpowers/plans/2026-07-31-ai-ranking-quality.md | Plan claims and implementation details collected. |
| Trace direct/semantic scheduling and gate | src-tauri/src/commands/stt/*.rs; src/lib/deepseek-ranker.ts; src/lib/verse-detection-workflow.ts | Independent workers/emits and batch-local gate confirmed. |
| Inspect workspace runtime logs for the cited service test | debug.log; dev-server.out.log; Vite logs | No cited AppData service log exists in the workspace. |
| Inspect direct book data and retrieval cap | direct/books.rs; pipeline.rs; deepseek-ranker.ts | The direct book catalog includes Exodus aliases; ranking candidates are capped to five before Task 5's proposed book filtering. |
| Hunt disconfirming cases for debounce and book scoping | Plan Task 3 and Task 5 pseudocode versus existing single-flight gate | Found the in-flight replacement and one-candidate-request cases recorded in claims 5 and 7. |
```
