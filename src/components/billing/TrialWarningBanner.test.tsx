// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { BillingSummary } from "@/lib/supabase/billing"

const DAY = 24 * 60 * 60 * 1000
// Pinned: building an expiry from one Date.now() and reading it from another a
// microsecond later turns "2 days" into 1.999, which floors to 1.
const NOW = Date.parse("2026-07-24T12:00:00.000Z")

const verificationState = {
  status: "verified" as string,
  verifiedUserId: "user-123" as string | null,
  verifiedEmail: "pastor@church.org" as string | null,
}

vi.mock("@/stores/verification-store", () => {
  const useVerificationStore = (
    selector: (state: typeof verificationState) => unknown
  ) => selector(verificationState)
  useVerificationStore.getState = () => verificationState
  return { useVerificationStore }
})

const fetchMyBillingSummary = vi.fn()
vi.mock("@/lib/supabase/billing", () => ({
  fetchMyBillingSummary: () => fetchMyBillingSummary(),
}))

vi.mock("@/components/billing/PaddleSubscribeButton", () => ({
  PaddleSubscribeButton: ({ label }: { label?: string }) => (
    <button type="button">{label ?? "Subscribe"}</button>
  ),
}))

import { TrialWarningBanner, dismissalDayKey } from "./TrialWarningBanner"

function summary(overrides: Partial<BillingSummary> = {}): BillingSummary {
  return {
    paddleCustomerId: "ctm_1",
    subscriptionId: "sub_1",
    subscriptionStatus: "trialing",
    scheduledChange: null,
    scheduledChangeAction: null,
    currentPeriodEnd: null,
    accessExpiresAt: new Date(NOW + 2 * DAY).toISOString(),
    ...overrides,
  }
}

let container: HTMLDivElement
let root: Root

async function render() {
  await act(async () => {
    root.render(<TrialWarningBanner />)
  })
}

beforeEach(() => {
  vi.spyOn(Date, "now").mockReturnValue(NOW)
  container = document.createElement("div")
  document.body.appendChild(container)
  root = createRoot(container)
  localStorage.clear()
  fetchMyBillingSummary.mockReset()
  verificationState.status = "verified"
  verificationState.verifiedUserId = "user-123"
  verificationState.verifiedEmail = "pastor@church.org"
})

afterEach(() => {
  act(() => root.unmount())
  vi.useRealTimers()
  container.remove()
})

describe("TrialWarningBanner", () => {
  it("warns a trialing account that its card will be charged", async () => {
    fetchMyBillingSummary.mockResolvedValue({ ok: true, summary: summary() })
    await render()

    expect(container.textContent).toContain("Your trial ends in 2 days")
    expect(container.textContent).toContain("card will be charged")
    expect(container.querySelector("[role=status]")).not.toBeNull()
  })

  it("renders nothing for a healthy auto-renewing subscriber", async () => {
    fetchMyBillingSummary.mockResolvedValue({
      ok: true,
      summary: summary({ subscriptionStatus: "active" }),
    })
    await render()

    expect(container.textContent).toBe("")
  })

  it("renders nothing when the billing lookup fails", async () => {
    fetchMyBillingSummary.mockResolvedValue({ ok: false, message: "offline" })
    await render()

    expect(container.textContent).toBe("")
  })

  it("says there is no grace period when a payment failed", async () => {
    fetchMyBillingSummary.mockResolvedValue({
      ok: true,
      summary: summary({ subscriptionStatus: "past_due" }),
    })
    await render()

    expect(container.textContent).toContain("no grace period")
  })

  it("hides once dismissed and records today", async () => {
    fetchMyBillingSummary.mockResolvedValue({ ok: true, summary: summary() })
    await render()

    const dismiss = container.querySelector<HTMLButtonElement>(
      "[aria-label='Dismiss access warning']"
    )
    expect(dismiss).not.toBeNull()
    await act(async () => {
      dismiss?.click()
    })

    expect(container.textContent).toBe("")
    expect(localStorage.getItem("sabbathcue.trial-warning-dismissed")).toBe(
      dismissalDayKey(Date.now())
    )
  })

  it("stays hidden when midnight passes before dismissal", async () => {
    const beforeMidnight = new Date(2026, 6, 24, 23, 59, 59).getTime()
    const afterMidnight = new Date(2026, 6, 25, 0, 0, 1).getTime()
    vi.mocked(Date.now).mockReturnValue(beforeMidnight)
    fetchMyBillingSummary.mockResolvedValue({
      ok: true,
      summary: summary({
        accessExpiresAt: new Date(beforeMidnight + 2 * DAY).toISOString(),
      }),
    })
    await render()

    vi.mocked(Date.now).mockReturnValue(afterMidnight)
    const dismiss = container.querySelector<HTMLButtonElement>(
      "[aria-label='Dismiss access warning']"
    )
    expect(dismiss).not.toBeNull()
    await act(async () => {
      dismiss?.click()
    })

    expect(localStorage.getItem("sabbathcue.trial-warning-dismissed")).toBe(
      dismissalDayKey(afterMidnight)
    )
    expect(container.textContent).toBe("")
  })

  it("returns at midnight without requiring a focus event", async () => {
    const beforeMidnight = new Date(2026, 6, 24, 23, 59, 59).getTime()
    vi.mocked(Date.now).mockRestore()
    vi.useFakeTimers({ now: beforeMidnight })
    fetchMyBillingSummary.mockResolvedValue({
      ok: true,
      summary: summary({
        accessExpiresAt: new Date(beforeMidnight + 2 * DAY).toISOString(),
      }),
    })
    await render()

    const dismiss = container.querySelector<HTMLButtonElement>(
      "[aria-label='Dismiss access warning']"
    )
    await act(async () => {
      dismiss?.click()
    })
    expect(container.textContent).toBe("")

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000)
    })

    expect(container.textContent).toContain("Your trial ends")
  })

  // Dismissal must not be permanent: a user who clears it on day 3 still needs
  // to hear about it on day 2 and day 1.
  it("returns the next day after being dismissed", async () => {
    localStorage.setItem(
      "sabbathcue.trial-warning-dismissed",
      dismissalDayKey(Date.now() - DAY)
    )
    fetchMyBillingSummary.mockResolvedValue({ ok: true, summary: summary() })
    await render()

    expect(container.textContent).toContain("Your trial ends")
  })

  it("does not query billing until the device is verified", async () => {
    verificationState.status = "checking"
    fetchMyBillingSummary.mockResolvedValue({ ok: true, summary: summary() })
    await render()

    expect(fetchMyBillingSummary).not.toHaveBeenCalled()
    expect(container.textContent).toBe("")
  })
})
