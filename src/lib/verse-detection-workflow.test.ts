import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { handleReadingAdvance, handleVerseDetections } from "./verse-detection-workflow"
import { useBibleStore } from "@/stores/bible-store"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useDetectionStore } from "@/stores/detection-store"
import { useQueueStore } from "@/stores/queue-store"
import type { DetectionResult, QueueItem, ReadingAdvance } from "@/types"

const { emitToMock } = vi.hoisted(() => ({
  emitToMock: vi.fn(),
}))

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: emitToMock,
}))

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))

vi.mock("@tauri-apps/plugin-store", () => ({
  load: vi.fn(),
}))

function makeDetection(
  overrides: Partial<DetectionResult> = {}
): DetectionResult {
  return {
    verse_ref: "John 3:16",
    verse_text: "For God so loved the world",
    book_name: "John",
    book_number: 43,
    chapter: 3,
    verse: 16,
    confidence: 0.96,
    source: "direct",
    auto_queued: true,
    transcript_snippet: "John three sixteen",
    is_chapter_only: false,
    ...overrides,
  }
}

function makeQueueItem(overrides: Partial<QueueItem> = {}): QueueItem {
  return {
    id: "chapter-hit",
    reference: "John 3",
    verse: {
      id: 0,
      translation_id: 7,
      book_number: 43,
      book_name: "John",
      book_abbreviation: "",
      chapter: 3,
      verse: 1,
      text: "Chapter start",
    },
    confidence: 0.9,
    source: "ai-direct",
    added_at: 100,
    is_chapter_only: true,
    ...overrides,
  }
}

function makeReadingAdvance(
  overrides: Partial<ReadingAdvance> = {}
): ReadingAdvance {
  return {
    book_number: 43,
    book_name: "John",
    chapter: 3,
    verse: 17,
    verse_text: "For God sent not his Son",
    reference: "John 3:17",
    confidence: 1,
    ...overrides,
  }
}

describe("verse detection workflow", () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-05-19T00:00:00Z"))
    vi.stubGlobal("crypto", {
      randomUUID: vi.fn(() => "detection-id"),
    })
    emitToMock.mockReset()
    emitToMock.mockResolvedValue(undefined)

    useBibleStore.setState({
      translations: [],
      activeTranslationId: 7,
      books: [],
      searchResults: [],
      semanticResults: [],
      selectedVerse: null,
      currentChapter: [],
      crossReferences: [],
      pendingNavigation: null,
    })
    useDetectionStore.setState({
      detections: [],
      autoMode: false,
      confidenceThreshold: 0.8,
    })
    useQueueStore.setState({
      items: [],
      activeIndex: null,
      highlightedId: null,
    })
    useBroadcastStore.setState({
      isLive: false,
      liveVerse: null,
      readingModeAutoLive: true,
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  it("selects a direct verse hit for preview and pending navigation", () => {
    handleVerseDetections([makeDetection({ auto_queued: false })])

    expect(useDetectionStore.getState().detections).toHaveLength(1)
    expect(useQueueStore.getState().items).toHaveLength(0)
    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      translation_id: 7,
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 16,
      text: "For God so loved the world",
    })
    expect(useBibleStore.getState().pendingNavigation).toEqual({
      bookNumber: 43,
      chapter: 3,
      verse: 16,
    })
  })

  it("queues an auto-queued direct detection with the active translation", () => {
    handleVerseDetections([makeDetection()])

    expect(useQueueStore.getState().items).toEqual([
      expect.objectContaining({
        id: "detection-id",
        reference: "John 3:16",
        confidence: 0.96,
        source: "ai-direct",
        added_at: Date.now(),
        is_chapter_only: false,
        verse: expect.objectContaining({
          translation_id: 7,
          book_number: 43,
          chapter: 3,
          verse: 16,
          text: "For God so loved the world",
        }),
      }),
    ])
  })

  it("refines a chapter-only queue item instead of adding a duplicate verse", () => {
    useQueueStore.setState({
      items: [makeQueueItem()],
      activeIndex: null,
      highlightedId: null,
    })

    handleVerseDetections([makeDetection()])

    expect(useQueueStore.getState().items).toHaveLength(1)
    expect(useQueueStore.getState().items[0]).toMatchObject({
      id: "chapter-hit",
      reference: "John 3:16",
      is_chapter_only: false,
      verse: expect.objectContaining({
        verse: 16,
        text: "For God so loved the world",
      }),
    })
  })

  it("uses reading-mode advances for preview/navigation without queueing", () => {
    handleReadingAdvance(makeReadingAdvance())

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 43,
      chapter: 3,
      verse: 17,
      text: "For God sent not his Son",
    })
    expect(useBibleStore.getState().pendingNavigation).toEqual({
      bookNumber: 43,
      chapter: 3,
      verse: 17,
    })
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("ignores invalid reading-mode advances", () => {
    handleReadingAdvance(makeReadingAdvance({ book_number: 0 }))

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBibleStore.getState().pendingNavigation).toBeNull()
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("does not auto-live normal direct detections", () => {
    const detection = {
      verse_ref: "John 3:16",
      verse_text: "For God so loved the world.",
      book_name: "John",
      book_number: 43,
      chapter: 3,
      verse: 16,
      confidence: 0.95,
      source: "direct" as const,
      auto_queued: false,
      transcript_snippet: "John 3:16",
      is_chapter_only: false,
    }

    useBroadcastStore.setState({
      isLive: true,
      liveVerse: {
        reference: "Romans 8:1 (KJV)",
        segments: [{ verseNumber: 1, text: "There is therefore now no condemnation." }],
      },
    })

    handleVerseDetections([detection])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 16,
    })
    expect(useBroadcastStore.getState().liveVerse?.reference).toBe("Romans 8:1 (KJV)")
  })

  it("auto-updates live output for reading mode when already live", () => {
    useBroadcastStore.setState({
      isLive: true,
      readingModeAutoLive: true,
      liveVerse: {
        reference: "John 3:16 (KJV)",
        segments: [{ verseNumber: 16, text: "For God so loved the world." }],
      },
    })

    handleReadingAdvance({
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 17,
      verse_text: "For God sent not his Son into the world to condemn the world.",
      reference: "John 3:17",
      confidence: 0.9,
    })

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 17,
    })
    expect(useBroadcastStore.getState().liveVerse?.reference).toBe("John 3:17 (KJV)")
    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({ verse: expect.objectContaining({ reference: "John 3:17 (KJV)" }) }),
    )
  })

  it("does not auto-update live output for reading mode when the toggle is off", () => {
    useBroadcastStore.setState({
      isLive: true,
      readingModeAutoLive: false,
      liveVerse: {
        reference: "John 3:16 (KJV)",
        segments: [{ verseNumber: 16, text: "For God so loved the world." }],
      },
    })

    handleReadingAdvance({
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 17,
      verse_text: "For God sent not his Son into the world to condemn the world.",
      reference: "John 3:17",
      confidence: 0.9,
    })

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 17,
    })
    expect(useBroadcastStore.getState().liveVerse?.reference).toBe("John 3:16 (KJV)")
  })

  it("does not turn live output on for reading mode when hidden", () => {
    useBroadcastStore.setState({
      isLive: false,
      liveVerse: null,
    })

    handleReadingAdvance({
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 17,
      verse_text: "For God sent not his Son into the world to condemn the world.",
      reference: "John 3:17",
      confidence: 0.9,
    })

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 17,
    })
    expect(useBroadcastStore.getState().isLive).toBe(false)
    expect(useBroadcastStore.getState().liveVerse).toBeNull()
  })
})
