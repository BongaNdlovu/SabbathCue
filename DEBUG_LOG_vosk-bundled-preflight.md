# Evidence Log — vosk bundled_worker_preflight_reports_ready

Copied from `.agents/skills/debugging/references/evidence-log.md`. Append-only.

---

## Bug definition (step 1)

```
BUG / TICKET:       vosk-bundled-preflight-reports-ready
SYMPTOM (exact):    vosk::tests::bundled_worker_preflight_reports_ready panicked at crates\stt\src\vosk.rs:451:14:
                    bundled Vosk worker/model preflight should report ready: ConnectionFailed("Vosk worker exited without reporting ready")
                    test result: FAILED. 43 passed; 1 failed; 1 ignored
EXPECTED:           check_ready() returns Ok(()) because the bundled vosk_worker.exe emits a Ready event
ACTUAL:             check_ready() returns ConnectionFailed("Vosk worker exited without reporting ready")
DELTA:              worker stdout never yielded WorkerEvent::Ready before the process/reader ended
REPRO STATUS:       NOT YET (reproducing locally next)
ENVIRONMENT:        Windows (user_info); user paste looks like CI (Error: Process completed with exit code 1)
FIRST OBSERVED:     user-reported cargo test failure for -p rhema-stt --lib
LAST KNOWN GOOD:    docs/detection-baselines/2026-08-01-cargo-test-workspace.txt shows this test ok
RECENT CHANGES:     TBD
IN SCOPE (may modify):  TBD until root cause found
OUT OF SCOPE:           unrelated crates, UI, unrelated STT providers
DEFINITION OF FIXED:    original test passes; root cause confirmed with evidence; no test weakening
```

## B.1 · Reproduction (step 2)

```
STEPS / SCRIPT:
1. Local cargo test with existing June-15 sidecar (PyInstaller 6.21.0-era):
   cargo test -p rhema-stt --lib vosk::tests::bundled_worker_preflight_reports_ready -- --nocapture
   RESULT: ok in 2.05s

2. Rebuild sidecar with PyInstaller 6.22.1 (what CI installed 2026-08-17):
   pip install pyinstaller==6.22.1
   PyInstaller --onefile --collect-all vosk ...
   cargo test -p rhema-stt --lib vosk::tests::bundled_worker_preflight_reports_ready -- --nocapture

FAILURE OUTPUT (verbatim, full stack trace):
thread 'vosk::tests::bundled_worker_preflight_reports_ready' (24004) panicked at crates\stt\src\vosk.rs:451:14:
bundled Vosk worker/model preflight should report ready: ConnectionFailed("Vosk worker exited without reporting ready")
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 44 filtered out; finished in 1.02s

CI (run 32050440304, rust job) same assertion, suite finished in 1.53s.
Successfully installed ... pyinstaller-6.22.1 ... vosk-0.3.41

RELIABILITY: deterministic 2/2 after 6.22.1 rebuild; 0/1 with pre-6.22 sidecar
```

## B.2 · Evidence captured (step 3)

```
FULL STACK TRACE: panic at vosk.rs:451 expect(check_ready)
OBSERVED STATE AT FAILURE (probe in check_ready, later reverted):
DEBUG preflight cwd=...src-tauri\crates\stt
model=...\crates\stt\..\..\..\models\vosk\...
worker=...\crates\stt\..\..\..\sidecars\vosk_worker.exe
DEBUG preflight stdout closed
DEBUG preflight event channel disconnected after 1288ms
DEBUG preflight no-ready before terminate status=None
stderr=
[PYI-31896:ERROR] Security validation failure: parent process has different executable!

DIFFERENTIAL (failing vs working case):
- Last green CI 2026-08-05 used PyInstaller <= 6.21.0 (6.22.0 released 2026-08-08, 6.22.1 2026-08-15)
- Unnormalized worker path with `..` fails; Resolve-Path canonical path succeeds
```

## B.3 · Hypothesis log (step 4)

```
H1: Missing/broken sidecar or model on CI
    Predicts: test skip or model-not-found
    Result: ELIMINATED — CI built sidecar and prepared model; test ran

H2: Full grammar crashes Vosk
    Predicts: manual spawn with verse_only grammar fails
    Result: ELIMINATED — ready in 4699ms

H3: CREATE_NO_WINDOW + Stdio::null() breaks 6.22.1
    Predicts: rust spawn with those flags fails even with small grammar
    Result: ELIMINATED — ready in 2384ms with small grammar

H4: PyInstaller 6.22.1 onefile parent-exe check fails on unnormalized `..` path
    Predicts: same exe + unnormalized path prints PYI security error; canonical path emits ready
    Result: CONFIRMED
      UNNORMALIZED: [PYI-2320:ERROR] Security validation failure: parent process has different executable!
      CANONICAL: {"type": "ready"} exit 0
```

## B.4 · Isolation / bisection log (step 5)

