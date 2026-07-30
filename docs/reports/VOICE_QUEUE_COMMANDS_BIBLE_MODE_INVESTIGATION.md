# Voice Queue Commands and Bible Mode

Researched: 2026-07-30 · Shelf life: re-check after changes to the STT loop,
queue presentation workflow, or detection settings contract.

## Implementation outcome — completed 2026-07-30

The recommended design below is now implemented.

- Final transcripts accept strict, whole-utterance queue commands including
  `"item 1"`, `"item number 2"`, number words, and explicit
  show/present/display/go-to variants.
- Commands resolve the current one-based queue order, set the item active, and use
  the shared presentation path for scripture, hymn, media, slide deck, EGW, and
  video items.
- The persisted **Bible mode** switch defaults ON. OFF disables live Bible direct,
  semantic, and reading-mode behavior while transcription, queue/slide/hymn
  commands, manually presented scripture, queued scripture, and EGW detection
  remain available.
- Semantic controls are disabled visually while Bible mode is OFF without erasing
  their saved preference. Pause Suggestions remains the separate pause-all control.

Verification receipts:

```text
Focused frontend: 8 files, 104 tests passed
TypeScript: tsc -b passed
Rust focused: queue-command filter 5 passed; Bible-mode policy 4 passed
Frontend regression (external Paddle live-API test excluded): 180 files,
  1188 passed, 1 skipped
Rust workspace: cargo test --workspace passed
Lint: 0 errors (4 pre-existing complexity warnings)
Production build: passed
Rust clippy: --workspace --all-targets -D warnings passed
```

The Paddle live-API test was not run to completion because the managed environment
blocked credentialed outbound access with `connect EACCES`; its four local tests
passed and the failure occurred before a Paddle response. No Paddle code was changed.

## 1 · Direct answer

Both ideas are feasible and fit the current architecture.

- **Queue voice commands:** High confidence. Final transcripts already feed
  deterministic slide and hymn command handlers, and every queue entry shares one
  presentation path. A strict `"item 1"` / `"item number 2"` handler can resolve the
  current one-based queue position and present that item regardless of its kind.
- **Bible mode:** High confidence, with one important design distinction. The existing
  “Pause Suggestions” backend control already proves transcription can continue while
  live direct and semantic work is stopped, but it also stops EGW detection. A true
  Bible-only mode should therefore be a separate master flag, not merely a rename.

The recommended behavior is: **Bible mode ON** keeps today's direct and semantic Bible
detection; **Bible mode OFF** stops new Bible detections and Bible reading-mode
advances while transcription, queue commands, slide commands, hymn commands, manual
queue presentation, and—if desired—EGW detection continue.

Confidence: **HIGH** for technical feasibility; **MEDIUM** for final command wording
until it is exercised against held-out real church audio from every supported STT
provider.

## 2 · Key findings

### FACT — the app already has the right voice-command entry point

The Rust STT loop emits a final transcript immediately, before starting detection work
(`src-tauri/src/commands/stt/mod.rs:481-511`). The React bridge then stores that final
segment before trying the sermon-slide and hymn command handlers
(`src/hooks/use-transcription.ts:247-274`). The bridge is mounted at app level, not in
the Queue workspace (`src/App.tsx:148-151`), so a queue command can work from any
workspace.

The existing sermon-slide parser recognizes deterministic phrases and presents a
requested slide live (`src/services/slides/sermon-slide-voice-control.ts:11-30,39-62`).
Its current tests verify next, previous, numbered, and out-of-range behavior
(`src/services/slides/sermon-slide-voice-control.test.ts:71-100`).

Grade: **VERIFIED**.

### FACT — “any item in the queue” can use one implementation

`QueueItem.presentation` is the common presentation union
(`src/types/queue.ts:8-37`). That union currently includes scripture, hymn, media,
slide deck, EGW, and video (`src/types/presentation.ts:6-12,155-161`).
`presentQueuedItem` restores any saved deck context and sends the item through the
common live workflow (`src/lib/queue-presentation.ts:13-16`). The Queue UI already
maps display number `index + 1` to `setActive(index)` plus `presentQueuedItem(item)`
(`src/components/queue/QueueSorterCard.tsx:131-139`).

