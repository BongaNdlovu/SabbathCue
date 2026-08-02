# Subscription admin renewal, device approval, suspension, and trial workflow fix plan

This plan is written for an implementation agent that must not infer missing behavior. Follow the steps in order. Do not edit files outside the strict boundary, do not modify an already-applied migration, and do not combine renewal with automatic device approval.

## Plan-generation record

### TRIAGE

- Scope Category: System Bug
- Files Touched: 7 implementation files
- Layers Touched: UI, Backend, Test, documentation
- External Effects: No — the code change itself does not send payments, webhooks, messages, or emails; production deployment is a separately approved operational step.
- Reversibility: Partially — code and the database function can be restored, but access extensions granted while the new function is active cannot be distinguished automatically without an audit record.
- Pipeline Selected: Full
- Justification: The defect crosses the admin UI, a Supabase RPC/database function, device-registration state, the verification UI, and persistent integration tests.

### INTERFACE MAP

- UI ENTRY POINTS:
  - `AdminAccountsPanel` in `src/components/settings/sections/AccountSection.tsx`; its 30-day and one-year buttons call `handleExtendAccess()`.
  - `AdminDeviceManager` in the same file; it lists and approves a named pending device independently of renewal.
  - `VerificationScreen` in `src/components/verification/VerificationScreen.tsx`; it renders `device_pending` and decides when to show Retry.
- API CONTRACTS:
  - `admin_set_access(p_user_id uuid, p_days integer) RETURNS void`; keep its name, arguments, return type, authorization error, and positive-day validation unchanged.
  - `admin_list_devices(p_user_id uuid)` and `admin_set_device_status(p_user_id uuid, p_device_id text, p_status text)`; use them unchanged.
  - Device activation response codes `trial_expired`, `device_pending`, `device_revoked`, `device_limit_reached`, `suspended`, and `ok`; do not rename or merge them.
- BACKEND BOUNDARIES:
  - `account_flags.access_expires_at` is the effective account access timestamp.
  - `account_flags.paddle_access_expires_at` records the last Paddle-owned access timestamp and must remain untouched by a manual admin grant.
  - `devices.status` remains independent from access and suspension.
  - `register_device_verified()` checks suspension, then access expiry, then device status.
- Inbound:
  - An authenticated app administrator invokes the access RPC from the settings screen.
  - A signed-in recipient computer retries verification through the saved Supabase refresh token.
- Outbound:
  - The admin UI refreshes the account list and optionally reads device rows to give accurate follow-up guidance.
  - The recipient refresh calls the existing device-activation Edge Function and receives the next gate result.

### STRICT BOUNDARY

- MODIFIABLE FILES:
  - `docs/CODEBASE.md`
  - `src/components/settings/sections/AccountSection.admin.test.tsx` (new)
  - `src/components/settings/sections/AccountSection.tsx`
  - `src/components/verification/VerificationScreen.device-pending.test.tsx` (new)
  - `src/components/verification/VerificationScreen.tsx`
  - `supabase/migrations/013_additive_admin_access.sql` (new)
  - `supabase/tests/admin_access_workflows.test.sql` (new)
- READ-ONLY DEPENDENCIES:
  - `src/lib/supabase/account.ts`
  - `src/lib/supabase/account.test.ts`
  - `src/lib/supabase/devices.ts`
  - `src/lib/verification/verification-provider.ts`
  - `src/stores/verification-store.ts`
  - `supabase/functions/device-activation/index.ts`
  - `supabase/migrations/002_account_management.sql`
  - `supabase/migrations/006_trial_access.sql`
  - `supabase/migrations/008_church_organization_profiles.sql`
  - `supabase/migrations/009_device_activation_management.sql`
  - `supabase/migrations/010_paddle_billing.sql`
  - `supabase/migrations/012_no_past_due_grace.sql`
  - `supabase/tests/paddle_billing.test.sql`
  - `scripts/run-sql-tests.mjs`
- OUT-OF-BOUNDS:
  - All applied migrations `001` through `012`; add migration `013` instead of rewriting history.
  - Paddle prices, products, checkout, webhook handlers, and subscription status semantics.
  - Device identity/signature code, the Edge Function, the two-approved-device limit, and automatic device approval.
  - Trial length, offline lease duration, account deletion, cancellation, and release infrastructure.

