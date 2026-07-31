// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const invokeTauri = vi.fn()
vi.mock("@/lib/tauri-runtime", () => ({
  invokeTauri: (...args: unknown[]) => invokeTauri(...args),
  isTauriRuntime: () => true,
}))

import { useSettingsStore } from "@/stores/settings-store"

import { AiRankingSection } from "./AiRankingSection"

describe("AiRankingSection", () => {
  beforeEach(() => {
    invokeTauri.mockReset()
    invokeTauri.mockResolvedValue(true)
    useSettingsStore.setState({
      hasDeepseekApiKey: false,
      deepseekRankingEnabled: false,
    })
  })

  afterEach(cleanup)

  it("disables the ranking toggle until a key is configured", () => {
    render(<AiRankingSection />)

    const toggle = screen.getByRole("switch", { name: "AI candidate ranking" })
    expect(toggle.hasAttribute("disabled")).toBe(true)
  })

  it("enables ranking when toggled with a key present", () => {
    useSettingsStore.setState({ hasDeepseekApiKey: true })
    render(<AiRankingSection />)

    const toggle = screen.getByRole("switch", { name: "AI candidate ranking" })
    expect(toggle.hasAttribute("disabled")).toBe(false)

    fireEvent.click(toggle)

    expect(useSettingsStore.getState().deepseekRankingEnabled).toBe(true)
  })

  it("renders the DeepSeek key input", () => {
    render(<AiRankingSection />)

    expect(
      screen.getByPlaceholderText("Enter your DeepSeek API key...")
    ).toBeTruthy()
  })

  it("turning the key off also disables ranking", async () => {
    useSettingsStore.setState({
      hasDeepseekApiKey: true,
      deepseekRankingEnabled: true,
    })
    render(<AiRankingSection />)

    fireEvent.click(screen.getByRole("button", { name: "Remove" }))
    await vi.waitFor(() => {
      expect(useSettingsStore.getState().deepseekRankingEnabled).toBe(false)
    })
    expect(useSettingsStore.getState().hasDeepseekApiKey).toBe(false)
  })

  it("states that suggestions never project automatically", () => {
    render(<AiRankingSection />)

    expect(screen.getByText(/nothing is projected automatically/i)).toBeTruthy()
  })
})