```
EXPERIMENT 1 — rebuild sidecar 6.21.0 -> 6.22.1 only
  OBSERVED: cargo test fails with original CI error in 1.02s
  VERDICT: confirms PyInstaller 6.22.1 as the new variable

EXPERIMENT 2 — spawn 6.22.1 exe from PowerShell with piped stdio
  OBSERVED: {"type":"ready"}
  VERDICT: exe itself is not broken

EXPERIMENT 3 — rust spawn, small vs full grammar
  OBSERVED: both emit ready
  VERDICT: not grammar

EXPERIMENT 4 — unnormalized vs canonical worker path (one variable)
  OBSERVED: unnormalized PYI security failure; canonical ready
  VERDICT: confirms H4
  PROBE REVERTED: yes (eprintln probes removed)

NARROWED TO: worker_command() launching Path with `..` segments;
PyInstaller 6.22.1 bootloader parent-exe path comparison
```

## B.5 · Root cause (step 6)

```
ROOT CAUSE (one sentence): PyInstaller 6.22.1 onefile exits with
"parent process has different executable" when spawned via a path
that still contains `..` (CARGO_MANIFEST_DIR/../../../sidecars/vosk_worker.exe
and src-tauri/../sidecars/vosk_worker.exe), so stdout closes before Ready.

WHY CHAIN: test/app builds dotted path -> Command::new(dotted) ->
onefile parent path != child path after bootloader re-exec ->
security check fails -> no JSON ready -> check_ready disconnect path

CAUSE→SYMPTOM EVIDENCE: probe stderr + unnormalized-vs-canonical spawn
```

## B.8 · Regression test (step 7 → 8)

```
TEST CODE: existing vosk::tests::bundled_worker_preflight_reports_ready
plus worker::tests::worker_command_resolves_parent_dir_segments_in_exe_path

RED  (before fix, 6.22.1 sidecar):
bundled Vosk worker/model preflight should report ready: ConnectionFailed("Vosk worker exited without reporting ready")
FAILED in 1.02s

GREEN (after fix, same 6.22.1 sidecar):
test vosk::tests::bundled_worker_preflight_reports_ready ... ok
test worker::tests::worker_command_resolves_parent_dir_segments_in_exe_path ... ok
rhema-stt lib: 45 passed; 0 failed; 1 ignored
```

## B.6 · Fix diff (step 8)

```
worker.rs: canonicalize + strip \\?\ before Command::new
worker.rs: include stderr on the no-ready error path
worker.rs: unit test that `..` is resolved out of an existing exe path
```

## B.7 · Verification output (step 8)

```
ORIGINAL REPRO RE-RUN:
test vosk::tests::bundled_worker_preflight_reports_ready ... ok
finished in 2.63s

FULL SUITE (rhema-stt --lib):
test result: ok. 45 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.01s

TYPE-CHECK / LINT:
cargo clippy -p rhema-stt --lib --all-targets -- -D warnings
Finished `dev` profile (clean after map_or_else clippy fix)

SIBLING GREP: vosk_worker / worker_command / join("..")
  - vosk.rs project_root() still uses ../../../ — handled at spawn
  - asset_paths::dev_root() uses CARGO_MANIFEST_DIR/.. — also handled at spawn
  - no other Command::new of vosk_worker.exe

INTERMITTENT: n/a (deterministic)
```

## B.9 · Root-cause writeup (step 9)

```
ROOT-CAUSE WRITEUP — vosk-bundled-preflight-reports-ready — 2026-08-17
1. SYMPTOM: bundled_worker_preflight_reports_ready failed on CI and locally
   after a PyInstaller 6.22.1 sidecar build: worker exited without ready.
2. ROOT CAUSE: 6.22.1 onefile security check requires parent exe path ==
   child exe path. Launching via a path that still contains `..` fails that
   check. CI started installing 6.22.1 after 2026-08-15; last green was 2026-08-05.
3. MECHANISM: bootloader prints PYI security error on stderr and exits;
   check_ready saw stdout close and reported a generic disconnect error.
4. HOW IT WAS FOUND: rebuild 6.22.1 reproduced CI; probe captured PYI
   stderr; unnormalized vs canonical spawn isolated the path.
5. THE FIX: canonicalize the worker path (and strip \\?\) in worker_command
   before spawn. Same spawn site the live app uses.
6. VERIFICATION: original test red→green on the 6.22.1 sidecar; new unit
   test; full rhema-stt lib suite green; clippy -D warnings clean.
7. PREVENTION: unit test locks the no-`..` spawn path; no-ready errors now
   include stderr so the next bootloader failure is not silent.
```

## Sign-off

```
Root cause confirmed with evidence:        YES
Symptom-masking introduced:                NONE
Regression test (red → green) attached:    YES
Original reproduction now passes:          YES
Definition of fixed verified:              YES
```
