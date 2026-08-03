// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it } from "vitest"

import { useBroadcastStore } from "@/stores/broadcast-store"
import { useSettingsStore } from "@/stores/settings-store"

import { DisplayModeSection } from "./DisplayModeSection"

describe("DisplayModeSection Bible mode", () => {
  beforeEach(() => {
    useSettingsStore.setState({
      bibleDetectionEnabled: true,
      semanticDetectionEnabled: true,
      semanticConfidenceThreshold: 0.7,
    })
    useBroadcastStore.setState({ readingModeAutoLive: false })
  })

  afterEach(cleanup)

  it("turns off Bible detection while leaving the transcription promise visible", () => {
    render(<DisplayModeSection />)

    const bibleMode = screen.getByRole("switch", { name: "Bible mode" })
    const semanticMode = screen.getByRole("switch", {
      name: "Semantic detection",
    })

    expect(bibleMode.getAttribute("data-state")).toBe("checked")
    expect(semanticMode.hasAttribute("disabled")).toBe(false)

    fireEvent.click(bibleMode)

    expect(useSettingsStore.getState().bibleDetectionEnabled).toBe(false)
    expect(semanticMode.hasAttribute("disabled")).toBe(true)
    expect(
      screen.getByText(
        "Bible mode is off. Transcription continues without Bible suggestions."
      )
    ).not.toBeNull()
  })

  it("uses the settings Auto Live switch as the live-output permission", () => {
    render(<DisplayModeSection />)

    const autoLive = screen.getByRole("switch", { name: "Auto Live output" })
    expect(autoLive.getAttribute("data-state")).toBe("unchecked")

    fireEvent.click(autoLive)

    expect(useBroadcastStore.getState().readingModeAutoLive).toBe(true)
  })
})
