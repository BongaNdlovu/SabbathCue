// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  getQueuedHymnDeckForRenderItem,
  restoreQueuedHymnDeckForRenderItem,
} from "@/lib/queued-hymn-deck"
import {
  presentQueuedItem,
  previewQueuedItem,
  previewQueuedItemAtEnd,
} from "@/lib/queue-presentation"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import { useQueueStore } from "@/stores/queue-store"
import { getPresentationRenderData } from "@/types"
import type { HymnPresentationItemData, QueueItem } from "@/types"

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: vi.fn().mockResolvedValue(undefined),
}))

function makeSlide(deckId: string, index: number): HymnPresentationItemData {
  return {
    kind: "hymn",
    hymnId: deckId,
    hymnNumber: deckId === "queued" ? 12 : 34,
    hymnTitle: deckId === "queued" ? "Queued Hymn" : "Current Hymn",
    screenId: `${deckId}-screen-${index}`,
    slideIndex: index,
    slideCount: 3,
    reference: `${deckId} - Slide ${index + 1}`,
    segments: [{ text: `${deckId} line ${index + 1}` }],
  }
}

function makeDeck(deckId: string): HymnPresentationItemData[] {
  return [makeSlide(deckId, 0), makeSlide(deckId, 1), makeSlide(deckId, 2)]
}

function makeQueuedHymnItem(
  deck: HymnPresentationItemData[],
  index: number
): QueueItem {
  return {
    id: `queue-${index}`,
    presentation: deck[index],
    confidence: 1,
    source: "hymn",
    added_at: 1,
    hymnGroup: {
      groupId: "queued-group",
      groupLabel: "Queued Hymn - 3 screens",
      itemIndex: index + 1,
      itemCount: deck.length,
    },
    hymnDeck: deck,
  }
}

describe("queued hymn deck presentation", () => {
  beforeEach(() => {
    useBroadcastStore.setState({
      isLive: false,
      previewItem: null,
      liveItem: null,
    })
    useHymnSlideStore.getState().setDeck([], 0)
    useQueueStore.setState({
      items: [],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })
  })

  it("restores a queued hymn deck before previewing an older queued slide", () => {
    const queuedDeck = makeDeck("queued")
    const currentDeck = makeDeck("current")
    const queuedItem = makeQueuedHymnItem(queuedDeck, 1)
    useHymnSlideStore.getState().setDeck(currentDeck, 0)

    previewQueuedItem(queuedItem)

    expect(
      useHymnSlideStore.getState().deck.map((slide) => slide.screenId)
    ).toEqual(queuedDeck.map((slide) => slide.screenId))
    expect(useHymnSlideStore.getState().activeIndex).toBe(1)
    expect(useBroadcastStore.getState().previewItem?.hymnSlide?.screenId).toBe(
      "queued-screen-1"
    )
  })

  it("previews the last slide of a queued hymn deck when moving backward", () => {
    const queuedDeck = makeDeck("queued")
    const currentDeck = makeDeck("current")
    const queuedItem = makeQueuedHymnItem(queuedDeck, 0)
    useHymnSlideStore.getState().setDeck(currentDeck, 0)

    previewQueuedItemAtEnd(queuedItem)

    expect(
      useHymnSlideStore.getState().deck.map((slide) => slide.screenId)
    ).toEqual(queuedDeck.map((slide) => slide.screenId))
    expect(useHymnSlideStore.getState().activeIndex).toBe(2)
    expect(useBroadcastStore.getState().previewItem?.hymnSlide?.screenId).toBe(
      "queued-screen-2"
    )
    expect(useBroadcastStore.getState().liveItem).toBeNull()
  })

  it("resolves deck controls from the active queued hymn deck", () => {
    const queuedDeck = makeDeck("queued")
    const currentDeck = makeDeck("current")
    const activeQueueItem = makeQueuedHymnItem(queuedDeck, 0)
    useQueueStore.setState({
      items: [activeQueueItem],
      activeIndex: 0,
    })
    useHymnSlideStore.getState().setDeck(currentDeck, 0)

    const renderItem = getPresentationRenderData(queuedDeck[2])

    expect(getQueuedHymnDeckForRenderItem(renderItem)).toBe(queuedDeck)
    expect(restoreQueuedHymnDeckForRenderItem(renderItem)).toBe(true)
    expect(useHymnSlideStore.getState().deck).toBe(queuedDeck)
    expect(useHymnSlideStore.getState().activeIndex).toBe(2)
  })

  it("previews and presents queued video material through the broadcast store", () => {
    const videoItem: QueueItem = {
      id: "queue-video",
      confidence: 1,
      source: "manual",
      added_at: 1,
      presentation: {
        kind: "video",
        videoId: "video-queue",
        title: "Welcome Video",
        source: "url",
        url: "https://cdn.example.com/welcome.mp4",
        reference: "Welcome Video",
        segments: [{ text: "Welcome Video" }],
      },
    }

    previewQueuedItem(videoItem)
    expect(useBroadcastStore.getState().previewItem).toMatchObject({
      kind: "video",
      reference: "Welcome Video",
    })

    presentQueuedItem(videoItem)
    expect(useBroadcastStore.getState().isLive).toBe(true)
    expect(useBroadcastStore.getState().liveQueueItemId).toBe("queue-video")
    expect(useBroadcastStore.getState().liveItem).toMatchObject({
      kind: "video",
      reference: "Welcome Video",
    })
  })
})
