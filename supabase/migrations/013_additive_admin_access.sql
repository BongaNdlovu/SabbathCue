-- Make the admin "Extend access" control literal: add days to whatever time
-- the account still has instead of resetting the expiry to now() + p_days.
-- Apply via Supabase SQL Editor AFTER 012_no_past_due_grace.sql.
--
-- Only account_flags.access_expires_at is written. Suspension, Paddle's
-- paddle_access_expires_at, and every devices row are owned by other RPCs and
-- stay untouched, so a renewal never reinstates a suspended account and never
-- approves a waiting computer.
--
-- The whole calculation happens inside one INSERT ... ON CONFLICT DO UPDATE so
-- two concurrent writers cannot both read a stale expiry and lose a grant.

CREATE OR REPLACE FUNCTION public.admin_set_access(
  p_user_id uuid,
  p_days integer
)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM public.app_admins WHERE user_id = auth.uid()) THEN
    RAISE EXCEPTION 'Admin access required'
      USING ERRCODE = '42501';
  END IF;

  IF p_days IS NULL OR p_days <= 0 THEN
    RAISE EXCEPTION 'days must be positive'
      USING ERRCODE = '22023';
  END IF;

  -- GREATEST(existing, now()) means an already-expired account starts from
  -- now() while an active one keeps the time it has left.
  INSERT INTO public.account_flags (user_id, access_expires_at)
  VALUES (p_user_id, now() + make_interval(days => p_days))
  ON CONFLICT (user_id) DO UPDATE SET
    access_expires_at =
      GREATEST(
        COALESCE(public.account_flags.access_expires_at, now()),
        now()
      ) + make_interval(days => p_days);
END;
$$;

REVOKE ALL ON FUNCTION public.admin_set_access(uuid, integer) FROM PUBLIC, anon;
GRANT EXECUTE ON FUNCTION public.admin_set_access(uuid, integer) TO authenticated;
