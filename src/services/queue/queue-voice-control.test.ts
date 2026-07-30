// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  handleQueueItemVoiceControl,
  parseQueueItemCommand,
  resetQueueVoiceControlState,
} from "./queue-voice-control"
import { presentQueuedItem } from "@/lib/queue-presentation"
import { useQueueStore } from "@/stores/queue-store"
import type { PresentationItem, QueueItem } from "@/types"

vi.mock("@/lib/queue-presentation", () => ({
  presentQueuedItem: vi.fn(),
}))

function queueItem(id: string, presentation: PresentationItem): QueueItem {
  return {
    id,
    presentation,
    confidence: 1,
    source: "manual",
    added_at: 1,
  }
}

const PRESENTATIONS: PresentationItem[] = [
  {
    kind: "scripture",
    reference: "John 3:16",
    verse: {
      id: 1,
      translation_id: 1,
      book_number: 43,
      book_name: "John",
      book_abbreviation: "Jn",
      chapter: 3,
      verse: 16,
      text: "For God so loved the world",
    },
  },
  {
    kind: "hymn",
    hymnId: "hymn-1",
    hymnNumber: 1,
    hymnTitle: "Praise",
    screenId: "screen-1",
    slideIndex: 0,
    slideCount: 1,
    reference: "Hymn 1",
    segments: [{ text: "Praise" }],
  },
  {
    kind: "media",
    mediaId: "media-1",
    title: "Welcome",
    mediaKind: "media",
    reference: "Welcome",
    segments: [],
  },
  {
    kind: "slideDeck",
    deckId: "deck-1",
    deckTitle: "Sermon",
    slideId: "slide-1",
    slideIndex: 0,
    slideCount: 1,
    slidePath: "slide.png",
    reference: "Sermon - Slide 1",
    segments: [],
  },
  {
    kind: "egw",
    paragraph: {
      id: 1,
      book_number: 1,
      book_title: "Steps to Christ",
      chapter: 1,
      chapter_title: "God's Love for Man",
      paragraph: 1,
      page: 1,
      page_paragraph: 1,
      text: "Example",
    },
    reference: "SC 1.1",
    segments: [{ text: "Example" }],
    slideId: "egw-1",
    slideIndex: 0,
    slideCount: 1,
  },
  {
    kind: "video",
    videoId: "video-1",
    title: "Announcements",
    source: "local",
    videoPath: "announcements.mp4",
    reference: "Announcements",
    segments: [],
  },
]

describe("queue item voice control", () => {
  beforeEach(() => {
    resetQueueVoiceControlState()
    vi.mocked(presentQueuedItem).mockReset()
    useQueueStore.setState({
      items: PRESENTATIONS.map((presentation, index) =>
        queueItem(`item-${index + 1}`, presentation)
      ),
      activeIndex: null,
    })
  })

  it.each([
    ["item 1", 1],
    ["item number 2", 2],
    ["ITEM TWO", 2],
    ["show item three", 3],
    ["present item number four", 4],
    ["go to item five", 5],
    ["please display item six.", 6],
  ])("parses %s", (text, expected) => {
    expect(parseQueueItemCommand(text)).toBe(expected)
  })

  it.each([
    "",
    "item",
    "item 0",
    "item -1",
    "item one two",
    "item one in our discussion is faith",
    "the first item is prayer",
    "John 3:16",
  ])("rejects non-command text: %s", (text) => {
    expect(parseQueueItemCommand(text)).toBeNull()
  })

  it("presents every supported queue presentation kind by current position", () => {
    for (let itemNumber = 1; itemNumber <= PRESENTATIONS.length; itemNumber += 1) {
      resetQueueVoiceControlState()
      expect(handleQueueItemVoiceControl(`item ${itemNumber}`)).toBe(true)
      expect(useQueueStore.getState().activeIndex).toBe(itemNumber - 1)
      expect(presentQueuedItem).toHaveBeenLastCalledWith(
        useQueueStore.getState().items[itemNumber - 1]
      )
    }
  })

  it("uses the queue order at execution time", () => {
    useQueueStore.getState().reorderItems(5, 0)

    expect(handleQueueItemVoiceControl("item 1")).toBe(true)
    expect(vi.mocked(presentQueuedItem).mock.calls[0][0].id).toBe("item-6")
  })

  it("ignores empty and out-of-range positions", () => {
    expect(handleQueueItemVoiceControl("item 99")).toBe(false)
    expect(useQueueStore.getState().activeIndex).toBeNull()
    expect(presentQueuedItem).not.toHaveBeenCalled()
  })

  it("suppresses duplicate provider finals inside the guard window", () => {
    expect(handleQueueItemVoiceControl("item 2", 1000)).toBe(true)
    expect(handleQueueItemVoiceControl("item number two", 2000)).toBe(true)
    expect(presentQueuedItem).toHaveBeenCalledTimes(1)

    expect(handleQueueItemVoiceControl("item 2", 7000)).toBe(true)
    expect(presentQueuedItem).toHaveBeenCalledTimes(2)
  })
})
