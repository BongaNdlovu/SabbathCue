import { useCallback, useEffect, useState } from "react"
import { AlertTriangleIcon, XIcon } from "lucide-react"
import { PaddleSubscribeButton } from "@/components/billing/PaddleSubscribeButton"
import {
  deriveTrialWarning,
  type TrialWarning,
} from "@/lib/paddle/trial-warning"
import {
  fetchMyBillingSummary,
  type BillingSummary,
} from "@/lib/supabase/billing"
import { useVerificationStore } from "@/stores/verification-store"

const DISMISS_KEY = "sabbathcue.trial-warning-dismissed"

/**
 * Local calendar day. Dismissal is keyed by day so clearing the banner silences
 * it until tomorrow rather than for good — someone must not be able to dismiss
 * on day 3 and then never hear about it again.
 */
export function dismissalDayKey(now: number): string {
  const date = new Date(now)
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${date.getFullYear()}-${month}-${day}`
}

function readDismissedDay(): string | null {
  try {
    return localStorage.getItem(DISMISS_KEY)
  } catch {
    return null
  }
}

function millisecondsUntilNextDay(now: number): number {
  const nextDay = new Date(now)
  nextDay.setHours(24, 0, 0, 0)
  return Math.max(1, nextDay.getTime() - now)
}

function whenText(daysRemaining: number): string {
  if (daysRemaining === 0) return "today"
  if (daysRemaining === 1) return "tomorrow"
  return `in ${daysRemaining} days`
}

export function trialWarningMessage(warning: TrialWarning): string {
  const when = whenText(warning.daysRemaining)
  switch (warning.kind) {
    case "trial_converting":
      return `Your trial ends ${when}. Your card will be charged to keep access.`
    case "payment_failed":
      return `A payment failed. Access ends ${when} — there is no grace period.`
    case "cancel_scheduled":
      return `Your subscription is set to cancel. Access ends ${when}.`
    case "access_ending":
      return `Your access ends ${when}. Renew to keep using SabbathCue.`
  }
}

/**
 * Warns before the server-side access gate closes. Without this the first sign
 * of an expiry is register_device failing and the app locking the operator out,
 * potentially mid-service.
 *
 * Renders nothing unless a warning applies, so it is safe to mount
 * unconditionally.
 */
export function TrialWarningBanner() {
  const status = useVerificationStore((s) => s.status)
  const userId = useVerificationStore((s) => s.verifiedUserId)
  const email = useVerificationStore((s) => s.verifiedEmail)
  const [summary, setSummary] = useState<BillingSummary | null>(null)
  const [now, setNow] = useState(() => Date.now())
  const [dismissedDay, setDismissedDay] = useState<string | null>(() =>
    readDismissedDay()
  )

  const load = useCallback(async () => {
    const result = await fetchMyBillingSummary()
    // A billing lookup must never interrupt a service, so a failure just means
    // no banner. Settings -> Account still reports the real state.
    setSummary(result.ok ? result.summary : null)
  }, [])

  useEffect(() => {
    if (status !== "verified" || !userId) return
    const onFocus = () => {
      setNow(Date.now())
      void load()
    }
    window.addEventListener("focus", onFocus)
    queueMicrotask(onFocus)
    return () => {
      window.removeEventListener("focus", onFocus)
    }
  }, [status, userId, load])

  useEffect(() => {
    let timeoutId: number

    function scheduleNextDay() {
      const current = Date.now()
      timeoutId = window.setTimeout(() => {
        setNow(Date.now())
        scheduleNextDay()
      }, millisecondsUntilNextDay(current))
    }

    scheduleNextDay()
    return () => window.clearTimeout(timeoutId)
  }, [])

  const warning = deriveTrialWarning(summary, now)
  if (!warning || !userId) return null
  if (dismissedDay === dismissalDayKey(now)) return null

  function dismiss() {
    const dismissedAt = Date.now()
    const key = dismissalDayKey(dismissedAt)
    try {
      localStorage.setItem(DISMISS_KEY, key)
    } catch {
      // Private mode / storage disabled: dismiss for this session only.
    }
    setNow(dismissedAt)
    setDismissedDay(key)
  }

  return (
    <div
      role="status"
      className="flex items-center gap-3 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2 text-sm"
    >
      <AlertTriangleIcon
        aria-hidden
        className="size-4 shrink-0 text-amber-600 dark:text-amber-400"
      />
      <p className="min-w-0 flex-1 truncate">{trialWarningMessage(warning)}</p>
      {email ? (
        <PaddleSubscribeButton
          email={email}
          userId={userId}
          label="Subscribe"
        />
      ) : null}
      <button
        type="button"
        aria-label="Dismiss access warning"
        onClick={dismiss}
        className="shrink-0 rounded p-1 opacity-70 transition-opacity hover:opacity-100"
      >
        <XIcon aria-hidden className="size-4" />
      </button>
    </div>
  )
}