**INFERENCE:** a queue voice handler does not need type-specific branches. It should
read the latest queue state at command time, convert the spoken one-based number to a
zero-based index, set it active, and call `presentQueuedItem`.

Grade: **VERIFIED** facts; **HIGH-confidence inference**.

### FACT — transcription and live detection are already separable

The backend emits transcript partials before checking whether detection is paused
(`src-tauri/src/commands/stt/mod.rs:398-419`) and emits finals before that same check
(`src-tauri/src/commands/stt/mod.rs:481-511`). When paused, it skips direct and
semantic job scheduling but does not stop the STT provider
(`src-tauri/src/commands/stt/mod.rs:419-457,503-617,634-644`).

The pause flag and STT-active flag are separate atomics
(`src-tauri/src/state.rs:9-12`), and the pause command only changes
`detection_paused` (`src-tauri/src/commands/detection.rs:293-319`).

**INFERENCE:** a Bible-mode flag can follow the same separation and will not damage
transcription if it gates detection jobs/results rather than STT lifecycle code.

Grade: **VERIFIED** facts; **HIGH-confidence inference**.

### FACT — the existing pause is not a precise Bible-only switch

The direct live worker falls back to explicit EGW detection and can append EGW results
to Bible results (`src-tauri/src/commands/stt/live_session.rs:239-240,318-325`).
The semantic worker also handles explicit EGW windows and verified EGW quotations
(`src-tauri/src/commands/stt/live_session.rs:395-417,499-562`).

**INFERENCE:** simply relabeling “Pause Suggestions” as “Bible mode” would be
misleading if EGW is expected to continue. A dedicated `bibleDetectionEnabled` flag
is the cleaner product contract. If the intended meaning is “pause every automatic
detection,” the current backend control can instead be reused with a small UI change.

Grade: **VERIFIED** facts; **HIGH-confidence inference**.

### FACT — the learned command classifier should not execute this feature yet

The MiniLM command experiment is deliberately not registered with Tauri and cannot
execute operator actions (`docs/minilm-command-benchmark.md:5-9`). Its synthetic data
does not model real microphones, accents, congregations, or STT-provider behavior
(`docs/minilm-command-benchmark.md:39-43`), and the repository keeps it disconnected
until held-out multi-church evaluation passes
(`docs/minilm-command-benchmark.md:83-84`).

**INFERENCE:** version one should use a narrow deterministic grammar on final
transcripts. The learned classifier can remain shadow-only until real-audio evidence
supports activating it.

Grade: **VERIFIED** facts; **HIGH-confidence inference**.

## 3 · Disputed and conflicting points

There is one product ambiguity, not a code conflict:

- If “Bible mode OFF” means **no automatic suggestions of any kind**, reuse the
  existing `detection_paused` mechanism and rename/reframe the UI.
- If it means **no Bible direct/semantic detections while other features continue**,
  use the dedicated flag recommended below.

This report plans for the second, more precise interpretation. It preserves EGW,
hymn/slide/queue commands, and manual presentation.

A second behavior choice is whether `"item 2"` should preview or go live. Existing
numbered sermon-slide commands go live, so the plan assumes queue commands also go
live. A separate `"preview item 2"` grammar can be added if preview-only speech is
desired.

## 4 · Gaps

- No real microphone or held-out multi-church recordings were available in this
  investigation. Parser correctness can be unit-tested, but provider-specific
  recognition of “item one” versus “item 1” needs audio/transcript fixtures.
- The desired scope of Bible mode with respect to EGW is not explicitly stated.
  The recommended implementation keeps EGW active because the request names Bible
  direct and semantic detection specifically.
- The desired persistence is not explicit. The plan treats Bible mode as a persisted
  setting, defaulting ON, while retaining previous semantic-toggle preference.

