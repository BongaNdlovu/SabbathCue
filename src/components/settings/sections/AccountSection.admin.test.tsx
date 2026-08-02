// @vitest-environment jsdom
//
// Admin renewal is one mutation plus one read-only follow-up. These tests pin
// the three things the incident turned on: the day payloads, the fact that a
// grant is not a device approval, and that a failed device lookup never
// re-presents a completed renewal as something to retry.
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { AdminAccountRow } from "@/lib/supabase/account"
import type { DeviceActivation } from "@/lib/supabase/devices"

// vi.mock factories run before the module body, so the doubles they return
// have to exist by then.
const mocks = vi.hoisted(() => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
  fetchIsAdmin: vi.fn(),
  adminListAccounts: vi.fn(),
  adminSetAccess: vi.fn(),
  adminSetSuspended: vi.fn(),
  adminSetOfflineLeaseHours: vi.fn(),
  adminDeleteAccount: vi.fn(),
  deleteOwnAccount: vi.fn(),
  requestAccountCancellation: vi.fn(),
  adminListDevices: vi.fn(),
  adminSetDeviceStatus: vi.fn(),
  listOwnDevices: vi.fn(),
  deactivateOwnDevice: vi.fn(),
  approveOwnDevice: vi.fn(),
}))

const {
  toast,
  fetchIsAdmin,
  adminListAccounts,
  adminSetAccess,
  adminListDevices,
  listOwnDevices,
} = mocks

const verificationState = {
  verifiedEmail: "admin@sabbathcue.test",
  verifiedUserId: "admin-1",
  verifiedDeviceId: "admin-device",
  offlineGraceExpiresAt: null,
  signOut: vi.fn(),
}

vi.mock("sonner", () => ({ toast: mocks.toast }))

vi.mock("@/lib/supabase/account", () => ({
  adminDeleteAccount: mocks.adminDeleteAccount,
  adminListAccounts: mocks.adminListAccounts,
  adminSetAccess: mocks.adminSetAccess,
  adminSetOfflineLeaseHours: mocks.adminSetOfflineLeaseHours,
  adminSetSuspended: mocks.adminSetSuspended,
  deleteOwnAccount: mocks.deleteOwnAccount,
  fetchIsAdmin: mocks.fetchIsAdmin,
  requestAccountCancellation: mocks.requestAccountCancellation,
}))

vi.mock("@/lib/supabase/devices", () => ({
  adminListDevices: mocks.adminListDevices,
  adminSetDeviceStatus: mocks.adminSetDeviceStatus,
  approveOwnDevice: mocks.approveOwnDevice,
  deactivateOwnDevice: mocks.deactivateOwnDevice,
  listOwnDevices: mocks.listOwnDevices,
}))

vi.mock("@/lib/supabase/billing", () => ({
  fetchMyBillingSummary: vi.fn().mockResolvedValue({
    ok: true,
    summary: { paddleCustomerId: null, accessExpiresAt: null },
  }),
  formatSubscriptionStatusLabel: () => null,
  canCancelSubscription: () => false,
  canActivateSubscriptionEarly: () => false,
  isSubscriptionCancelScheduled: () => false,
}))

vi.mock("@/lib/support-contact", () => ({
  buildCancellationEmailOptions: () => ({}),
  openSupportEmail: vi.fn().mockResolvedValue(undefined),
}))

vi.mock("@/stores/verification-store", () => {
  const useVerificationStore = (
    selector: (state: typeof verificationState) => unknown
  ) => selector(verificationState)
  useVerificationStore.getState = () => verificationState
  return { useVerificationStore }
})

vi.mock("@/components/billing/ManageSubscriptionButton", () => ({
  ManageSubscriptionButton: () => null,
}))
vi.mock("@/components/billing/CancelSubscriptionButton", () => ({
  CancelSubscriptionButton: () => null,
}))
vi.mock("@/components/billing/ActivateSubscriptionButton", () => ({
  ActivateSubscriptionButton: () => null,
}))
vi.mock("@/components/billing/PaddleSubscribePanel", () => ({
  PaddleSubscribePanel: () => null,
}))
vi.mock("@/components/settings/sections/AnnouncementsAdminPanel", () => ({
  AnnouncementsAdminPanel: () => null,
}))

import { AccountSection } from "./AccountSection"

function account(overrides: Partial<AdminAccountRow>): AdminAccountRow {
  return {
    user_id: "user-alpha",
    email: "alpha@example.com",
    created_at: "2026-01-01T00:00:00.000Z",
    suspended: false,
    suspend_reason: null,
    access_expires_at: "2026-09-01T00:00:00.000Z",
    device_count: 1,
    last_seen_at: "2026-08-01T00:00:00.000Z",
    is_admin: false,
    is_church_organization: false,
    church_name: null,
    offline_lease_hours: 72,
    ...overrides,
  }
}

function device(overrides: Partial<DeviceActivation>): DeviceActivation {
  return {
    deviceId: "device-1",
    os: "windows",
    appVersion: "0.1.9",
    label: null,
    status: "approved",
    firstSeenAt: "2026-07-01T00:00:00.000Z",
    lastSeenAt: "2026-08-01T00:00:00.000Z",
    approvedAt: "2026-07-01T00:00:00.000Z",
    revokedAt: null,
    ...overrides,
  }
}

const ALPHA = "alpha@example.com"
const BETA = "beta@example.com"

