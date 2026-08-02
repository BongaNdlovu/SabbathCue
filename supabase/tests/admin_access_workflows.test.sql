-- Behavioural tests for supabase/migrations/013_additive_admin_access.sql and
-- the gates admin renewal must NOT touch (suspension, Paddle ownership,
-- device activation state).
-- Run with `npm run test:db` (spins up a throwaway Postgres via Docker).
--
-- Each test runs in its own DO block; a failure is recorded rather than
-- aborting the run, so one broken case does not hide the rest.
--
-- now() is transaction-stable, so a DO block can compute the expected expiry
-- before calling admin_set_access and compare for exact equality.

CREATE TABLE IF NOT EXISTS test_results (
  name text PRIMARY KEY,
  passed boolean NOT NULL,
  detail text
);
TRUNCATE test_results;

CREATE OR REPLACE FUNCTION test_assert(p_condition boolean, p_label text)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
  IF p_condition IS NOT TRUE THEN
    RAISE EXCEPTION 'assertion failed: %', p_label;
  END IF;
END;
$$;


-- An account whose access has already lapsed has no time left to preserve, so
-- the grant starts from database now().
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_expected timestamptz;
  v_access timestamptz;
BEGIN
  INSERT INTO auth.users (email) VALUES ('grant.admin@admin-access.test')
  RETURNING id INTO v_admin;
  INSERT INTO public.app_admins (user_id) VALUES (v_admin);

  INSERT INTO auth.users (email) VALUES ('expired30@admin-access.test')
  RETURNING id INTO v_user;
  UPDATE public.account_flags
  SET access_expires_at = now() - interval '10 days'
  WHERE user_id = v_user;

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  v_expected := now() + interval '30 days';
  PERFORM public.admin_set_access(v_user, 30);
  PERFORM set_config('request.jwt.claim.sub', '', true);

  SELECT access_expires_at INTO v_access
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM test_assert(v_access = v_expected,
    'an expired account granted 30 days expires 30 days from now()');

  INSERT INTO test_results VALUES
    ('expired_account_plus_30_starts_from_now', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('expired_account_plus_30_starts_from_now', false, SQLERRM);
END
$$;


-- The button says "Extend"/"Add". An account that still has 20 days must end
-- up with 50, not 30: reset-from-now silently destroys paid or comped time.
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_before timestamptz;
  v_after timestamptz;
BEGIN
  SELECT user_id INTO v_admin FROM public.app_admins
  WHERE user_id = (SELECT id FROM auth.users
                   WHERE email = 'grant.admin@admin-access.test');

  INSERT INTO auth.users (email) VALUES ('active30@admin-access.test')
  RETURNING id INTO v_user;
  UPDATE public.account_flags
  SET access_expires_at = now() + interval '20 days'
  WHERE user_id = v_user;

  SELECT access_expires_at INTO v_before
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_access(v_user, 30);
  PERFORM set_config('request.jwt.claim.sub', '', true);

  SELECT access_expires_at INTO v_after
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM test_assert(v_after = v_before + interval '30 days',
    'an active account granted 30 days adds to its existing expiry');
  PERFORM test_assert(v_after > now() + interval '49 days',
    'the account holds roughly 50 future days, not 30');

  INSERT INTO test_results VALUES
    ('active_account_plus_30_preserves_existing_time', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('active_account_plus_30_preserves_existing_time', false, SQLERRM);
END
$$;


-- Same rule for the one-year button: exactly 365 days on top of what is left.
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_before timestamptz;
  v_after timestamptz;
BEGIN
  SELECT user_id INTO v_admin FROM public.app_admins
  WHERE user_id = (SELECT id FROM auth.users
                   WHERE email = 'grant.admin@admin-access.test');

  INSERT INTO auth.users (email) VALUES ('active365@admin-access.test')
  RETURNING id INTO v_user;
  UPDATE public.account_flags
  SET access_expires_at = now() + interval '20 days'
  WHERE user_id = v_user;

  SELECT access_expires_at INTO v_before
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_access(v_user, 365);
  PERFORM set_config('request.jwt.claim.sub', '', true);

  SELECT access_expires_at INTO v_after
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM test_assert(v_after = v_before + interval '365 days',
    'an active account granted 1 year adds 365 days to its existing expiry');

  INSERT INTO test_results VALUES
    ('active_account_plus_365_preserves_existing_time', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('active_account_plus_365_preserves_existing_time', false, SQLERRM);
END
$$;


-- Suspension is the highest account-level block and Paddle owns its own
-- column. A manual grant moves the expiry and nothing else, so a suspended
-- account stays locked out and a later Paddle event cannot mistake the comp
-- for paid access and claw it back.
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_paddle_before timestamptz;
  v_paddle_after timestamptz;
  v_suspended boolean;
  v_reason text;
  v_registration jsonb;
BEGIN
  SELECT user_id INTO v_admin FROM public.app_admins
  WHERE user_id = (SELECT id FROM auth.users
                   WHERE email = 'grant.admin@admin-access.test');

  INSERT INTO auth.users (email) VALUES ('suspended.grant@admin-access.test')
  RETURNING id INTO v_user;
  UPDATE public.account_flags
  SET paddle_access_expires_at = now() + interval '5 days'
  WHERE user_id = v_user;

  SELECT paddle_access_expires_at INTO v_paddle_before
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_suspended(v_user, true, 'billing hold');
  PERFORM public.admin_set_access(v_user, 30);
  PERFORM set_config('request.jwt.claim.sub', '', true);

  SELECT suspended, suspend_reason, paddle_access_expires_at
  INTO v_suspended, v_reason, v_paddle_after
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM test_assert(v_suspended IS TRUE,
    'renewal does not reinstate a suspended account');
  PERFORM test_assert(v_reason = 'billing hold',
    'renewal does not clear the suspension reason');
  PERFORM test_assert(v_paddle_after IS NOT DISTINCT FROM v_paddle_before,
    'renewal leaves paddle_access_expires_at untouched');

  PERFORM set_config('request.jwt.claim.role', 'service_role', true);
  v_registration := public.register_device_verified(
    v_user, 'device-suspended-1', 'windows', '0.1.9');
  PERFORM set_config('request.jwt.claim.role', '', true);

  PERFORM test_assert(v_registration->>'status' = 'suspended',
    'a renewed but suspended account is still refused at registration');

  INSERT INTO test_results VALUES
    ('grant_preserves_suspension_and_paddle_column', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('grant_preserves_suspension_and_paddle_column', false, SQLERRM);
END
$$;


-- Reinstate is the mirror image: it owns the suspension flag only. It must not
-- hand back access time or silently re-approve a computer.
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_access_before timestamptz;
  v_access_after timestamptz;
  v_status_before text;
  v_status_after text;
  v_suspended boolean;
BEGIN
  SELECT user_id INTO v_admin FROM public.app_admins
  WHERE user_id = (SELECT id FROM auth.users
                   WHERE email = 'grant.admin@admin-access.test');

  INSERT INTO auth.users (email) VALUES ('reinstate@admin-access.test')
  RETURNING id INTO v_user;

  PERFORM set_config('request.jwt.claim.role', 'service_role', true);
  PERFORM public.register_device_verified(
    v_user, 'device-reinstate-1', 'windows', '0.1.9');
  PERFORM set_config('request.jwt.claim.role', '', true);

  SELECT status INTO v_status_before FROM public.devices
  WHERE user_id = v_user AND device_id = 'device-reinstate-1';

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_suspended(v_user, true, 'temporary');

  SELECT access_expires_at INTO v_access_before
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM public.admin_set_suspended(v_user, false);
  PERFORM set_config('request.jwt.claim.sub', '', true);

  SELECT suspended, access_expires_at INTO v_suspended, v_access_after
  FROM public.account_flags WHERE user_id = v_user;
  SELECT status INTO v_status_after FROM public.devices
  WHERE user_id = v_user AND device_id = 'device-reinstate-1';

  PERFORM test_assert(v_suspended IS FALSE, 'reinstate clears suspension');
  PERFORM test_assert(v_access_after IS NOT DISTINCT FROM v_access_before,
    'reinstate does not change the access expiry');
  PERFORM test_assert(v_status_after = v_status_before,
    'reinstate does not change device status');

  INSERT INTO test_results VALUES
    ('reinstate_clears_suspension_only', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('reinstate_clears_suspension_only', false, SQLERRM);
END
$$;


-- The reported incident, end to end. Renewal clears the expiry gate; the
-- second computer then stops on the NEXT independent gate (approval) rather
-- than being let in by the grant.
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_first jsonb;
  v_second jsonb;
  v_expired jsonb;
  v_after_grant jsonb;
  v_final jsonb;
  v_first_status text;
BEGIN
  SELECT user_id INTO v_admin FROM public.app_admins
  WHERE user_id = (SELECT id FROM auth.users
                   WHERE email = 'grant.admin@admin-access.test');

  INSERT INTO auth.users (email) VALUES ('incident@admin-access.test')
  RETURNING id INTO v_user;

  PERFORM set_config('request.jwt.claim.role', 'service_role', true);

  v_first := public.register_device_verified(
    v_user, 'device-incident-a', 'windows', '0.1.9');
  PERFORM test_assert(v_first->>'status' = 'ok',
    'the first computer activates on the trial');

  v_second := public.register_device_verified(
    v_user, 'device-incident-b', 'windows', '0.1.9');
  PERFORM test_assert(v_second->>'status' = 'device_pending',
    'the second computer starts pending approval');

  UPDATE public.account_flags
  SET access_expires_at = now() - interval '1 day'
  WHERE user_id = v_user;

  v_expired := public.register_device_verified(
    v_user, 'device-incident-b', 'windows', '0.1.9');
  PERFORM test_assert(v_expired->>'status' = 'trial_expired',
    'expiry is reported before device status');

  PERFORM set_config('request.jwt.claim.role', '', true);
  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_access(v_user, 30);
  PERFORM set_config('request.jwt.claim.sub', '', true);
  PERFORM set_config('request.jwt.claim.role', 'service_role', true);

  v_after_grant := public.register_device_verified(
    v_user, 'device-incident-b', 'windows', '0.1.9');
  PERFORM test_assert(v_after_grant->>'status' = 'device_pending',
    'renewal clears the expiry gate but does not approve the computer');

  PERFORM set_config('request.jwt.claim.role', '', true);
  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_device_status(
    v_user, 'device-incident-b', 'approved');
  PERFORM set_config('request.jwt.claim.sub', '', true);
  PERFORM set_config('request.jwt.claim.role', 'service_role', true);

  v_final := public.register_device_verified(
    v_user, 'device-incident-b', 'windows', '0.1.9');
  PERFORM set_config('request.jwt.claim.role', '', true);

  PERFORM test_assert(v_final->>'status' = 'ok',
    'the named approval is what finally lets the second computer in');

  SELECT status INTO v_first_status FROM public.devices
  WHERE user_id = v_user AND device_id = 'device-incident-a';
  PERFORM test_assert(v_first_status = 'approved',
    'the first computer is untouched by the whole sequence');

  INSERT INTO test_results VALUES
    ('expired_then_renewed_then_pending_then_approved', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('expired_then_renewed_then_pending_then_approved', false, SQLERRM);
END
$$;


-- Access and device approval are separate security gates. A grant must not
-- move any device row in any direction, including the approval timestamps.
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_before jsonb;
  v_after jsonb;
BEGIN
  SELECT user_id INTO v_admin FROM public.app_admins
  WHERE user_id = (SELECT id FROM auth.users
                   WHERE email = 'grant.admin@admin-access.test');

  INSERT INTO auth.users (email) VALUES ('devicestates@admin-access.test')
  RETURNING id INTO v_user;

  PERFORM set_config('request.jwt.claim.role', 'service_role', true);
  PERFORM public.register_device_verified(
    v_user, 'device-states-approved', 'windows', '0.1.9');
  PERFORM public.register_device_verified(
    v_user, 'device-states-pending', 'windows', '0.1.9');
  PERFORM public.register_device_verified(
    v_user, 'device-states-revoked', 'windows', '0.1.9');
  PERFORM set_config('request.jwt.claim.role', '', true);

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_device_status(
    v_user, 'device-states-revoked', 'revoked');
  PERFORM set_config('request.jwt.claim.sub', '', true);

  SELECT jsonb_agg(jsonb_build_object(
           'device_id', device_id, 'status', status,
           'approved_at', approved_at, 'revoked_at', revoked_at)
         ORDER BY device_id)
  INTO v_before FROM public.devices WHERE user_id = v_user;

  PERFORM test_assert(
    (SELECT count(*) FROM public.devices
      WHERE user_id = v_user AND status = 'approved') = 1
    AND (SELECT count(*) FROM public.devices
      WHERE user_id = v_user AND status = 'pending') = 1
    AND (SELECT count(*) FROM public.devices
      WHERE user_id = v_user AND status = 'revoked') = 1,
    'the fixture holds one approved, one pending, and one revoked computer');

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  PERFORM public.admin_set_access(v_user, 30);
  PERFORM set_config('request.jwt.claim.sub', '', true);

  SELECT jsonb_agg(jsonb_build_object(
           'device_id', device_id, 'status', status,
           'approved_at', approved_at, 'revoked_at', revoked_at)
         ORDER BY device_id)
  INTO v_after FROM public.devices WHERE user_id = v_user;

  PERFORM test_assert(v_after = v_before,
    'renewal leaves every device row exactly as it was');

  INSERT INTO test_results VALUES
    ('renewal_never_changes_device_status', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('renewal_never_changes_device_status', false, SQLERRM);
END
$$;


-- The security gates on the RPC are unchanged by the new calculation.
DO $$
DECLARE
  v_admin uuid;
  v_user uuid;
  v_outsider uuid;
  v_state text;
  v_access timestamptz;
BEGIN
  SELECT user_id INTO v_admin FROM public.app_admins
  WHERE user_id = (SELECT id FROM auth.users
                   WHERE email = 'grant.admin@admin-access.test');

  INSERT INTO auth.users (email) VALUES ('gatetarget@admin-access.test')
  RETURNING id INTO v_user;
  INSERT INTO auth.users (email) VALUES ('outsider@admin-access.test')
  RETURNING id INTO v_outsider;

  SELECT access_expires_at INTO v_access
  FROM public.account_flags WHERE user_id = v_user;

  PERFORM set_config('request.jwt.claim.sub', v_outsider::text, true);
  BEGIN
    PERFORM public.admin_set_access(v_user, 30);
    v_state := 'no error';
  EXCEPTION WHEN others THEN
    v_state := SQLSTATE;
  END;
  PERFORM test_assert(v_state = '42501',
    'a non-admin caller still gets 42501');

  PERFORM set_config('request.jwt.claim.sub', v_admin::text, true);
  BEGIN
    PERFORM public.admin_set_access(v_user, 0);
    v_state := 'no error';
  EXCEPTION WHEN others THEN
    v_state := SQLSTATE;
  END;
  PERFORM test_assert(v_state = '22023', 'zero days still gets 22023');

  BEGIN
    PERFORM public.admin_set_access(v_user, -5);
    v_state := 'no error';
  EXCEPTION WHEN others THEN
    v_state := SQLSTATE;
  END;
  PERFORM test_assert(v_state = '22023', 'negative days still gets 22023');

  BEGIN
    PERFORM public.admin_set_access(v_user, NULL);
    v_state := 'no error';
  EXCEPTION WHEN others THEN
    v_state := SQLSTATE;
  END;
  PERFORM test_assert(v_state = '22023', 'null days still gets 22023');
  PERFORM set_config('request.jwt.claim.sub', '', true);

  PERFORM test_assert(
    (SELECT access_expires_at FROM public.account_flags
      WHERE user_id = v_user) IS NOT DISTINCT FROM v_access,
    'no rejected call moved the expiry');

  INSERT INTO test_results VALUES
    ('admin_set_access_authorization_and_validation', true, NULL);
EXCEPTION WHEN others THEN
  INSERT INTO test_results VALUES
    ('admin_set_access_authorization_and_validation', false, SQLERRM);
END
$$;


\echo ''
SELECT
  CASE WHEN passed THEN 'ok  ' ELSE 'FAIL' END AS result,
  name,
  detail
FROM test_results
ORDER BY passed, name;

DO $$
DECLARE
  v_failures integer;
BEGIN
  SELECT count(*) INTO v_failures FROM test_results WHERE NOT passed;
  IF v_failures > 0 THEN
    RAISE EXCEPTION '% of % SQL test(s) failed',
      v_failures, (SELECT count(*) FROM test_results);
  END IF;
  RAISE NOTICE 'all % SQL test(s) passed', (SELECT count(*) FROM test_results);
END
$$;