## 5 · Recommended implementation plan

### Phase 1 — lock the product contract

Define these invariants before coding:

1. Queue positions are spoken and shown as one-based numbers.
2. Exact bare forms `"item 1"` and `"item number 1"` are supported, as requested.
3. Also accept number words that STT providers may return, such as `"item one"`.
4. Commands operate only on final transcripts and must match the complete normalized
   utterance; sermon prose containing those words must not fire.
5. A valid queue command presents live, sets `activeIndex`, and restores deck context.
6. Empty/out-of-range commands do nothing destructive and show a concise operator
   notification.
7. Bible mode defaults ON. Turning it OFF stops new Bible direct, semantic, and
   reading-mode output but does not erase prior detections or block manually queued
   scripture.
8. EGW stays active; “Pause Suggestions” remains the separate pause-all control.

### Phase 2 — add queue voice control

Add a small queue command service beside the existing slide/hymn services:

- `parseQueueItemCommand(text)` returns a positive item number or `null`.
- Use a strict anchored grammar for `"item N"`, `"item number N"`, and optionally
  explicit verbs such as `"show item N"` / `"present item N"`.
- Reuse or extract the hymn handler's spoken-number parser so digit and number-word
  behavior remains consistent.
- `handleQueueItemVoiceControl(text)` reads `useQueueStore.getState()` at execution
  time, bounds-checks the current order, sets the zero-based active index, and calls
  `presentQueuedItem`.
- Add a short duplicate-final guard, keyed by queue item ID, because providers can
  repeat the same final.
- Invoke it in `handleTranscriptFinalPayload` after the segment is stored and before
  the expensive/lazy hymn path.

Also extend the backend command-utterance predicate used by semantic enqueue filtering
so queue commands do not become Bible suggestion noise
(`src-tauri/src/commands/stt/detection_jobs.rs:57-84,111-129`). Keep the TypeScript and
Rust command grammars covered by the same fixture table to prevent drift.

### Phase 3 — add a true Bible-mode flag

Follow the existing persisted semantic-setting pattern
(`src/stores/settings-store.ts:19-28,102-116`;
`src/hooks/use-detection-settings-sync.ts:7-34`):

- Add persisted `bibleDetectionEnabled`, default `true`.
- Add it to the detection-settings IPC payload and backend `AppState` as a separate
  atomic.
- Expose a master “Bible mode” switch. When OFF, disable the semantic controls
  visually without overwriting their saved values.
- Keep the current pause-all state and UI behavior separate.

Backend gating needs care because EGW shares both live workers:

- Direct worker: when Bible mode is OFF, skip `DirectDetector::detect` and Bible
  reading candidates, but still run explicit EGW detection.
- Semantic worker: retain explicit/quote EGW work, but skip Bible FTS/vector
  resolution and Bible emissions.
- Reading mode: deactivate it when Bible mode turns OFF and skip future Bible
  reading-mode checks until re-enabled.
- In-flight work: re-check the Bible flag immediately before emitting results so a
  job started before the toggle cannot leak a Bible detection afterward.
- Manual presentation/search and existing queued scripture remain available.

This is the main reason the recommended Bible-only implementation is a medium-sized
change rather than a label swap.

### Phase 4 — tests and acceptance evidence

#### Queue parser and handler tests

- Accept: `"item 1"`, `"ITEM NUMBER 2"`, `"show item one"`, harmless surrounding
  punctuation, and supported spoken-number variants.
- Reject: zero, negatives, decimals, missing numbers, out-of-range numbers, and longer
  sermon sentences such as “item one in our discussion is faith.”
- Verify one-based-to-zero-based mapping after queue reorder.
- Verify empty and out-of-range queues leave active/live state unchanged.
- Verify duplicate finals within the guard window do not present twice.
- Table-test scripture, hymn, media, slide deck, EGW, and video queue entries through
  the common handler.