describe("AdminAccountsPanel access renewal", () => {
  let root: Root | null = null
  let container: HTMLDivElement | null = null

  beforeEach(async () => {
    vi.clearAllMocks()

    fetchIsAdmin.mockResolvedValue(true)
    adminListAccounts.mockResolvedValue({
      ok: true,
      accounts: [
        account({ user_id: "user-alpha", email: ALPHA }),
        account({ user_id: "user-beta", email: BETA }),
      ],
    })
    adminSetAccess.mockResolvedValue({ ok: true })
    adminListDevices.mockResolvedValue({ ok: true, devices: [device({})] })
    listOwnDevices.mockResolvedValue({ ok: true, devices: [] })

    container = document.createElement("div")
    document.body.appendChild(container)
    root = createRoot(container)
    await act(async () => {
      root?.render(<AccountSection />)
    })
    await flush()
  })

  afterEach(async () => {
    await act(async () => root?.unmount())
    container?.remove()
    root = null
    container = null
  })

  async function flush(): Promise<void> {
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
  }

  function card(email: string): HTMLElement {
    const match = Array.from(
      container?.querySelectorAll<HTMLElement>(".glass-panel.p-3") ?? []
    ).find((candidate) => candidate.textContent?.includes(email))
    expect(match).toBeTruthy()
    return match as HTMLElement
  }

  function button(email: string, text: string): HTMLButtonElement {
    const match = Array.from(
      card(email).querySelectorAll<HTMLButtonElement>("button")
    ).find((candidate) => candidate.textContent?.trim() === text)
    expect(match).toBeTruthy()
    return match as HTMLButtonElement
  }

  async function click(element: HTMLElement): Promise<void> {
    await act(async () => {
      element.dispatchEvent(
        new MouseEvent("click", { bubbles: true, cancelable: true })
      )
    })
    await flush()
  }

  it("sends 30 days for the 30-day button", async () => {
    await click(button(ALPHA, "Add 30 days"))

    expect(adminSetAccess).toHaveBeenCalledTimes(1)
    expect(adminSetAccess).toHaveBeenCalledWith("user-alpha", 30)
  })

  it("sends 365 days for the one-year button", async () => {
    await click(button(ALPHA, "Add 1 year"))

    expect(adminSetAccess).toHaveBeenCalledTimes(1)
    expect(adminSetAccess).toHaveBeenCalledWith("user-alpha", 365)
  })

  it("tells the admin a computer is still waiting for approval", async () => {
    adminListDevices.mockResolvedValue({
      ok: true,
      devices: [
        device({ deviceId: "device-approved" }),
        device({ deviceId: "device-waiting", status: "pending" }),
      ],
    })

    await click(button(ALPHA, "Add 30 days"))

    expect(adminListDevices).toHaveBeenCalledTimes(1)
    expect(adminListDevices).toHaveBeenCalledWith("user-alpha")
    expect(toast.warning).toHaveBeenCalledTimes(1)
    const warning = String(toast.warning.mock.calls[0]?.[0])
    expect(warning).toContain("1")
    expect(warning).toContain("Manage computers")
    expect(toast.error).not.toHaveBeenCalled()
  })

  it("reports plain success when no computer is waiting", async () => {
    await click(button(ALPHA, "Add 30 days"))

    expect(toast.success).toHaveBeenCalledTimes(1)
    expect(toast.warning).not.toHaveBeenCalled()
    expect(toast.error).not.toHaveBeenCalled()
  })

  it("still reports success when the follow-up device lookup fails", async () => {
    adminListDevices.mockResolvedValue({
      ok: false,
      message: "Could not load activated computers.",
    })

    await click(button(ALPHA, "Add 30 days"))

    expect(toast.success).toHaveBeenCalledTimes(1)
    expect(toast.error).not.toHaveBeenCalled()
    // The mutation is never replayed to recover a read-only failure.
    expect(adminSetAccess).toHaveBeenCalledTimes(1)
  })

  it("reports the failure and skips device inspection when the grant fails", async () => {
    adminSetAccess.mockResolvedValue({ ok: false, message: "Access update failed." })

    await click(button(ALPHA, "Add 30 days"))

    expect(toast.error).toHaveBeenCalledWith("Access update failed.")
    expect(adminListDevices).not.toHaveBeenCalled()
    expect(toast.success).not.toHaveBeenCalled()
    expect(toast.warning).not.toHaveBeenCalled()
  })

  it("disables the clicked account's renewal buttons until the grant settles", async () => {
    let release: ((value: { ok: true }) => void) | null = null
    adminSetAccess.mockImplementation(
      () =>
        new Promise<{ ok: true }>((resolve) => {
          release = resolve
        })
    )

    await act(async () => {
      button(ALPHA, "Add 30 days").dispatchEvent(
        new MouseEvent("click", { bubbles: true, cancelable: true })
      )
    })

    expect(button(ALPHA, "Add 30 days").disabled).toBe(true)
    expect(button(ALPHA, "Add 1 year").disabled).toBe(true)
    expect(button(BETA, "Add 30 days").disabled).toBe(false)

    // A double click while the grant is in flight must not buy a second period.
    await click(button(ALPHA, "Add 1 year"))
    expect(adminSetAccess).toHaveBeenCalledTimes(1)

    await act(async () => {
      release?.({ ok: true })
    })
    await flush()

    expect(adminSetAccess).toHaveBeenCalledTimes(1)
    expect(button(ALPHA, "Add 30 days").disabled).toBe(false)
  })
})