### CHECKPOINT LOG

- Checkpoint: Boundary selection
- Decision: Keep the existing RPC signatures and security gates; add one migration, two focused UI changes, two UI test files, one SQL test file, and the required codebase-map update.
- Continuing to: Failure-mode analysis and the validation-first blueprint.

## Failure-mode analysis

### System assumptions

#### Assumption 1 — Manual duration choices

- Statement: The desktop admin UI intentionally offers only 30 and 365 days.
- Basis: `ACCESS_EXTENSION_OPTIONS` in `AccountSection.tsx` and payload tests in `account.test.ts`.
- Layer: UI

#### Assumption 2 — Existing time must be preserved

- Statement: “Extend” means add time without discarding an unexpired trial, paid period, or previous manual grant.
- Basis: The user-facing label says “Extend”; the current reset-from-now SQL contradicts that label.
- Layer: Backend

#### Assumption 3 — Renewal is not approval

- Statement: Granting access must not approve, revoke, or otherwise mutate any device row.
- Basis: Access and named-device approval are separate RPCs and security gates.
- Layer: API Contract

#### Assumption 4 — A pending computer can retry with its saved session

- Statement: `refreshVerification()` can restore the saved Supabase session and attempt device registration again without collecting the password again.
- Basis: `refreshVerification()` delegates to `loadCachedVerification()`, which reads the saved refresh token before calling registration.
- Layer: UI

#### Assumption 5 — Applied migrations are immutable

- Statement: Migration 006 may be read but not edited; the behavior change must be migration 013 and must survive a repeat apply.
- Basis: `run-sql-tests.mjs` applies migrations in order and repeat-applies the newest file.
- Layer: Backend

#### Assumption 6 — Suspension remains the highest account-level block

- Statement: Renewing a suspended account changes its expiry but does not reinstate it; reinstate remains a separate admin action.
- Basis: `register_device_verified()` checks `suspended` before expiry and `admin_set_suspended()` owns that flag.
- Layer: API Contract

### Failure modes

#### Failure Mode 1 — Logical / Integration: additive extension loses or duplicates time

- Layer: Backend
- Description: An active account is reset to a shorter period, or concurrent updates calculate from a stale timestamp and lose one extension.
- Trigger Condition: The account already has future access, or two access writers touch the same row.
- Blast Radius: Paid/admin-granted days disappear or are counted incorrectly.
- Mitigation: Perform one atomic `INSERT ... ON CONFLICT DO UPDATE` whose update expression uses `GREATEST(existing access_expires_at, now()) + make_interval(days => p_days)` inside PostgreSQL.

#### Failure Mode 2 — Security / Safety: renewal silently approves a computer

- Layer: Cross-layer
- Description: A renewal bypasses installation proof or the two-device limit.
- Trigger Condition: A pending/revoked/third device exists when access is extended.
- Blast Radius: An unauthorized installation receives access.
- Mitigation: Migration 013 updates only `account_flags.access_expires_at`; UI guidance points the administrator to the existing named-device approval control.

#### Failure Mode 3 — UI / UX Contract: successful renewal is presented as a failure

- Layer: UI
- Description: A device lookup fails after the access RPC succeeds, and the UI reports the whole renewal as failed.
- Trigger Condition: `admin_set_access` succeeds but `admin_list_devices` encounters a network/RPC error.
- Blast Radius: The admin retries and accidentally adds another 30/365 days.
- Mitigation: Treat the access result as authoritative; show renewal success even if follow-up device inspection fails, and never call `admin_set_access` again as a retry for device lookup.

#### Failure Mode 4 — UI / UX Contract: pending computer remains stranded after approval

- Layer: UI
- Description: The recipient sees `device_pending` but has no Retry control after the administrator approves it.
- Trigger Condition: The screen is in `status="error"`, `errorCode="device_pending"`.
- Blast Radius: The recipient re-enters credentials unnecessarily or concludes renewal still failed.
- Mitigation: Include only `device_pending` alongside `network` in `authFeedback.canRetry`, reusing the existing `refresh()` callback.

