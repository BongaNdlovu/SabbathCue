// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from "vitest"
import { handleDashboardKeyboardEvent } from "./use-dashboard-keyboard-controls"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import type { HymnPresentationItemData } from "@/types"

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: vi.fn().mockResolvedValue(undefined),
}))

function makeSlide(index: number): HymnPresentationItemData {
  return {
    kind: "hymn",
    hymnId: "hymn-12",
    hymnNumber: 12,
    hymnTitle: "Joyful",
    screenId: `screen-${index}`,
    slideIndex: index,
    slideCount: 3,
    reference: `Joyful - Slide ${index + 1}/3`,
    segments: [{ text: `Line ${index + 1}` }],
  }
}

function previewFirstDeckSlide() {
  const deck = useHymnSlideStore.getState().deck
  useBroadcastStore.getState().setPreviewItem({
    kind: "hymn",
    reference: deck[0].reference,
    segments: deck[0].segments,
    hymnSlide: {
      screenId: deck[0].screenId,
      slideIndex: deck[0].slideIndex,
      slideCount: deck[0].slideCount,
    },
  })
}

describe("handleDashboardKeyboardEvent", () => {
  beforeEach(() => {
    useHymnSlideStore.getState().setDeck([makeSlide(0), makeSlide(1), makeSlide(2)], 0)
    useBroadcastStore.setState({
      isLive: false,
      previewItem: null,
      liveItem: null,
    })
  })

  it("advances a staged hymn preview with arrow keys", () => {
    previewFirstDeckSlide()

    handleDashboardKeyboardEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }))

    expect(useHymnSlideStore.getState().activeIndex).toBe(1)
    expect(useBroadcastStore.getState().previewItem?.hymnSlide?.screenId).toBe("screen-1")
  })

  it("advances a live hymn with arrow keys", () => {
    const deck = useHymnSlideStore.getState().deck
    useBroadcastStore.setState({
      isLive: true,
      liveItem: {
        kind: "hymn",
        reference: deck[0].reference,
        segments: deck[0].segments,
        hymnSlide: {
          screenId: deck[0].screenId,
          slideIndex: deck[0].slideIndex,
          slideCount: deck[0].slideCount,
        },
      },
    })

    handleDashboardKeyboardEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }))

    expect(useHymnSlideStore.getState().activeIndex).toBe(1)
    expect(useBroadcastStore.getState().liveItem?.hymnSlide?.screenId).toBe("screen-1")
  })

  it("ignores arrow keys from editable controls", () => {
    previewFirstDeckSlide()
    const input = document.createElement("input")
    const event = new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })
    input.dispatchEvent(event)

    handleDashboardKeyboardEvent(event)

    expect(useHymnSlideStore.getState().activeIndex).toBe(0)
    expect(useBroadcastStore.getState().previewItem?.hymnSlide?.screenId).toBe("screen-0")
  })
})
