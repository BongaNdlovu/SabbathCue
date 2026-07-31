import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  aiSuggestionSettledForTests,
  handleReadingAdvance,
  handleVerseDetections,
  pendingSemanticConfirmationCountForTests,
  resetSemanticConfirmationForTests,
} from "./verse-detection-workflow"
import { useBibleStore } from "@/stores/bible-store"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useDetectionStore } from "@/stores/detection-store"
import { useEgwSlideStore } from "@/stores/egw-slide-store"
import { useQueueStore } from "@/stores/queue-store"
import { useSettingsStore } from "@/stores/settings-store"
import type { DetectionResult, QueueItem, ReadingAdvance } from "@/types"

const { emitToMock, invokeMock, scheduleRankingMock } = vi.hoisted(() => ({
  emitToMock: vi.fn(),
  invokeMock: vi.fn(),
  scheduleRankingMock: vi.fn(),
}))

vi.mock("@/lib/deepseek-ranker", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/deepseek-ranker")>()),
  scheduleRanking: scheduleRankingMock,
}))

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: emitToMock,
}))

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
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
    presentation: {
      kind: "scripture",
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
      reference: "John 3",
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
    invokeMock.mockReset()
    invokeMock.mockResolvedValue(null)
    scheduleRankingMock.mockReset()
    scheduleRankingMock.mockResolvedValue(null)
    resetSemanticConfirmationForTests()

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
      aiSuggestedKey: null,
    })
    useQueueStore.setState({
      items: [],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })
    useEgwSlideStore.setState({
      deck: [],
      activeIndex: 0,
    })
    useBroadcastStore.setState({
      isLive: false,
      liveItem: null,
      previewItem: null,
      readingModeAutoLive: true,
    })
    useSettingsStore.setState({
      autoMode: true,
      semanticDetectionEnabled: true,
      confidenceThreshold: 0.85,
      semanticConfidenceThreshold: 0.7,
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  it("selects a direct verse hit for preview without pending navigation", async () => {
    await handleVerseDetections([makeDetection({ auto_queued: false })])

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
    expect(useBibleStore.getState().pendingNavigation).toBeNull()
  })

  it("queues an auto-queued direct detection with the active translation", async () => {
    useSettingsStore.setState({ autoMode: false })
    await handleVerseDetections([makeDetection()])

    expect(useQueueStore.getState().items).toEqual([
      expect.objectContaining({
        id: "detection-id",
        confidence: 0.96,
        source: "ai-direct",
        added_at: Date.now(),
        is_chapter_only: false,
        presentation: expect.objectContaining({
          reference: "John 3:16",
          verse: expect.objectContaining({
            translation_id: 7,
            book_number: 43,
            chapter: 3,
            verse: 16,
            text: "For God so loved the world",
          }),
        }),
      }),
    ])
  })

  it("keeps detection and queueing automatic when auto mode is off", async () => {
    useSettingsStore.setState({ autoMode: false })

    await handleVerseDetections([makeDetection()])

    expect(useDetectionStore.getState().detections).toHaveLength(1)
    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().previewItem).toBeNull()
    expect(useQueueStore.getState().items).toEqual([
      expect.objectContaining({
        source: "ai-direct",
        presentation: expect.objectContaining({
          reference: "John 3:16",
        }),
      }),
    ])
  })

  it("does not auto-preview direct detections below the settings threshold", async () => {
    useSettingsStore.setState({ confidenceThreshold: 0.85 })
    await handleVerseDetections([makeDetection({ confidence: 0.84 })])

    expect(useDetectionStore.getState().detections).toHaveLength(1)
    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().previewItem).toBeNull()
    // Auto mode stages to preview; below-threshold detections still do not queue.
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("auto-previews direct detections at the settings threshold", async () => {
    useSettingsStore.setState({ confidenceThreshold: 0.85 })
    await handleVerseDetections([
      makeDetection({ confidence: 0.85, auto_queued: false }),
    ])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 43,
      chapter: 3,
      verse: 16,
    })
  })

  it("does not auto-preview semantic Bible detections below the auto-live threshold", async () => {
    useSettingsStore.setState({ confidenceThreshold: 0.85 })
    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        verse_ref: "Daniel 7:10",
        verse_text: "The court was seated, and the books were opened.",
        book_name: "Daniel",
        book_number: 27,
        chapter: 7,
        verse: 10,
        confidence: 0.84,
        auto_queued: false,
        transcript_snippet: "the court was seated and the books were opened",
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().liveItem).toBeNull()
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("ignores semantic detections when semantic detection is disabled", async () => {
    useSettingsStore.setState({
      autoMode: false,
      semanticDetectionEnabled: false,
    })

    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        confidence: 1,
        transcript_snippet: "God loved the world and gave his son",
      }),
    ])

    expect(useDetectionStore.getState().detections).toHaveLength(0)
    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("requires repeated semantic evidence at the auto-live threshold", async () => {
    useSettingsStore.setState({ confidenceThreshold: 0.85 })
    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        verse_ref: "Daniel 7:10",
        verse_text: "The court was seated, and the books were opened.",
        book_name: "Daniel",
        book_number: 27,
        chapter: 7,
        verse: 10,
        confidence: 0.85,
        auto_queued: false,
        transcript_snippet: "the court was seated and the books were opened",
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()

    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        verse_ref: "Daniel 7:10",
        verse_text: "The court was seated, and the books were opened.",
        book_name: "Daniel",
        book_number: 27,
        chapter: 7,
        verse: 10,
        confidence: 0.85,
        auto_queued: false,
        transcript_snippet: "the court was seated and the books were opened",
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 27,
      chapter: 7,
      verse: 10,
    })
    expect(useBroadcastStore.getState().liveItem?.reference).toBe(
      "Daniel 7:10 (KJV)"
    )
  })

  it("holds repeated semantic candidates when the runner-up is too close", async () => {
    const strongest = makeDetection({
      source: "semantic",
      verse_ref: "Revelation 21:4",
      verse_text: "And God shall wipe away all tears from their eyes.",
      book_name: "Revelation",
      book_number: 66,
      chapter: 21,
      verse: 4,
      confidence: 0.92,
      rank_score: 0.92,
      auto_queued: false,
    })
    const runnerUp = makeDetection({
      source: "semantic",
      verse_ref: "Revelation 7:17",
      verse_text: "God shall wipe away all tears from their eyes.",
      book_name: "Revelation",
      book_number: 66,
      chapter: 7,
      verse: 17,
      confidence: 0.92,
      rank_score: 0.92,
      auto_queued: false,
    })

    await handleVerseDetections([strongest, runnerUp])
    await handleVerseDetections([strongest, runnerUp])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().liveItem).toBeNull()
  })

  it("confirms a semantic verse after an intervening semantic candidate", async () => {
    const first = makeDetection({
      source: "semantic",
      verse_ref: "Daniel 7:10",
      verse_text: "The court was seated, and the books were opened.",
      book_name: "Daniel",
      book_number: 27,
      chapter: 7,
      verse: 10,
      confidence: 0.85,
      auto_queued: false,
    })
    const intervening = makeDetection({
      source: "semantic",
      verse_ref: "Romans 8:1",
      verse_text: "There is therefore now no condemnation.",
      book_name: "Romans",
      book_number: 45,
      chapter: 8,
      verse: 1,
      confidence: 0.85,
      auto_queued: false,
    })

    await handleVerseDetections([first])
    vi.advanceTimersByTime(1_000)
    await handleVerseDetections([intervening])
    vi.advanceTimersByTime(1_000)
    await handleVerseDetections([first])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 27,
      chapter: 7,
      verse: 10,
    })
  })

  it("prunes expired pending semantic confirmations instead of accumulating them", async () => {
    const unconfirmed = [
      {
        verse_ref: "Daniel 7:10",
        book_name: "Daniel",
        book_number: 27,
        chapter: 7,
        verse: 10,
      },
      {
        verse_ref: "Romans 8:1",
        book_name: "Romans",
        book_number: 45,
        chapter: 8,
        verse: 1,
      },
      {
        verse_ref: "Isaiah 40:8",
        book_name: "Isaiah",
        book_number: 23,
        chapter: 40,
        verse: 8,
      },
      {
        verse_ref: "Psalm 46:1",
        book_name: "Psalms",
        book_number: 19,
        chapter: 46,
        verse: 1,
      },
    ]
    for (const v of unconfirmed) {
      await handleVerseDetections([
        makeDetection({
          source: "semantic",
          confidence: 0.85,
          auto_queued: false,
          ...v,
        }),
      ])
    }
    expect(pendingSemanticConfirmationCountForTests()).toBe(unconfirmed.length)

    // Let every pending entry age out past the confirmation window.
    vi.advanceTimersByTime(9_000)

    // A fresh weak semantic detection must evict the expired keys, not pile on.
    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        confidence: 0.85,
        auto_queued: false,
        verse_ref: "Micah 6:8",
        book_name: "Micah",
        book_number: 33,
        chapter: 6,
        verse: 8,
      }),
    ])

    expect(pendingSemanticConfirmationCountForTests()).toBe(1)
  })

  it("auto-previews one exceptionally strong semantic detection", async () => {
    await handleVerseDetections([
      makeDetection({ source: "semantic", confidence: 0.95 }),
    ])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 43,
      chapter: 3,
      verse: 16,
    })
  })

  it("does not auto-queue detections while auto mode is staging preview", async () => {
    await handleVerseDetections([makeDetection()])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 43,
      chapter: 3,
      verse: 16,
    })
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("queues semantic detections without pending navigation", async () => {
    useSettingsStore.setState({ autoMode: false })
    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        confidence: 0.72,
        transcript_snippet: "God loved the world and gave his son",
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBibleStore.getState().pendingNavigation).toBeNull()
    expect(useQueueStore.getState().items).toEqual([
      expect.objectContaining({
        source: "ai-semantic",
        confidence: 0.72,
        presentation: expect.objectContaining({
          reference: "John 3:16",
        }),
      }),
    ])
  })

  it("previews a direct hit over a stronger semantic suggestion", async () => {
    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        verse_ref: "Romans 8:28",
        verse_text: "All things work together for good",
        book_name: "Romans",
        book_number: 45,
        chapter: 8,
        verse: 28,
        confidence: 0.99,
        transcript_snippet: "all things work together for good",
      }),
      makeDetection({
        auto_queued: false,
        confidence: 0.86,
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 43,
      chapter: 3,
      verse: 16,
    })
    // Direct hit wins the preview; with auto-preview on, the semantic
    // suggestion is not auto-queued.
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("keeps non-auto-queued semantic detections out of the queue", async () => {
    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        auto_queued: false,
        confidence: 0.79,
      }),
    ])

    expect(useDetectionStore.getState().detections).toHaveLength(1)
    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("queues chapter-only direct detections without selecting preview", async () => {
    useSettingsStore.setState({ autoMode: false })
    await handleVerseDetections([
      makeDetection({
        verse_ref: "John 3",
        verse: 1,
        verse_text: "Chapter start",
        transcript_snippet: "John chapter three",
        is_chapter_only: true,
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useQueueStore.getState().items).toEqual([
      expect.objectContaining({
        source: "ai-direct",
        is_chapter_only: true,
        presentation: expect.objectContaining({
          reference: "John 3",
          verse: expect.objectContaining({
            book_number: 43,
            chapter: 3,
            verse: 1,
          }),
        }),
      }),
    ])
  })

  it("refines a chapter-only queue item instead of adding a duplicate verse", async () => {
    useSettingsStore.setState({ autoMode: false })
    useQueueStore.setState({
      items: [makeQueueItem()],
      activeIndex: null,
      highlightedId: null,
    })

    await handleVerseDetections([makeDetection()])

    expect(useQueueStore.getState().items).toHaveLength(1)
    expect(useQueueStore.getState().items[0]).toMatchObject({
      id: "chapter-hit",
      is_chapter_only: false,
      presentation: expect.objectContaining({
        reference: "John 3:16",
        verse: expect.objectContaining({
          verse: 16,
          text: "For God so loved the world",
        }),
      }),
    })
  })

  it("uses reading-mode advances for preview without queueing or navigation", () => {
    handleReadingAdvance(makeReadingAdvance())

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 43,
      chapter: 3,
      verse: 17,
      text: "For God sent not his Son",
    })
    expect(useBibleStore.getState().pendingNavigation).toBeNull()
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("surfaces reading-mode advances in the detections panel", () => {
    handleReadingAdvance(makeReadingAdvance())

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0]).toMatchObject({
      verse_ref: "John 3:17",
      book_number: 43,
      chapter: 3,
      verse: 17,
      source: "direct",
    })
  })

  it("does not surface reading-mode advances as detections in manual mode", () => {
    useSettingsStore.setState({ autoMode: false })
    handleReadingAdvance(makeReadingAdvance())

    expect(useDetectionStore.getState().detections).toHaveLength(0)
  })

  it("ignores invalid reading-mode advances", () => {
    handleReadingAdvance(makeReadingAdvance({ book_number: 0 }))

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBibleStore.getState().pendingNavigation).toBeNull()
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("ignores reading-mode advances in manual broadcast mode", () => {
    useSettingsStore.setState({ autoMode: false })
    handleReadingAdvance(makeReadingAdvance())

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().previewItem).toBeNull()
  })

  it("does not auto-live direct detections when the toggle is off", async () => {
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
      readingModeAutoLive: false,
      liveItem: {
        reference: "Romans 8:1 (KJV)",
        segments: [
          { verseNumber: 1, text: "There is therefore now no condemnation." },
        ],
      },
    })

    await handleVerseDetections([detection])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 16,
    })
    expect(useBroadcastStore.getState().liveItem?.reference).toBe(
      "Romans 8:1 (KJV)"
    )
  })

  it("auto-lives a direct detection when the toggle is on", async () => {
    useBroadcastStore.setState({
      isLive: false,
      readingModeAutoLive: true,
      liveItem: null,
    })

    await handleVerseDetections([makeDetection({ auto_queued: false })])

    expect(useBroadcastStore.getState().isLive).toBe(true)
    expect(useBroadcastStore.getState().liveItem?.reference).toBe(
      "John 3:16 (KJV)"
    )
  })

  it("auto-updates live output for reading mode when already live", () => {
    useBroadcastStore.setState({
      isLive: true,
      readingModeAutoLive: true,
      liveItem: {
        reference: "John 3:16 (KJV)",
        segments: [{ verseNumber: 16, text: "For God so loved the world." }],
      },
    })

    handleReadingAdvance({
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 17,
      verse_text:
        "For God sent not his Son into the world to condemn the world.",
      reference: "John 3:17",
      confidence: 0.9,
    })

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 17,
    })
    expect(useBroadcastStore.getState().liveItem?.reference).toBe(
      "John 3:17 (KJV)"
    )
    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({
        item: expect.objectContaining({ reference: "John 3:17 (KJV)" }),
      })
    )
  })

  it("does not auto-update live output for reading mode when the toggle is off", () => {
    useBroadcastStore.setState({
      isLive: true,
      readingModeAutoLive: false,
      liveItem: {
        reference: "John 3:16 (KJV)",
        segments: [{ verseNumber: 16, text: "For God so loved the world." }],
      },
    })

    handleReadingAdvance({
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 17,
      verse_text:
        "For God sent not his Son into the world to condemn the world.",
      reference: "John 3:17",
      confidence: 0.9,
    })

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 17,
    })
    expect(useBroadcastStore.getState().liveItem?.reference).toBe(
      "John 3:16 (KJV)"
    )
  })

  it("turns live output on for reading mode when the toggle is on and hidden", () => {
    useBroadcastStore.setState({
      isLive: false,
      readingModeAutoLive: true,
      liveItem: null,
    })

    handleReadingAdvance({
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 17,
      verse_text:
        "For God sent not his Son into the world to condemn the world.",
      reference: "John 3:17",
      confidence: 0.9,
    })

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 17,
    })
    expect(useBroadcastStore.getState().isLive).toBe(true)
    expect(useBroadcastStore.getState().liveItem?.reference).toBe(
      "John 3:17 (KJV)"
    )
  })

  it("does not turn live output on for reading mode when the toggle is off", () => {
    useBroadcastStore.setState({
      isLive: false,
      readingModeAutoLive: false,
      liveItem: null,
    })

    handleReadingAdvance({
      book_number: 43,
      book_name: "John",
      chapter: 3,
      verse: 17,
      verse_text:
        "For God sent not his Son into the world to condemn the world.",
      reference: "John 3:17",
      confidence: 0.9,
    })

    expect(useBroadcastStore.getState().isLive).toBe(false)
    expect(useBroadcastStore.getState().liveItem).toBeNull()
  })

  it("previews from incoming direct detection event", async () => {
    const detection = makeDetection({
      verse_ref: "Romans 5:8",
      book_number: 45,
      chapter: 5,
      verse: 8,
    })
    await handleVerseDetections([detection])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 45,
      chapter: 5,
      verse: 8,
    })
  })

  it("previews the highest-confidence direct detection from the incoming batch", async () => {
    const detection1 = makeDetection({
      verse_ref: "Romans 5:8",
      book_number: 45,
      chapter: 5,
      verse: 8,
      confidence: 0.7,
    })
    const detection2 = makeDetection({
      verse_ref: "Romans 8:1",
      book_number: 45,
      chapter: 8,
      verse: 1,
      confidence: 0.95,
    })
    await handleVerseDetections([detection1, detection2])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 45,
      chapter: 8,
      verse: 1,
    })
  })

  it("previews EGW direct detections without selecting a Bible verse", async () => {
    await handleVerseDetections([
      makeDetection({
        content_type: "egw",
        verse_ref: "Patriarchs and Prophets p.29 par.2",
        verse_text: "The history of the great conflict.",
        book_name: "Patriarchs and Prophets",
        book_number: 1,
        chapter: 29,
        verse: 2,
        auto_queued: false,
        egw_paragraph: {
          id: 12,
          book_number: 1,
          book_title: "Patriarchs and Prophets",
          chapter: 1,
          chapter_title: "Why Was Sin Permitted?",
          paragraph: 2,
          page: 29,
          page_paragraph: 2,
          text: "The history of the great conflict.",
        },
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useEgwSlideStore.getState().deck[0]).toMatchObject({
      kind: "egw",
      reference: "Patriarchs and Prophets p.29 par.2",
    })
    expect(useBroadcastStore.getState().previewItem).toMatchObject({
      kind: "egw",
      reference: "Patriarchs and Prophets p.29 par.2",
    })
  })

  it("auto-lives auto-queued EGW direct detections", async () => {
    await handleVerseDetections([
      makeDetection({
        content_type: "egw",
        verse_ref: "Patriarchs and Prophets p.29 par.2",
        verse_text: "The history of the great conflict.",
        book_name: "Patriarchs and Prophets",
        book_number: 1,
        chapter: 29,
        verse: 2,
        auto_queued: true,
        egw_paragraph: {
          id: 12,
          book_number: 1,
          book_title: "Patriarchs and Prophets",
          chapter: 1,
          chapter_title: "Why Was Sin Permitted?",
          paragraph: 2,
          page: 29,
          page_paragraph: 2,
          text: "The history of the great conflict.",
        },
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().isLive).toBe(true)
    expect(useBroadcastStore.getState().liveItem).toMatchObject({
      kind: "egw",
      reference: "Patriarchs and Prophets p.29 par.2",
    })
    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({
        item: expect.objectContaining({
          kind: "egw",
          reference: "Patriarchs and Prophets p.29 par.2",
        }),
      })
    )
  })

  it("keeps the first direct hit when confidence is tied", async () => {
    const detection1 = makeDetection({
      verse_ref: "Romans 5:8",
      book_number: 45,
      chapter: 5,
      verse: 8,
      confidence: 0.9,
    })
    const detection2 = makeDetection({
      verse_ref: "Romans 8:1",
      book_number: 45,
      chapter: 8,
      verse: 1,
      confidence: 0.9,
    })
    await handleVerseDetections([detection1, detection2])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 45,
      chapter: 5,
      verse: 8,
    })
  })

  it("serializes overlapping detection batches", async () => {
    useSettingsStore.setState({ autoMode: false })
    const order: string[] = []
    invokeMock.mockImplementation(async () => {
      order.push("fetch")
      return null
    })

    const first = handleVerseDetections([
      makeDetection({ verse_ref: "John 3:16", auto_queued: true }),
    ])
    const second = handleVerseDetections([
      makeDetection({
        verse_ref: "Romans 8:1",
        book_number: 45,
        chapter: 8,
        verse: 1,
        auto_queued: true,
      }),
    ])

    await Promise.all([first, second])
    expect(order.length).toBeGreaterThan(0)
    expect(useQueueStore.getState().items.length).toBeGreaterThanOrEqual(2)
  })

  it("reports a verse lookup issue when fetch fails and fallback text is used", async () => {
    useBroadcastStore.setState({ outputIssues: [] })
    invokeMock.mockRejectedValueOnce(new Error("network down"))

    await handleVerseDetections([
      makeDetection({ verse_text: "Fallback verse text" }),
    ])

    expect(useBroadcastStore.getState().outputIssues).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "verse-lookup",
          outputId: "global",
        }),
      ])
    )
  })

  it("reports unexpected detection batch errors instead of swallowing them", async () => {
    useBroadcastStore.setState({ outputIssues: [] })
    const originalAddDetections = useDetectionStore.getState().addDetections
    useDetectionStore.setState({
      addDetections: () => {
        throw new Error("batch exploded")
      },
    })

    await handleVerseDetections([makeDetection()])

    expect(useBroadcastStore.getState().outputIssues).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          outputId: "global",
          kind: "auto-detection",
          title: "Detection batch failed",
        }),
      ])
    )

    useDetectionStore.setState({ addDetections: originalAddDetections })
  })

  it("queues text fetched from the current translation", async () => {
    useSettingsStore.setState({ autoMode: false })
    invokeMock.mockResolvedValueOnce({
      id: 25,
      translation_id: 7,
      book_number: 43,
      book_name: "John",
      book_abbreviation: "John",
      chapter: 3,
      verse: 16,
      text: "Current translation text",
    })

    await handleVerseDetections([
      makeDetection({ verse_text: "Text from the earlier translation" }),
    ])

    const presentation = useQueueStore.getState().items[0].presentation
    expect(
      presentation.kind === "scripture" ? presentation.verse.text : null
    ).toBe("Current translation text")
  })

  it("previews text fetched from the current translation", async () => {
    invokeMock.mockResolvedValue({
      id: 25,
      translation_id: 7,
      book_number: 43,
      book_name: "John",
      book_abbreviation: "John",
      chapter: 3,
      verse: 16,
      text: "Current translation preview text",
    })

    await handleVerseDetections([
      makeDetection({
        auto_queued: false,
        verse_text: "Detection event text",
      }),
    ])

    expect(useBibleStore.getState().selectedVerse?.text).toBe(
      "Current translation preview text"
    )
    expect(useBroadcastStore.getState().previewItem?.segments[0]?.text).toBe(
      "Current translation preview text"
    )
  })

  it("falls back to loaded current chapter text when verse fetch is unavailable", async () => {
    useSettingsStore.setState({ autoMode: false })
    useBibleStore.setState({
      currentChapter: [
        {
          id: 25,
          translation_id: 7,
          book_number: 43,
          book_name: "John",
          book_abbreviation: "John",
          chapter: 3,
          verse: 16,
          text: "Loaded current chapter text",
        },
      ],
    })

    await handleVerseDetections([
      makeDetection({ verse_text: "Text from the earlier translation" }),
    ])

    const presentation = useQueueStore.getState().items[0].presentation
    expect(
      presentation.kind === "scripture" ? presentation.verse.text : null
    ).toBe("Loaded current chapter text")
  })

  describe("AI suggestion wiring", () => {
    function makeSemantic(
      overrides: Partial<DetectionResult> = {}
    ): DetectionResult {
      return makeDetection({
        source: "semantic",
        confidence: 0.78,
        auto_queued: false,
        verse_ref: "Acts 16:25",
        verse_text: "And at midnight Paul and Silas prayed",
        book_name: "Acts",
        book_number: 44,
        chapter: 16,
        verse: 25,
        transcript_snippet: "the passage where paul and silas sang in prison",
        ...overrides,
      })
    }

    it("marks the ranked detection as AI-suggested", async () => {
      const winner = makeSemantic()
      scheduleRankingMock.mockResolvedValue(winner)

      await handleVerseDetections([
        winner,
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])
      await aiSuggestionSettledForTests()

      expect(useDetectionStore.getState().aiSuggestedKey).toBe("44:16:25")
    })

    it("clears the marker when ranking abstains", async () => {
      useDetectionStore.getState().markAiSuggested("44:16:25")
      scheduleRankingMock.mockResolvedValue(null)

      await handleVerseDetections([
        makeSemantic(),
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])
      await aiSuggestionSettledForTests()

      expect(useDetectionStore.getState().aiSuggestedKey).toBeNull()
    })

    it("does not let a ranker failure break the detection batch", async () => {
      scheduleRankingMock.mockRejectedValue(new Error("network down"))

      await handleVerseDetections([makeDetection({ auto_queued: false })])
      await aiSuggestionSettledForTests()

      // The direct hit still reached preview despite the ranker throwing.
      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 43,
        chapter: 3,
        verse: 16,
      })
      expect(useDetectionStore.getState().aiSuggestedKey).toBeNull()
    })

    it("a stale in-flight ranking cannot overwrite a newer batch's badge", async () => {
      const winner = makeSemantic()
      let resolveFirstFlight: (value: DetectionResult | null) => void = () => {}
      scheduleRankingMock.mockReturnValueOnce(
        new Promise<DetectionResult | null>((resolve) => {
          resolveFirstFlight = resolve
        })
      )

      // Batch A: ranking flight hangs (network in progress).
      await handleVerseDetections([
        winner,
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])
      // Batch B: ranker abstains immediately, clearing the badge.
      await handleVerseDetections([
        makeSemantic({ verse_ref: "Acts 16:31", verse: 31 }),
      ])
      await aiSuggestionSettledForTests()
      expect(useDetectionStore.getState().aiSuggestedKey).toBeNull()

      // Batch A's flight finally resolves with a winner computed from older
      // speech — it must not resurrect the badge batch B cleared.
      resolveFirstFlight(winner)
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()

      expect(useDetectionStore.getState().aiSuggestedKey).toBeNull()
    })

    it("does not influence which detection is previewed", async () => {
      // Ranker picks the semantic Acts hit, but a strong direct John hit is
      // present: preview must still follow the deterministic direct path.
      scheduleRankingMock.mockResolvedValue(makeSemantic())

      await handleVerseDetections([
        makeDetection({ auto_queued: false }),
        makeSemantic(),
      ])
      await aiSuggestionSettledForTests()

      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 43,
        chapter: 3,
        verse: 16,
      })
      expect(useDetectionStore.getState().aiSuggestedKey).toBe("44:16:25")
    })
  })
})