#### Failure Mode 5 — Security / Safety: Retry is exposed for revoked or identity-mismatched devices

- Layer: UI
- Description: A blocked computer is encouraged to retry a state that requires explicit administrative action.
- Trigger Condition: A broad “all device errors can retry” condition is implemented.
- Blast Radius: Confusing retry loops and weakened operator expectations around revocation.
- Mitigation: Add an exact equality check for `device_pending`; tests must assert Retry remains absent for `device_revoked`.

#### Failure Mode 6 — Logical / Integration: Paddle later overwrites a manual extension

- Layer: Backend
- Description: A subsequent subscription event mistakes the manual grant for Paddle-owned access and lowers it.
- Trigger Condition: Migration 013 modifies `paddle_access_expires_at` or makes it equal to the new manual expiry.
- Blast Radius: Manual 30-day or one-year access disappears after a Paddle update/cancel event.
- Mitigation: Update only `access_expires_at`; retain and rerun `admin_grant_survives_subscription_cancel` from `paddle_billing.test.sql`.

#### Failure Mode 7 — Concurrency / Timing: double-click creates two grants

- Layer: UI
- Description: Two renewal calls are sent before the busy state disables the button.
- Trigger Condition: Rapid repeated activation of the same button.
- Blast Radius: The account receives 60 days instead of 30 or 730 instead of 365.
- Mitigation: Preserve the existing per-account busy disable, and make the UI test verify the clicked account’s renewal controls are disabled while the promise is unresolved; do not add retry logic around the mutating RPC.

#### Failure Mode 8 — Memory / Performance: account listing performs an N+1 device query

- Layer: UI
- Description: Every account loads all device rows merely to render the admin list.
- Trigger Condition: Device inspection is moved into initial account rendering.
- Blast Radius: Admin screen latency grows with account count.
- Mitigation: Query `adminListDevices()` only after a successful renewal for the one affected account; do not preload device lists.

#### Failure Mode 9 — Irreversibility: grants issued under additive semantics cannot be auto-undone

- Layer: Backend
- Description: After production deployment, an administrator grants time and the implementation is later rolled back; there is no grant audit table that distinguishes those increments.
- Trigger Condition: Rollback after real renewal actions have occurred.
- Blast Radius: Some accounts retain additional access.
- Mitigation: Take the function-definition snapshot before deployment, restrict the rollout window, record accounts used during smoke tests, and compensate only those known test accounts; do not guess at customer expiry corrections.

### Required-category exclusions

- No injection surface is introduced: `p_days` remains a typed PostgreSQL integer passed through the existing Supabase RPC client, and interval construction uses `make_interval`, not dynamic SQL.
- No new external payment/message side effect is introduced: Paddle code and notification code are out of bounds.

### Assumption–failure cross-reference

- Assumption 1: Manual duration choices → PRESERVED. The UI still sends only 30 or 365 and the RPC signature remains unchanged.
- Assumption 2: Existing time must be preserved → MITIGATED BY Failure Mode 1. The atomic `GREATEST` expression is the implementation requirement and receives SQL regression coverage.
- Assumption 3: Renewal is not approval → MITIGATED BY Failure Mode 2. Tests assert the device stays pending until the named approval RPC runs.
- Assumption 4: Pending computer can retry → MITIGATED BY Failure Modes 4 and 5. Retry is added only for pending and uses the existing refresh path.
- Assumption 5: Applied migrations are immutable → PRESERVED. Only new migration 013 is modifiable, and repeat application is a required database test.
- Assumption 6: Suspension remains highest block → PRESERVED. The migration updates only expiry, and SQL tests cover suspended and reinstated outcomes.

## Adversarial audit record

Attempting to find a failure for item 1: The deployment or rollback instructions could silently modify migration 006 or an unlisted release file. The plan explicitly forbids editing migrations 001–012, uses migration 013, treats production SQL execution as an operational action rather than a repository edit, and confines documentation to `docs/CODEBASE.md`.

- [x] 1. BOUNDARY COMPLIANCE
  - Justification: Every planned repository write appears in the seven-file modifiable list; all other named files are read or executed only.

