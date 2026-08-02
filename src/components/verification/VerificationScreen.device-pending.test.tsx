// @vitest-environment jsdom
//
// A computer that is waiting for approval already holds a valid saved session,
// so recovering after an administrator approves it is a refresh, not a second
// sign-in. Retry is offered for that state and for nothing else that requires
// an explicit administrative decision.
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, describe, expect, it, vi } from "vitest"
import type { VerificationErrorCode } from "@/types/verification"

const refresh = vi.fn()

const verificationState = {
  status: "error" as const,
  error: null as string | null,
  errorCode: "device_pending" as VerificationErrorCode,
  verifiedEmail: "pastor@church.org",
  verifiedUserId: "user-123",
  signIn: vi.fn(),
  signUp: vi.fn(),
  signOut: vi.fn(),
  refresh,
}

vi.mock("@/stores/verification-store", () => {
  const useVerificationStore = (
    selector: (state: typeof verificationState) => unknown
  ) => selector(verificationState)
  useVerificationStore.getState = () => verificationState
  return { useVerificationStore }
})

vi.mock("@/stores/accent-theme-store", () => ({
  accentThemeClassName: () => "",
  useAccentThemeStore: (selector: (state: { theme: string }) => unknown) =>
    selector({ theme: "amber" }),
}))

vi.mock("@/stores/color-mode-store", () => ({
  darkSurfaceClassName: () => "",
  useColorModeStore: (selector: (state: { darkSurface: boolean }) => unknown) =>
    selector({ darkSurface: false }),
}))

import { VerificationScreen } from "./VerificationScreen"

describe("VerificationScreen pending computer", () => {
  let root: Root | null = null
  let container: HTMLDivElement | null = null

  afterEach(async () => {
    if (root) await act(async () => root?.unmount())
    container?.remove()
    root = null
    container = null
  })

  async function mount(errorCode: VerificationErrorCode): Promise<void> {
    vi.clearAllMocks()
    verificationState.errorCode = errorCode
    container = document.createElement("div")
    document.body.appendChild(container)
    root = createRoot(container)
    await act(async () => {
      root?.render(<VerificationScreen />)
    })
  }

  function retryButton(): HTMLButtonElement | undefined {
    return Array.from(
      container?.querySelectorAll<HTMLButtonElement>("button") ?? []
    ).find((candidate) => candidate.textContent?.trim() === "Retry")
  }

  it("offers Retry once an administrator has approved this computer", async () => {
    await mount("device_pending")

    expect(container?.textContent).toContain("waiting for activation approval")
    expect(retryButton()).toBeTruthy()
  })

  it("reuses the saved session instead of asking for the password again", async () => {
    await mount("device_pending")

    await act(async () => {
      retryButton()?.dispatchEvent(
        new MouseEvent("click", { bubbles: true, cancelable: true })
      )
    })

    expect(refresh).toHaveBeenCalledTimes(1)
  })

  it("does not offer Retry to a revoked computer", async () => {
    await mount("device_revoked")

    expect(container?.textContent).toContain("has been deactivated")
    expect(retryButton()).toBeUndefined()
  })

  it.each(["device_limit_reached", "suspended", "invalid_credentials"] as const)(
    "does not offer Retry for %s",
    async (errorCode) => {
      await mount(errorCode)

      expect(retryButton()).toBeUndefined()
    }
  )
})
