# Debug Evidence Log - Queued Item Voice Commands

## Bug definition (step 1)

```
BUG / TICKET:       queued-item-voice-command-intermittent
SYMPTOM (exact):    During the latest live test, voice commands for items in the queue worked intermittently.
EXPECTED:           A recognized supported queue-item command consistently presents the intended queued item.
ACTUAL:             Some utterances take effect and some do not; exact transcript, queue state, and routing outcome are not yet captured.
DELTA:              The command workflow does not consistently reach the expected queued-item action.
REPRO STATUS:       CONFIRMED for repeated queue-item finals: a second `item 2` final after a new partial is suppressed before the frontend.
ENVIRONMENT:        Windows desktop application; live STT and queued-item voice presentation.
FIRST OBSERVED:     User report, 2026-07-31.
LAST KNOWN GOOD:    Unknown.
RECENT CHANGES:     AI-ranking quality work completed in this workspace; command workflow has not been modified in that work.
IN SCOPE (may modify): Queue-item voice-command recognition, routing, state transition, regression tests, and observability required to reproduce.
OUT OF SCOPE:       AI-ranking behavior, unrelated queue presentation behavior, and external STT provider changes.
DEFINITION OF FIXED: A captured failing utterance has a deterministic regression test that fails before the minimal root-cause fix, passes after it, and remains stable across repeated runs.
```

## Evidence log (append-only)

### E1 — Live STT restart left two audio fanout threads (2026-07-31)

- **Source:** `C:\Users\fanel\AppData\Local\com.bongandlovu.sabbathcue.personal\logs\SabbathCue Personal.log`, lines 12965–13274 and 21620–21623.
- **Observed sequence:** first audio capture starts at 11:16:54; stop is requested at 11:17:31; another capture starts in that same second; the replacement provider connects at 11:17:32; the next stop at 11:18:54 logs *two* `Audio capture stopped on fanout thread` events.
- **Interpretation:** each fanout thread emits that shutdown message once (`src-tauri/src/commands/stt/mod.rs:263`), so the log establishes two concurrently surviving fanout threads in that interval.

### E2 — The stale fanout produced 8,226 failed sends (2026-07-31)

- **Source:** same runtime log; exact count obtained with `rg 'Dropped STT frame: provider queue full'`.
- **Observed:** 8,226 warnings, from 11:17:31 through 11:18:54; steady-state counts are 100 per second from 11:17:32 through 11:18:53.
- **Source behavior:** the warning is emitted for *any* `send_timeout` error at `src-tauri/src/commands/stt/mod.rs:242–249`, including a disconnected receiver as well as a full queue.
- **Mechanism:** the fanout thread captures the session-wide `stt_active` flag (`mod.rs:125`) and only exits after observing it false (`mod.rs:142–143`, `194–196`). `stop_transcription` clears that shared flag and aborts Tokio task handles but neither owns nor joins the native fanout thread (`mod.rs:699–715`). A new start flips the same flag true (`mod.rs:74–84`) before the old thread has necessarily observed false, allowing the old fanout to continue sending to its now-aborted provider channel.

### E3 — Repeated queue command final is suppressed despite a new utterance (2026-07-31)

- **Source:** temporary regression probe in `src-tauri/src/commands/transcript_router.rs`, removed immediately after execution to leave no product change.
- **Probe:** route final `item 2`; route partial `item`; route final `item 2`; assert that the second final emits to the frontend.
- **Command:** `cargo test -p sabbathcue repeated_queue_item_command_after_a_new_partial_reaches_the_frontend --lib`.
- **Observed:** one test ran and failed at the assertion, with `a second queue command must reach frontend queue control`.
- **Mechanism:** every partial records `saw_partial_since_final` (`transcript_router.rs:67–76`), but only verse-navigation commands use it to bypass duplicate suppression (`100–114`). Queue-item finals fall through to the 12-entry exact-text dedupe (`116–123`), returning `emit_transcript: false`. The event router only emits the frontend `transcript_final` when that flag is true (`src-tauri/src/commands/stt/mod.rs:472–496`), while frontend queue control is invoked only from that event (`src/hooks/use-transcription.ts:251–272`).

### E4 — Frontend intentionally suppresses same-item presentation for five seconds (2026-07-31)

- **Source:** `src/services/queue/queue-voice-control.ts:43–52` returns success without calling `presentQueuedItem` when the same item ID was handled in the previous 5,000 ms.
- **Verification:** `npm.cmd run test:unit -- src/services/queue/queue-voice-control.test.ts` passed: 19 tests. Its `suppresses duplicate provider finals inside the guard window` case asserts that `item 2` followed by `item number two` one second later presents only once (`queue-voice-control.test.ts:162–168`).
- **Impact:** a real operator retry of the same item within five seconds is intentionally ignored at the presentation layer, even when the final reaches the frontend.

### E5 — Attribution limit in the current log (2026-07-31)

- **Source:** runtime log search found zero `[ROUTER]` entries; router suppression is logged at debug level (`src-tauri/src/commands/stt/mod.rs:479–481`). Final pipeline info logs only provider, confidence, character count, and timing (`514–520`), not text or queue-control outcome.
- **Consequence:** this log proves the restart/audio fault occurred during the live test, but cannot associate a particular spoken queue command with router suppression, the frontend five-second guard, a parser rejection, or lost input. Renderer workflow traces contain transcript text in memory only (`src/lib/workflow-trace.ts:77–101`); they are not persisted to the desktop log.