Attempting to find a failure for item 2: The plan could handle additive dates but omit pending-device recovery, suspension, Paddle preservation, or rapid duplicate actions. Each listed failure mode maps to a concrete code constraint and at least one persistent test or validation step below; device lookup failure is explicitly non-transactional and must not negate renewal success.

- [x] 2. FAILURE MODE COVERAGE
  - Justification: Steps 2–8 cover all nine failure modes, including exact gate sequencing, security-negative tests, atomic SQL, busy-state behavior, and Paddle regression.

Attempting to find a failure for item 3: Making extensions additive could accidentally change trial length, auto-reinstate suspension, or approve devices. The migration changes one column only, while SQL tests assert all three invariants and the UI retains separate controls.

- [x] 3. ASSUMPTION PRESERVATION
  - Justification: Every assumption is either unchanged by boundary or cross-referenced to a tested mitigation.

Attempting to find a failure for item 4: Unit transport tests could pass while the real SQL still resets expiry, or a Retry button could render without invoking refresh. The plan requires behavioral SQL assertions against a real PostgreSQL container and DOM click assertions, not string/transport checks alone.

- [x] 4. STEP VALIDATION COMPLETENESS
  - Justification: Each execution step has an observable exit condition tied to its actual behavior, with RED-before-GREEN checks for the defects.

Attempting to find a failure for item 5: A production rollback could restore the function but leave grants already issued under additive semantics. The rollback route calls this out as compensation, limits correction to known smoke-test accounts, and forbids speculative customer data edits.

- [x] 5. ROLLBACK VIABILITY
  - Justification: Code/UI/function changes are reversible; the only non-automatic state restoration is explicitly classified and bounded.

Attempting to find a failure for item 6: The plan could expand into Paddle billing, trial pricing, device cryptography, or a general AccountSection refactor. Those areas are explicitly out of bounds, and unchanged workflows receive regression tests rather than implementation churn.

- [x] 6. SCOPE DISCIPLINE
  - Justification: Every behavior change directly addresses additive renewal, accurate pending-device guidance, or retry after approval.

Attempting to find a failure for item 7: Test and docs steps might exist while one runtime layer is absent. The blueprint contains backend migration, admin UI, recipient UI, persistent test artifacts, documentation, staging verification, and deployment validation.

- [x] 7. WORKFLOW COMPLETENESS
  - Justification: All declared layers have explicit steps and the path is traced from admin click through database mutation to recipient retry.

Attempting to find a failure for item 8: Changing the RPC return type or device status name would require client and Edge Function changes not in boundary. The plan freezes all names, arguments, return types, and status values; only the meaning of the expiry calculation changes.

- [x] 8. CONTRACT CONSISTENCY
  - Justification: UI inputs remain `p_user_id` plus integer `p_days`; backend still returns void; device APIs and error codes remain unchanged.

[PLAN VERIFIED: SAFE FOR PRESENTATION]

## Final verified plan

### PLAN METADATA

- Rulebook Version: v1.5
- Plan Generated: 2026-08-02T09:12:01+02:00
- Problem Statement: Fix admin renewal so it preserves existing access time and make the separate pending-computer gate clear and retryable, while proving suspension, trial, 30-day, one-year, device-limit, and Paddle behavior remain correct.
- Workflow Scope: UI → Backend → Test → Documentation; API signatures are mapped and intentionally unchanged.
- Boundary Hash: `docs/CODEBASE.md, src/components/settings/sections/AccountSection.admin.test.tsx, src/components/settings/sections/AccountSection.tsx, src/components/verification/VerificationScreen.device-pending.test.tsx, src/components/verification/VerificationScreen.tsx, supabase/migrations/013_additive_admin_access.sql, supabase/tests/admin_access_workflows.test.sql`
- Known Limitations: Database tests require Docker. Production migration state and customer/device rows were unavailable during planning. The repository map says full CI/CD ownership is not mapped, so release/promotion must use the existing operator-approved release procedure. No audit table exists for individual manual grants.

### Non-negotiable product decisions