- Verify a queued deck restores its navigation state before going live.

#### Transcript integration tests

- A command final remains in transcript history before the queue action fires.
- Non-command finals still reach hymn handling.
- Queue commands work when the Queue workspace is not selected.
- Bible mode OFF does not block final/partial transcript storage or queue commands.

#### Bible-mode frontend tests

- Default is ON and persisted hydration/save round-trips it.
- Settings sync includes the master flag.
- The UI announces ON/OFF correctly and disables only subordinate semantic controls.
- A failed backend sync reports an output issue and does not silently claim success.

#### Bible-mode Rust tests

- OFF suppresses direct Bible results.
- OFF suppresses semantic Bible results and avoids Bible embedding/search work.
- OFF deactivates and blocks reading-mode advances.
- OFF still allows explicit and quote-based EGW results.
- A Bible job already in flight cannot emit after the toggle changes to OFF.
- Re-enabling restores direct/semantic behavior with the previous semantic preference.
- Existing pause-all still suppresses both Bible and EGW.

#### Regression and end-to-end gates

Run:

```powershell
npm.cmd run typecheck
npm.cmd run test:unit
npm.cmd run test:semantic-detections
cargo test --manifest-path src-tauri/Cargo.toml --workspace
npm.cmd run build
```

Add an operator-flow test that seeds a mixed queue, reorders it, submits `"item 2"`,
and asserts the displayed live reference and `activeIndex`. Add a second flow that
turns Bible mode OFF, feeds a final `"John 3:16"` transcript, confirms the transcript
is visible with no Bible detection, then feeds `"item 1"` and confirms the queued item
goes live.

Before enabling broader/natural-language variants, replay held-out recordings from
each supported provider and require:

- 100% execution of the narrow authored command set,
- zero actions on the safety/sermon-prose set,
- correct bounds behavior under queue mutation,
- no duplicate action from repeated provider finals.

## 6 · Evidence from this investigation

Executed on 2026-07-30:

```powershell
npm.cmd run test:unit -- `
  src/services/slides/sermon-slide-voice-control.test.ts `
  src/services/hymnal/hymn-voice-control.test.ts `
  src/hooks/use-transcription.test.ts `
  src/stores/queue-store.test.ts `
  src/lib/queue-presentation.test.ts `
  src/lib/presentation-queue-walk.test.ts `
  src/components/queue/QueueWorkspace.test.tsx `
  src/components/panels/queue-panel.test.tsx `
  src/hooks/use-detection-settings-sync.test.ts `
  src/components/layout/operator-status-strip.test.tsx
```

Result: **10 test files passed; 96 tests passed**.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml `
  -p sabbathcue test_detection_paused_state --lib
```

Result: **1 passed; 0 failed**.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml `
  -p rhema-detection command --lib
```

Result: **60 passed; 0 failed**.

These are baseline tests, not proof of the unimplemented features. They verify that
the proposed seams—queue presentation, transcript ordering, deterministic command
handling, settings synchronization, and detection pause state—are healthy before work
starts.

## 7 · Sources

All sources are repository-primary and were opened on 2026-07-30:

1. `src/hooks/use-transcription.ts` — final transcript storage and frontend commands.
2. `src-tauri/src/commands/stt/mod.rs` — STT emission and live job scheduling.
3. `src/services/slides/sermon-slide-voice-control.ts` and tests — existing command
   precedent.
4. `src/types/queue.ts`, `src/types/presentation.ts`,
   `src/lib/queue-presentation.ts` — common queue presentation contract.
5. `src-tauri/src/commands/detection.rs`, `src-tauri/src/state.rs` — pause/settings
   state.
6. `src-tauri/src/commands/stt/live_session.rs` — Bible/EGW worker interleaving.
7. `src/stores/settings-store.ts`,
   `src/hooks/use-detection-settings-sync.ts` and tests — persistence/sync pattern.
8. `docs/minilm-command-benchmark.md` — classifier status, evidence limits, and
   activation gate.
