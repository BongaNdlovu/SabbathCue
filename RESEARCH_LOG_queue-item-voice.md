# Research Log - Queued Item Voice Command Investigation

## Question definition

```
QUESTION(S):          Why did queued-item voice commands work intermittently in the latest live test, and which local workflow stage is responsible?
SUFFICIENT ANSWER:    Correlate working and failing log events with source-code routing/state transitions, identify a verified root cause or document the missing evidence needed to reproduce.
FRESHNESS REQUIRED:   Most recent runtime log from the user-described live test on 2026-07-31.
KNOWN UNKNOWNS:       Exact utterances, queue contents, STT provider, and runtime log location are not yet confirmed.
OUT OF SCOPE:         Modifying code until the debugging protocol has a reliable reproduction and confirmed cause.
DATE STARTED:         2026-07-31
```

## Claims ledger

| # | Claim | Source (path + locator) | Tier | Info date | Corroborated by | Grade |
|---|-------|-------------------------|------|-----------|-----------------|-------|
| 1 | An immediate stop/start on 2026-07-31 left two audio fanout threads alive until the next stop. | Runtime log lines 12965–13274, 21620–21623 | Primary runtime log | 2026-07-31 | `stt/mod.rs:128–264` has one shutdown log per fanout thread | Verified |
| 2 | The same interval recorded 8,226 failed audio sends, from 11:17:31 through 11:18:54. | Runtime log, `rg` count and timestamps | Primary runtime log | 2026-07-31 | `stt/mod.rs:242–249` identifies the warning source | Verified |
| 3 | A repeated `item 2` after a new partial is blocked before frontend queue control. | Temporary red Rust regression probe, command output | Primary executable test | 2026-07-31 | `transcript_router.rs:67–123`; `stt/mod.rs:472–496`; `use-transcription.ts:251–272` | Verified |
| 4 | A same-item command inside five seconds is intentionally not re-presented by the frontend. | `queue-voice-control.ts:43–52` | Primary source | 2026-07-31 | `queue-voice-control.test.ts:162–168`; 19-test Vitest run | Verified |
| 5 | The current runtime log cannot attribute individual command misses to one of the three suppression paths. | Runtime log had 0 `[ROUTER]` entries | Primary runtime log | 2026-07-31 | `stt/mod.rs:479–481`; `workflow-trace.ts:77–101` | Verified |

## Search trail

| Query / tactic | Where | Result |
|----------------|-------|--------|
| Locate latest desktop log | `%LOCALAPPDATA%\\com.bongandlovu.sabbathcue.personal\\logs` | `SabbathCue Personal.log`, last write 2026-07-31 13:29:35 |
| Count router and audio-failure entries | Latest runtime log | 0 router entries; 8,226 dropped-frame warnings from 11:17:31–11:18:54 |
| Trace STT lifecycle | Runtime log + `src-tauri/src/commands/stt/mod.rs` | Immediate restart reused session flag; two fanout shutdowns logged later |
| Trace queue-final routing | `transcript_router.rs` → `stt/mod.rs` → `use-transcription.ts` | Queue handling depends on `emit_transcript`; generic duplicate dedupe blocks repeated item command |
| Reproduce repeated command route | Temporary Rust red probe | Fails on current code; probe removed after recording output |
| Verify frontend guard contract | `queue-voice-control` test | 19/19 passed; same item within 5 seconds intentionally no-ops presentation |