1. “Add 30 days” means: if access is expired or null, set it to 30 days from database `now()`; if access is still active, add 30 days to the existing expiry.
2. “Add 1 year” means the same rule with exactly 365 days; it is not a calendar-year calculation.
3. Renewing access does not reinstate a suspended account.
4. Renewing access does not approve pending/revoked computers.
5. Only a specifically selected pending computer can be approved, and the two-approved-device limit remains enforced.
6. After approval, a pending computer can press Retry and reuse its saved session.
7. A failed post-renewal device lookup does not turn a completed renewal into an error and does not retry the renewal mutation.

### Definition of done

- An expired account granted 30 days receives an expiry approximately 30 days from database time.
- An account with 20 future days granted 30 receives approximately 50 future days, not 30.
- An account with future access granted 365 preserves its existing remaining time and adds 365 days.
- `suspended`, `paddle_access_expires_at`, and every device row are byte-for-byte/value-for-value unchanged by `admin_set_access` except for unrelated timestamps changed by explicit registration calls in tests.
- The exact expired → renewed → pending → approved → `ok` device sequence passes in PostgreSQL.
- The admin gets explicit pending-device guidance after renewal and can still use Manage computers.
- A waiting recipient sees Retry; clicking it calls the existing verification refresh once.
- Revoked devices do not receive the new Retry behavior.
- Focused tests, SQL tests, typecheck, lint, and the full unit suite pass with no new failures.
- `docs/CODEBASE.md` accurately records the revised admin-access flow and changelog entry.

### ROLLBACK ROUTE

- Precondition: Before implementation, save `git status --short`, the current commit, baseline test output, and the current `pg_get_functiondef('public.admin_set_access(uuid,integer)'::regprocedure)` from the target Supabase environment. Do not deploy with unrelated working-tree edits.
- Trigger: If Step 8 validation fails — specifically, if any additive-date, suspension, pending-device, Paddle-preservation, typecheck, or lint assertion fails — revert the affected code/test changes before Step 9. If Step 10 production smoke validation fails, restore the saved function definition and redeploy the previous UI build before further grants are issued.
- UI: Undo — revert only the two TSX files and their two test files, rebuild, and redeploy the prior compatible desktop/web artifact.
- Contract: No-op — RPC names, arguments, return type, and device response codes do not change.
- Backend: Undo — before production, revert migration 013 in source. After production application, execute the saved pre-deploy `CREATE OR REPLACE FUNCTION admin_set_access` definition to restore reset-from-now semantics; then record a follow-up migration that formalizes the restored definition before any later schema work.
- External: Compensate — no payment/webhook/email compensation exists. If known smoke-test accounts received additive grants, restore only their pre-recorded `access_expires_at` values. Do not alter unidentified customer accounts.
- Partial rollback: A recipient Retry UI failure can be rolled back without undoing the correct database extension. An admin guidance failure can be rolled back without changing the backend. A backend failure requires restoring the function but not changing device approval code.
- Verification: Re-run focused UI tests and `npm.cmd run test:db`; query `pg_get_functiondef`; confirm a disposable expired account follows the restored semantics; confirm revoked/pending devices remain blocked as expected.

### Execution steps

#### Step 1: Capture a clean baseline and exact pre-change behavior

- Layer: Test
- Dependencies: NONE
- Code/action:
  - Run `git status --short`, record the branch and commit, and stop if unrelated changes overlap any modifiable file.
  - Run:
    - `npm.cmd run test:unit -- src/lib/supabase/account.test.ts src/lib/supabase/devices.test.ts src/lib/verification/verification-provider.test.ts src/stores/verification-store.test.ts src/components/verification/VerificationScreen.test.tsx src/components/verification/VerificationScreen.trial-expired.test.tsx`
    - `npm.cmd run test:db`
  - If Docker is stopped, start Docker Desktop through the normal operator-approved method and rerun; do not skip SQL behavior tests.
  - Save the current function definition from the target staging database before applying migration 013.
- Reason: This distinguishes pre-existing failures from regressions and creates the backend rollback artifact.
- Validation: Focused unit tests and the existing SQL suite pass, or every pre-existing failure is captured verbatim and proven unrelated before proceeding.

#### Step 2: Write the regression tests first and prove they fail for the intended reasons

