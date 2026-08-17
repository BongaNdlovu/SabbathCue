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
      hasCerebrasApiKey: false,
      aiRankingEnabled: false,
      aiRankingProvider: "deepseek",
      deepseekRankingEnabled: false,
    })
  })

  afterEach(cleanup)

  it("disables the ranking toggle until a key is configured for active provider", () => {
    render(<AiRankingSection />)

    const toggle = screen.getByRole("switch", { name: "AI candidate ranking" })
    expect(toggle.hasAttribute("disabled")).toBe(true)
    expect(
      screen.getByText(/Save a DeepSeek API key below to enable ranking/i)
    ).toBeTruthy()
  })

  it("enables ranking when toggled with a key present for DeepSeek", () => {
    useSettingsStore.setState({ hasDeepseekApiKey: true })
    render(<AiRankingSection />)

    const toggle = screen.getByRole("switch", { name: "AI candidate ranking" })
    expect(toggle.hasAttribute("disabled")).toBe(false)

    fireEvent.click(toggle)

    expect(useSettingsStore.getState().aiRankingEnabled).toBe(true)
  })

  it("renders the DeepSeek key input by default", () => {
    render(<AiRankingSection />)

    expect(
      screen.getByPlaceholderText("Enter your DeepSeek API key...")
    ).toBeTruthy()
  })

  it("switches to Cerebras and renders Cerebras key input when provider changed", () => {
    render(<AiRankingSection />)

    const cerebrasRadio = screen.getByRole("radio", {
      name: /Cerebras GPT-OSS-120B/i,
    })
    fireEvent.click(cerebrasRadio)

    expect(useSettingsStore.getState().aiRankingProvider).toBe("cerebras")
    expect(
      screen.getByPlaceholderText("Enter your Cerebras API key...")
    ).toBeTruthy()
    expect(
      screen.getByText(/Save a Cerebras API key below to enable ranking/i)
    ).toBeTruthy()
  })

  it("switching providers preserves both key-presence states", () => {
    useSettingsStore.setState({
      hasDeepseekApiKey: true,
      hasCerebrasApiKey: true,
      aiRankingEnabled: true,
    })
    render(<AiRankingSection />)

    const cerebrasRadio = screen.getByRole("radio", {
      name: /Cerebras GPT-OSS-120B/i,
    })
    fireEvent.click(cerebrasRadio)

    expect(useSettingsStore.getState().hasDeepseekApiKey).toBe(true)
    expect(useSettingsStore.getState().hasCerebrasApiKey).toBe(true)
  })

  it("disables ranking toggle if switching to a provider with no key", () => {
    useSettingsStore.setState({
      hasDeepseekApiKey: true,
      hasCerebrasApiKey: false,
      aiRankingEnabled: true,
    })
    render(<AiRankingSection />)

    const cerebrasRadio = screen.getByRole("radio", {
      name: /Cerebras GPT-OSS-120B/i,
    })
    fireEvent.click(cerebrasRadio)

    const toggle = screen.getByRole("switch", { name: "AI candidate ranking" })
    expect(toggle.hasAttribute("disabled")).toBe(true)
  })

  it("requires a fresh opt-in after changing ranking provider", () => {
    useSettingsStore.setState({
      hasDeepseekApiKey: true,
      hasCerebrasApiKey: false,
      aiRankingEnabled: true,
      deepseekRankingEnabled: true,
      aiRankingProvider: "deepseek",
    })
    render(<AiRankingSection />)

    fireEvent.click(
      screen.getByRole("radio", { name: /Cerebras GPT-OSS-120B/i })
    )

    const state = useSettingsStore.getState()
    expect(state.aiRankingProvider).toBe("cerebras")
    expect(state.aiRankingEnabled).toBe(false)
    expect(state.deepseekRankingEnabled).toBe(false)
  })

  it("turning the active provider key off also disables ranking", async () => {
    useSettingsStore.setState({
      hasDeepseekApiKey: true,
      aiRankingEnabled: true,
      aiRankingProvider: "deepseek",
    })
    render(<AiRankingSection />)

    fireEvent.click(screen.getByRole("button", { name: "Remove" }))
    await vi.waitFor(() => {
      expect(useSettingsStore.getState().aiRankingEnabled).toBe(false)
    })
    expect(useSettingsStore.getState().hasDeepseekApiKey).toBe(false)
  })

  it("removing an active Cerebras key disables ranking", async () => {
    useSettingsStore.setState({
      hasCerebrasApiKey: true,
      aiRankingEnabled: true,
      aiRankingProvider: "cerebras",
    })
    render(<AiRankingSection />)

    fireEvent.click(screen.getByRole("button", { name: "Remove" }))
    await vi.waitFor(() => {
      expect(useSettingsStore.getState().aiRankingEnabled).toBe(false)
    })
    expect(useSettingsStore.getState().hasCerebrasApiKey).toBe(false)
  })

  it("states that suggestions never project automatically and describes bounded data flow", () => {
    render(<AiRankingSection />)

    expect(screen.getByText(/nothing is projected automatically/i)).toBeTruthy()
    expect(
      screen.getByText(/up to eight locally detected reference-and-verse candidate packs/i)
    ).toBeTruthy()
  })
})