- Layer: Test
- Dependencies: Step 1
- Code/action:
  - Create `supabase/tests/admin_access_workflows.test.sql` using the same `test_results`, `test_assert`, per-case `DO` block, final result table, and final failure-count pattern as `paddle_billing.test.sql`.
  - Add these SQL cases:
    1. expired + 30 → expiry equals transaction `now() + interval '30 days'`;
    2. active future expiry + 30 → new expiry equals old expiry + 30 days;
    3. active future expiry + 365 → new expiry equals old expiry + 365 days;
    4. grant preserves `suspended=true`, `suspend_reason`, and `paddle_access_expires_at`;
    5. reinstate clears suspension but does not change expiry or device status;
    6. exact incident sequence: first device approved, second device pending, expire account, registration returns `trial_expired`, admin grants 30, second registration returns `device_pending`, admin approves that named device, final registration returns `ok`;
    7. renewal never changes pending/revoked/approved device statuses;
    8. non-admin caller still receives SQLSTATE `42501` and non-positive days still receive `22023`.
  - Create `AccountSection.admin.test.tsx`. Mock `fetchIsAdmin`, `adminListAccounts`, `adminSetAccess`, `adminListDevices`, billing helpers, verification store selectors, and `sonner`. Test the 30 and 365 payloads, pending-device warning, success behavior when device inspection fails, and disabled buttons while the grant promise is unresolved.
  - Create `VerificationScreen.device-pending.test.tsx` following the isolated store-mock pattern in `VerificationScreen.trial-expired.test.tsx`. Assert `device_pending` renders Retry, clicking Retry calls `refresh` exactly once, and `device_revoked` does not render Retry.
  - Run only the new tests against unchanged product code.
- Reason: The repository’s debugging protocol requires RED before GREEN, and these artifacts prevent future regressions across the exact combined workflow.
- Validation: The active-extension SQL cases fail because current SQL resets from `now`; pending Retry and new admin guidance tests fail because the UI behavior is absent. Existing invariants in the same test files must not fail due to setup errors.

#### Step 3: Add migration 013 with atomic additive access semantics

- Layer: Backend
- Dependencies: Step 2 RED evidence
- Code/action:
  - Create `supabase/migrations/013_additive_admin_access.sql`; do not edit migration 006.
  - Use `CREATE OR REPLACE FUNCTION public.admin_set_access(p_user_id uuid, p_days integer) RETURNS void` with the existing `LANGUAGE plpgsql`, `SECURITY DEFINER`, and `SET search_path = public` attributes.
  - Preserve the existing admin authorization and `p_days > 0` validation exactly.
  - Replace only the write calculation:

    ```sql
    INSERT INTO public.account_flags (user_id, access_expires_at)
    VALUES (p_user_id, now() + make_interval(days => p_days))
    ON CONFLICT (user_id) DO UPDATE SET
      access_expires_at =
        GREATEST(
          COALESCE(public.account_flags.access_expires_at, now()),
          now()
        ) + make_interval(days => p_days);
    ```

  - Do not include `suspended`, suspension timestamps/reason, church metadata, offline lease hours, `paddle_access_expires_at`, or device tables in the write.
  - End the migration with explicit `REVOKE ALL ... FROM PUBLIC, anon` and `GRANT EXECUTE ... TO authenticated`, matching migration 006.
  - Keep the file repeat-safe because the SQL harness applies the newest migration twice.
- Reason: This is the smallest backend change that makes “Extend” literal while preserving the RPC contract and every independent gate.
- Validation: Run `npm.cmd run test:db`. All new admin workflow tests and existing Paddle tests pass, and the harness’s `reapply 013_additive_admin_access.sql` step succeeds.

#### Step 4: Make admin renewal wording and pending-device follow-up unambiguous

- Layer: UI
- Dependencies: Step 3
- Code/action in `AccountSection.tsx`:
  - Rename button labels to `Add 30 days` and `Add 1 year`; keep numeric payloads 30 and 365.
  - Add concise helper text near the renewal/device controls: renewal adds account time but does not approve a waiting computer; use Manage computers to approve a named pending device.
  - In `handleExtendAccess()`, call `adminSetAccess()` once only.
  - Keep `busyUserId` set until the access call, the one follow-up device lookup, and the account-list refresh have completed; clear it in a `finally` block so a rapid second click cannot issue another additive grant.
  - On RPC failure: show the existing error and return immediately; do not inspect devices or refresh as success.
  - On success: call `adminListDevices(account.user_id)` once for that account. Count rows with `status === "pending"`.
  - If pending count is greater than zero, show a warning that access was added successfully and state the exact pending count plus “Open Manage computers below to approve it/them.”
  - If pending count is zero or the device query fails, show the normal access-success toast. A failed inspection must not claim the access mutation failed.
  - Refresh the admin account list once after feedback so the new expiry is visible.
  - Preserve `busyUserId` behavior and never retry `adminSetAccess` automatically.
- Reason: The administrator must understand that renewal succeeded and that device approval is a separate, named security action.
- Validation: Run `AccountSection.admin.test.tsx`; confirm 30/365 payloads, one mutation per click, pending guidance, lookup-failure semantics, and busy-state assertions all pass.

#### Step 5: Add one-click Retry for a pending recipient computer

- Layer: UI
- Dependencies: Step 3; does not depend on Step 4 implementation
- Code/action in `VerificationScreen.tsx`:
  - Change `authFeedback().canRetry` so it is true for `status === "error"` when `errorCode` is exactly `network` or `device_pending`, plus the existing stale-session condition.
  - Do not include `device_revoked`, `device_limit_reached`, `suspended`, `trial_expired`, `invalid_credentials`, or `unknown` in this new branch.
  - Keep the existing `onRetry={() => void refresh()}` wiring; do not add a new API, password cache, timer, or polling loop.
  - Keep the existing pending message instructing the user to ask an administrator for approval.
- Reason: After the administrator approves the named device, the recipient can re-run the existing signed registration flow without re-entering credentials.
- Validation: Run `VerificationScreen.device-pending.test.tsx`; pending shows and invokes Retry, revoked does not, and existing sign-in/trial-expired tests remain green.

#### Step 6: Update the living codebase map

- Layer: Documentation
- Dependencies: Steps 3–5
- Code/action in `docs/CODEBASE.md`:
  - Add an “Admin access extension and pending-device recovery” flow under the authentication/device/billing flows.
  - Record that `admin_set_access` adds from `GREATEST(current expiry, now())`, does not alter suspension/Paddle ownership/devices, and that the admin UI performs a post-success pending-device check.
  - Record that `device_pending` exposes Retry using the saved-session refresh path.
  - Update migration/data-model receipts to include migration 013.
  - Add a dated 2026-08-02 changelog row.
  - Do not rewrite unrelated stale line receipts.
- Reason: The repository requires architecture/flow documentation to change with structural database behavior.
- Validation: Every new claim has a current `file:line` receipt after implementation; Markdown lint/diff review shows no unrelated edits.

#### Step 7: Run focused verification and inspect the diff

- Layer: Test
- Dependencies: Steps 3–6
- Code/action:
  - Run:
    - `npm.cmd run test:unit -- src/components/settings/sections/AccountSection.admin.test.tsx src/components/verification/VerificationScreen.device-pending.test.tsx src/lib/supabase/account.test.ts src/lib/supabase/devices.test.ts src/lib/verification/verification-provider.test.ts src/stores/verification-store.test.ts src/components/verification/VerificationScreen.test.tsx src/components/verification/VerificationScreen.trial-expired.test.tsx src/lib/paddle/paddle-billing-contract.test.ts`
    - `npm.cmd run test:db`
    - `npm.cmd run typecheck`
    - `npm.cmd run lint`
    - `git diff --check`
  - Review `git diff --` for only the seven boundary files.
- Reason: This proves the changed path and its sibling contracts before spending time on the full suite.
- Validation: Zero focused failures; SQL output includes every new case and existing Paddle cases; typecheck/lint/diff-check have zero new errors; no file lies outside the boundary.

#### Step 8: Run the full regression suite

- Layer: Test
- Dependencies: Step 7
- Code/action:
  - Run `npm.cmd run test:unit`.
  - If a live-network test fails with sandbox `EACCES`, rerun that exact test with approved network access and record both outputs; do not label it a product pass until the isolated live test succeeds.
  - If a timeout fails only under full-suite contention, rerun it alone and record both outputs; do not raise global timeouts as part of this fix.
  - Rerun `npm.cmd run test:db` after the full unit suite to ensure no test altered the database harness assumptions.
- Reason: The change touches a central access timestamp consumed by trial, Paddle, device, and suspension flows.
- Validation: All deterministic tests pass; any environmental exception is isolated, rerun successfully, and documented without weakening assertions.

#### Step 9: Perform staging workflow smoke tests with disposable accounts

- Layer: Backend
- Dependencies: Step 8
- Code/action:
  - Apply migration 013 to a non-production Supabase environment.
  - Use disposable non-admin accounts and record starting expiry/device/suspension values.
  - Exercise these paths from the actual app:
    1. expired account + Add 30 days + already-approved computer → recipient Retry/sign-in reaches the app;
    2. expired account + pending second computer → admin sees successful renewal plus pending warning; recipient stays pending; admin approves exact device; recipient presses Retry and reaches the app;
    3. active account + Add 30 days → old expiry plus 30 days;
    4. active account + Add 1 year → old expiry plus 365 days;
    5. suspended active account + renewal → remains suspended; Reinstate → access works;
    6. suspended expired account + Reinstate → remains blocked by expiry until renewed;
    7. revoked device → no Retry bypass and registration remains revoked;
    8. two approved devices → approving another still returns the existing limit error.
  - Confirm no Paddle checkout, transaction, webhook, or customer record is created by manual renewal.
- Reason: Unit and SQL tests cannot prove the packaged UI’s operator wording and real cross-computer sequence.
- Validation: Record before/after timestamps and statuses for every scenario; all eight match the non-negotiable decisions and no unrelated table changes appear.

#### Step 10: Deploy in backend-first compatible order and verify production safely

- Layer: Backend
- Dependencies: Step 9 and operator release approval
- Code/action:
  - Snapshot the production function definition and relevant migration/version evidence.
  - Apply migration 013 first. The old UI remains compatible because the RPC signature is unchanged.
  - Verify `pg_get_functiondef` contains the additive `GREATEST` calculation and permissions still allow authenticated admin calls only.
  - Use one designated disposable/internal account for a 30-day smoke grant; record and later restore its original expiry if required.
  - Release the UI build containing guidance and Retry through the existing approved release process.
  - Watch account-service/device-activation errors during the rollout window; specifically look for `42501`, `22023`, device-limit, and unexpected registration responses.
- Reason: Backend-first deployment avoids a new UI depending on behavior that is not yet deployed while preserving compatibility with old clients.
- Validation: Production function definition matches migration 013, one controlled workflow succeeds, no error-rate increase appears, and the released app displays the new copy and pending Retry.

### Exact change checklist for the implementing model

- [ ] Do not edit migrations 001–012.
- [ ] Do not change any RPC name, argument name, return type, or device status string.
- [ ] Do not touch `paddle_access_expires_at` in migration 013.
- [ ] Do not clear suspension during renewal.
- [ ] Do not approve any device during renewal.
- [ ] Do not add polling or automatic renewal retries.
- [ ] Query devices only after one successful renewal, for that account only.
- [ ] Show renewal success even if the follow-up device query fails.
- [ ] Add Retry only for `device_pending`, not revoked/suspended/expired/limit states.
- [ ] Prove new tests fail before implementation and pass afterward.
- [ ] Run SQL tests with Docker; do not substitute source-string assertions for PostgreSQL behavior.
- [ ] Update `docs/CODEBASE.md` in the same change.

### Evidence and handoff artifacts

At implementation completion, attach:

1. RED and GREEN output for `admin_access_workflows.test.sql`.
2. RED and GREEN output for both new TSX test files.
3. Focused/full test, database test, typecheck, lint, and `git diff --check` summaries.
4. Final `git diff --stat` and list of changed files.
5. Staging scenario table with old/new expiry and device status.
6. Production `pg_get_functiondef` verification with secrets and user identifiers removed.
7. A short root-cause note: renewal clears the expiry gate; pending approval is the next independent gate.
