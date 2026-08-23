import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  aiSuggestionSettledForTests,
  dropDigitPrefixLosers,
  handleReadingAdvance,
  handleVerseDetections,
  refineSemanticAutoLiveWinner,
  resetDetectionArbitrationForTests,
  resetSemanticConfirmationForTests,
  resetStableDirectCitationForTests,
  scheduleVerseDetections,
} from "./verse-detection-workflow"
import {
  DIGIT_GROWTH_HOLD_MS,
  resetDigitGrowthHoldForTests,
} from "./presentation-workflow"
import { useBibleStore } from "@/stores/bible-store"

/** Flush single-digit auto-live holds under fake timers. */
async function flushDigitGrowthHold() {
  await vi.advanceTimersByTimeAsync(DIGIT_GROWTH_HOLD_MS)
}
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
  const detection: DetectionResult = {
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
    is_fuzzy_book: false,
    has_lexical_quote: false,
    is_final_utterance: true,
    ...overrides,
  }
  if (!detection.authorization) {
    if (detection.source === "direct") {
      detection.authorization = detection.is_chapter_only
        ? "suggestion"
        : "live-authorized"
      detection.job = "citation"
    } else if (detection.content_type === "egw") {
      detection.authorization =
        detection.confidence >= 0.88 || detection.auto_queued
          ? "live-authorized"
          : "preview-authorized"
      detection.job = "quotation"
    } else {
      detection.job = "quotation"
      if (detection.confidence >= 0.85) {
        detection.authorization = "live-authorized"
      } else if (detection.confidence >= 0.7) {
        detection.authorization = "preview-authorized"
      } else {
        detection.authorization = "suggestion"
      }
    }
  }
  if (!detection.job) {
    detection.job = detection.source === "direct" ? "citation" : "quotation"
  }
  return detection
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
    resetDetectionArbitrationForTests()
    resetSemanticConfirmationForTests()
    resetStableDirectCitationForTests()
    resetDigitGrowthHoldForTests()

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
    resetDetectionArbitrationForTests()
    resetStableDirectCitationForTests()
    resetDigitGrowthHoldForTests()
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

  it("does not auto-preview unauthorized semantic Bible detections", async () => {
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
        authorization: "suggestion",
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
        authorization: "suggestion",
        transcript_snippet: "the court was seated and the books were opened",
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().liveItem).toBeNull()
    expect(invokeMock).not.toHaveBeenCalledWith(
      "set_reading_mode_reference",
      expect.anything()
    )
  })

  it("does not auto-preview semantic EGW below the shared panel floor", () => {
    // The detections panel hides semantic EGW below max(threshold, 0.75);
    // the workflow previously presented anything above the plain 0.70
    // threshold, so the live output could change while the detection card
    // was hidden. Both gates must agree.
    useSettingsStore.setState({
      autoMode: true,
      confidenceThreshold: 0.85,
      semanticDetectionEnabled: true,
      semanticConfidenceThreshold: 0.7,
    })

    const egw = makeDetection({
      content_type: "egw",
      verse_ref: "Patriarchs and Prophets p.322 par.1",
      book_name: "Patriarchs and Prophets",
      book_number: 1,
      chapter: 322,
      verse: 1,
      confidence: 0.72,
      source: "semantic",
      auto_queued: false,
      authorization: "preview-authorized",
      egw_paragraph: {
        id: 1,
        book_number: 1,
        book_title: "Patriarchs and Prophets",
        chapter: 1,
        chapter_title: "Why Was Sin Permitted?",
        paragraph: 1,
        page: 322,
        page_paragraph: 1,
        text: "Adam and Eve at their creation had a knowledge of the law of God.",
      },
    })
    void handleVerseDetections([egw])

    expect(useBroadcastStore.getState().liveItem).toBeNull()
    expect(useBroadcastStore.getState().previewItem).toBeNull()
    expect(useDetectionStore.getState().detections).toHaveLength(0)
  })

  it("previews an authorized quotation without starting reading mode", async () => {
    await handleVerseDetections([
      makeDetection({
        source: "semantic",
        verse_ref: "Daniel 7:10",
        verse_text: "The court was seated, and the books were opened.",
        book_name: "Daniel",
        book_number: 27,
        chapter: 7,
        verse: 10,
        confidence: 0.98,
        auto_queued: false,
        has_lexical_quote: true,
        authorization: "live-authorized",
        job: "quotation",
        is_final_utterance: true,
        utterance_id: 9,
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
    expect(invokeMock).not.toHaveBeenCalledWith(
      "set_reading_mode_reference",
      expect.anything()
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

  it("uses a visible runner-up below the auto-live threshold for ambiguity", async () => {
    useSettingsStore.setState({ confidenceThreshold: 0.9 })
    const strongest = makeDetection({
      source: "semantic",
      verse_ref: "Romans 8:39",
      book_name: "Romans",
      book_number: 45,
      chapter: 8,
      verse: 39,
      confidence: 0.91,
      rank_score: 0.91,
      auto_queued: false,
    })
    const runnerUp = makeDetection({
      source: "semantic",
      verse_ref: "Romans 8:38",
      book_name: "Romans",
      book_number: 45,
      chapter: 8,
      verse: 38,
      confidence: 0.891,
      rank_score: 0.891,
      auto_queued: false,
    })

    await handleVerseDetections([strongest, runnerUp])
    await handleVerseDetections([strongest, runnerUp])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().liveItem).toBeNull()
  })

  it("uses confidence before rerank score for semantic auto-live", async () => {
    useSettingsStore.setState({ confidenceThreshold: 0.9 })
    const strongest = makeDetection({
      source: "semantic",
      verse_ref: "Matthew 5:16",
      verse_text: "Let your light so shine before men",
      book_name: "Matthew",
      book_number: 40,
      chapter: 5,
      verse: 16,
      confidence: 0.93,
      rank_score: 0.80,
      auto_queued: false,
    })
    const runnerUp = makeDetection({
      source: "semantic",
      verse_ref: "Psalms 4:6",
      verse_text: "Lift thou up the light of thy countenance",
      book_name: "Psalms",
      book_number: 19,
      chapter: 4,
      verse: 6,
      confidence: 0.90,
      rank_score: 0.99,
      auto_queued: false,
    })

    await handleVerseDetections([strongest, runnerUp])
    await handleVerseDetections([strongest, runnerUp])

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 40,
      chapter: 5,
      verse: 16,
    })
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

  it("does not preview or queue unconfirmed weak semantic detections", async () => {
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
          confidence: 0.65,
          authorization: "suggestion",
          auto_queued: false,
          ...v,
        }),
      ])
    }
    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useQueueStore.getState().items).toHaveLength(0)
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

  it("keeps semantic quotations out of the auto-queue in manual mode", async () => {
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
    expect(useDetectionStore.getState().detections).toHaveLength(1)
    expect(useQueueStore.getState().items).toHaveLength(0)
  })

  it("previews a stronger semantic hit over a lower-confidence direct hit", async () => {
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
      book_number: 45,
      chapter: 8,
      verse: 28,
    })
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

  it("keeps chapter-only direct detections as suggestions without auto-queueing", async () => {
    useSettingsStore.setState({ autoMode: false })
    await handleVerseDetections([
      makeDetection({
        verse_ref: "John 3",
        verse: 1,
        verse_text: "Chapter start",
        transcript_snippet: "John chapter three",
        is_chapter_only: true,
        authorization: "suggestion",
      }),
    ])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useDetectionStore.getState().detections).toHaveLength(1)
    expect(useQueueStore.getState().items).toHaveLength(0)
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
    const detection = makeDetection({
      verse_ref: "John 3:16",
      verse_text: "For God so loved the world.",
      book_name: "John",
      book_number: 43,
      chapter: 3,
      verse: 16,
      confidence: 0.95,
      source: "direct",
      auto_queued: false,
      transcript_snippet: "John 3:16",
      is_chapter_only: false,
    })

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

  it("keeps live John 3:16 when semantic ranks adjacent 3:15 higher", () => {
    const john315 = makeDetection({
      verse_ref: "John 3:15",
      book_name: "John",
      book_number: 43,
      chapter: 3,
      verse: 15,
      confidence: 0.98,
      source: "semantic",
      auto_queued: false,
    })
    const john316 = makeDetection({
      verse_ref: "John 3:16",
      book_name: "John",
      book_number: 43,
      chapter: 3,
      verse: 16,
      confidence: 0.92,
      source: "semantic",
      auto_queued: false,
    })
    const winner = refineSemanticAutoLiveWinner(john315, [john315, john316], {
      book_number: 43,
      chapter: 3,
      verse: 16,
    })
    expect(winner?.verse_ref).toBe("John 3:16")
  })

  it("blocks adjacent semantic steal when live verse is missing from candidates", () => {
    const john315 = makeDetection({
      verse_ref: "John 3:15",
      book_name: "John",
      book_number: 43,
      chapter: 3,
      verse: 15,
      confidence: 0.98,
      source: "semantic",
      auto_queued: false,
    })
    const winner = refineSemanticAutoLiveWinner(john315, [john315], {
      book_number: 43,
      chapter: 3,
      verse: 16,
    })
    expect(winner).toBeNull()
  })

  it("auto-lives EGW fire-band hits on first sighting without double confirmation", async () => {
    useBroadcastStore.setState({
      isLive: false,
      readingModeAutoLive: true,
      liveItem: null,
    })
    const egw = makeDetection({
      content_type: "egw",
      verse_ref: "Patriarchs and Prophets p.322 par.1",
      book_name: "Patriarchs and Prophets",
      book_number: 1,
      chapter: 322,
      verse: 1,
      confidence: 0.92,
      source: "semantic",
      auto_queued: true,
      egw_paragraph: {
        id: 1,
        book_number: 1,
        book_title: "Patriarchs and Prophets",
        chapter: 1,
        chapter_title: "Why Was Sin Permitted?",
        paragraph: 1,
        page: 322,
        page_paragraph: 1,
        text: "Adam and Eve at their creation had a knowledge of the law of God.",
      },
    })
    await handleVerseDetections([egw])
    expect(useBroadcastStore.getState().isLive).toBe(true)
    expect(useBroadcastStore.getState().liveItem?.kind).toBe("egw")
  })

  it("prefers competitive EGW over Bible semantic noise", () => {
    const jeremiah = makeDetection({
      verse_ref: "Jeremiah 17:1",
      book_name: "Jeremiah",
      book_number: 24,
      chapter: 17,
      verse: 1,
      confidence: 0.87,
      source: "semantic",
      auto_queued: false,
    })
    const egw = makeDetection({
      content_type: "egw",
      verse_ref: "Patriarchs and Prophets p.322 par.1",
      book_name: "Patriarchs and Prophets",
      book_number: 1,
      chapter: 322,
      verse: 1,
      confidence: 0.92,
      source: "semantic",
      auto_queued: false,
      egw_paragraph: {
        id: 1,
        book_number: 1,
        book_title: "Patriarchs and Prophets",
        chapter: 1,
        chapter_title: "Why Was Sin Permitted?",
        paragraph: 1,
        page: 322,
        page_paragraph: 1,
        text: "Adam and Eve at their creation had a knowledge of the law of God.",
      },
    })
    const winner = refineSemanticAutoLiveWinner(jeremiah, [jeremiah, egw], null)
    expect(winner?.content_type).toBe("egw")
  })

  it("drops digit-prefix intermediate citations when the full form is present", () => {
    const kept = dropDigitPrefixLosers([
      makeDetection({
        verse_ref: "Matthew 6:3",
        book_name: "Matthew",
        book_number: 40,
        chapter: 6,
        verse: 3,
        confidence: 1,
      }),
      makeDetection({
        verse_ref: "Matthew 6:33",
        book_name: "Matthew",
        book_number: 40,
        chapter: 6,
        verse: 33,
        confidence: 1,
      }),
      makeDetection({
        verse_ref: "Luke 12:31",
        book_name: "Luke",
        book_number: 42,
        chapter: 12,
        verse: 31,
        source: "semantic",
        confidence: 0.75,
      }),
    ])
    expect(kept.map((d) => d.verse_ref)).toEqual([
      "Matthew 6:33",
      "Luke 12:31",
    ])
  })

  it("prefers the digit-stable form when both 6:3 and 6:33 arrive in one batch", async () => {
    await handleVerseDetections([
      makeDetection({
        verse_ref: "Matthew 6:3",
        book_name: "Matthew",
        book_number: 40,
        chapter: 6,
        verse: 3,
        confidence: 1,
        auto_queued: true,
      }),
      makeDetection({
        verse_ref: "Matthew 6:33",
        book_name: "Matthew",
        book_number: 40,
        chapter: 6,
        verse: 33,
        confidence: 1,
        auto_queued: false,
      }),
    ])

    expect(useDetectionStore.getState().detections.map((d) => d.verse_ref)).toEqual([
      "Matthew 6:33",
    ])
    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "Matthew",
      chapter: 6,
      verse: 33,
    })
    expect(useBroadcastStore.getState().liveItem?.reference).toContain("Matthew 6:33")
  })

  it("previews from incoming direct detection event", async () => {
    const detection = makeDetection({
      verse_ref: "Romans 5:8",
      book_number: 45,
      chapter: 5,
      verse: 8,
    })
    await handleVerseDetections([detection])
    await flushDigitGrowthHold()

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
    await flushDigitGrowthHold()

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_number: 45,
      chapter: 8,
      verse: 1,
    })
  })

  it("auto-lives EGW detections when Live Auto Live is on", async () => {
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
    expect(useBroadcastStore.getState().liveItem).toMatchObject({
      kind: "egw",
      reference: "Patriarchs and Prophets p.29 par.2",
    })
  })

  it("keeps EGW in preview when Live Auto Live is off", async () => {
    useBroadcastStore.setState({ readingModeAutoLive: false })

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

    expect(useBroadcastStore.getState().isLive).toBe(false)
    expect(useBroadcastStore.getState().previewItem).toMatchObject({
      kind: "egw",
      reference: "Patriarchs and Prophets p.29 par.2",
    })
  })

  it("selects a higher-confidence EGW statement before a direct Bible hit", async () => {
    await handleVerseDetections([
      makeDetection({ confidence: 0.9, auto_queued: false }),
      makeDetection({
        content_type: "egw",
        source: "semantic",
        confidence: 0.97,
        rank_score: 0.97,
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
    expect(useBroadcastStore.getState().liveItem).toMatchObject({
      kind: "egw",
      reference: "Patriarchs and Prophets p.29 par.2",
    })
  })

  it("arbitrates adjacent direct and EGW events before presenting", async () => {
    const directFlight = scheduleVerseDetections([
      makeDetection({ confidence: 0.9, auto_queued: false }),
    ])
    const egwFlight = scheduleVerseDetections([
      makeDetection({
        content_type: "egw",
        source: "semantic",
        confidence: 0.97,
        rank_score: 0.97,
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

    expect(useBroadcastStore.getState().liveItem).toBeNull()
    await vi.advanceTimersByTimeAsync(400)
    await Promise.all([directFlight, egwFlight])

    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().liveItem).toMatchObject({
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
    await flushDigitGrowthHold()

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
        authorization: "preview-authorized",
        job: "request",
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

    it("passes the Paul-and-Silas candidate batch to the configured Cerebras gate", async () => {
      useSettingsStore.setState({
        aiRankingEnabled: true,
        aiRankingProvider: "cerebras",
        hasDeepseekApiKey: false,
        hasCerebrasApiKey: true,
      })
      const paulAndSilas = makeSemantic()
      scheduleRankingMock.mockResolvedValue(paulAndSilas)

      await handleVerseDetections([
        paulAndSilas,
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])
      await aiSuggestionSettledForTests()

      expect(scheduleRankingMock).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            verse_ref: "Acts 16:25",
            verse_text: "And at midnight Paul and Silas prayed",
            transcript_snippet: "the passage where paul and silas sang in prison",
          }),
        ]),
        expect.objectContaining({
          aiRankingEnabled: true,
          aiRankingProvider: "cerebras",
          hasCerebrasApiKey: true,
        })
      )
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

    it("waits for an in-flight ranking before completing the batch", async () => {
      const winner = makeSemantic()
      let resolveFirstFlight: (value: DetectionResult | null) => void = () => {}
      scheduleRankingMock.mockReturnValueOnce(
        new Promise<DetectionResult | null>((resolve) => {
          resolveFirstFlight = resolve
        })
      )

      const handling = handleVerseDetections([
        winner,
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])
      resolveFirstFlight(winner)
      await handling

      // A ranked partial is only advisory on its first appearance. A second
      // stable batch is required before it can commit the live preview.
      scheduleRankingMock.mockResolvedValue(winner)
      await handleVerseDetections([
        winner,
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])

      expect(useDetectionStore.getState().aiSuggestedKey).toBe("44:16:25")
      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 44,
        chapter: 16,
        verse: 25,
      })
    })

    it("does not let a stale ranking hold a newer detection batch", async () => {
      let resolveFirstFlight: (value: DetectionResult | null) => void = () => {}
      scheduleRankingMock.mockReturnValueOnce(
        new Promise<DetectionResult | null>((resolve) => {
          resolveFirstFlight = resolve
        })
      )

      const first = handleVerseDetections([
        makeSemantic(),
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])
      await Promise.resolve()

      const second = handleVerseDetections([
        makeDetection({ auto_queued: false }),
      ])
      await Promise.resolve()
      await Promise.resolve()
      await vi.runAllTicks()
      await vi.advanceTimersByTimeAsync(0)

      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 43,
        chapter: 3,
        verse: 16,
      })

      resolveFirstFlight(null)
      await Promise.all([first, second])
    })

    it("abandons the AI ranking await when a newer batch supersedes it", async () => {
      // Semantic-only batches await ranking before acting. A newer batch must
      // free that await without waiting for the ranker to finish — even when
      // the ranker promise never settles (real scheduleRanking usually
      // supersedes with null; this asserts workflow generation alone is enough).
      scheduleRankingMock.mockReturnValueOnce(
        new Promise<DetectionResult | null>(() => {
          // Intentionally never resolves: if the stale batch still awaits
          // this promise, `first` never settles.
        })
      )
      scheduleRankingMock.mockResolvedValueOnce(null)

      const first = handleVerseDetections([
        makeSemantic(),
        makeSemantic({ verse_ref: "Acts 12:5", chapter: 12, verse: 5 }),
      ])
      await Promise.resolve()
      await Promise.resolve()

      const second = handleVerseDetections([
        makeDetection({ auto_queued: false }),
      ])
      await Promise.resolve()
      await Promise.resolve()
      await vi.runAllTicks()

      await expect(first).resolves.toBeUndefined()
      await second
      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 43,
        chapter: 3,
        verse: 16,
      })
    })

    it("does not stage preview from a stale batch after a slow verse lookup", async () => {
      let resolveFirstFetch: (value: null) => void = () => {}
      let getVerseCalls = 0
      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "get_verse") {
          getVerseCalls += 1
          if (getVerseCalls === 1) {
            return new Promise((resolve) => {
              resolveFirstFetch = resolve
            })
          }
          return Promise.resolve({
            id: 16,
            translation_id: 7,
            book_number: 43,
            book_name: "John",
            book_abbreviation: "John",
            chapter: 3,
            verse: 16,
            text: "For God so loved the world",
          })
        }
        return Promise.resolve(null)
      })

      const first = handleVerseDetections([
        makeDetection({
          auto_queued: false,
          confidence: 0.99,
          verse_ref: "Acts 16:25",
          book_name: "Acts",
          book_number: 44,
          chapter: 16,
          verse: 25,
          verse_text: "stale batch text",
        }),
      ])
      await Promise.resolve()
      await Promise.resolve()

      const second = handleVerseDetections([
        makeDetection({
          auto_queued: false,
          confidence: 0.99,
          verse_ref: "John 3:16",
          book_name: "John",
          book_number: 43,
          chapter: 3,
          verse: 16,
          verse_text: "For God so loved the world",
        }),
      ])
      await Promise.resolve()
      await Promise.resolve()
      await vi.runAllTicks()

      await second
      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 43,
        chapter: 3,
        verse: 16,
      })

      resolveFirstFetch(null)
      await first
      // Stale Acts lookup must not overwrite the newer John preview.
      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 43,
        chapter: 3,
        verse: 16,
      })
    })

    it("does not let a lower-confidence AI winner displace a stronger direct hit", async () => {
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

    it("uses an AI-confirmed review candidate when no stronger eligible hit exists", async () => {
      const winner = makeSemantic({ confidence: 0.72 })
      scheduleRankingMock.mockResolvedValue(winner)

      await handleVerseDetections([
        winner,
        makeSemantic({
          confidence: 0.71,
          verse_ref: "Acts 15:40",
          chapter: 15,
          verse: 40,
        }),
      ])
      await handleVerseDetections([
        winner,
        makeSemantic({
          confidence: 0.71,
          verse_ref: "Acts 15:40",
          chapter: 15,
          verse: 40,
        }),
      ])

      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 44,
        chapter: 16,
        verse: 25,
      })
    })

    it("uses the AI-confirmed storm verse after modern wording retrieval", async () => {
      const winner = makeSemantic({
        confidence: 0.7,
        verse_ref: "Mark 4:39",
        verse_text:
          "Peace, be still. And the wind ceased, and there was a great calm.",
        book_name: "Mark",
        book_number: 41,
        chapter: 4,
        verse: 39,
        transcript_snippet:
          "Please show the verse that talks about Jesus coming the storm in the boat",
      })
      scheduleRankingMock.mockResolvedValue(winner)

      await handleVerseDetections([
        winner,
        makeSemantic({
          confidence: 0.71,
          verse_ref: "Jeremiah 44:22",
          book_name: "Jeremiah",
          book_number: 24,
          chapter: 44,
          verse: 22,
        }),
      ])
      await handleVerseDetections([
        winner,
        makeSemantic({
          confidence: 0.71,
          verse_ref: "Jeremiah 44:22",
          book_name: "Jeremiah",
          book_number: 24,
          chapter: 44,
          verse: 22,
        }),
      ])

      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 41,
        chapter: 4,
        verse: 39,
      })
    })

    it("selects AI-confirmed 1 Peter 5:8 over tied Ezekiel 22:25", async () => {
      const peter = makeSemantic({
        confidence: 0.88,
        verse_ref: "1 Peter 5:8",
        verse_text:
          "Be sober, be vigilant; because your adversary the devil, as a roaring lion, walketh about, seeking whom he may devour:",
        book_name: "1 Peter",
        book_number: 60,
        chapter: 5,
        verse: 8,
        transcript_snippet: "the devil is like a roaring lion",
      })
      const ezekiel = makeSemantic({
        confidence: 0.88,
        verse_ref: "Ezekiel 22:25",
        verse_text:
          "There is a conspiracy of her prophets in the midst thereof, like a roaring lion ravening the prey; they have devoured souls;",
        book_name: "Ezekiel",
        book_number: 26,
        chapter: 22,
        verse: 25,
        transcript_snippet: "the devil is like a roaring lion",
      })
      scheduleRankingMock.mockResolvedValue(peter)

      await handleVerseDetections([ezekiel, peter])

      expect(useBibleStore.getState().selectedVerse).toMatchObject({
        book_number: 60,
        chapter: 5,
        verse: 8,
      })
    })
  })

  it("does not add 88% semantic cards that lack a lexical quote", async () => {
    await handleVerseDetections([
      makeDetection({
        verse_ref: "Genesis 37:17",
        verse_text: "And the man said, They are departed hence.",
        book_name: "Genesis",
        book_number: 1,
        chapter: 37,
        verse: 17,
        confidence: 0.88,
        source: "semantic",
        auto_queued: false,
        authorization: "suggestion",
        job: "quotation",
        has_lexical_quote: false,
        transcript_snippet: "Let's go to Genesis for this eight",
      }),
    ])

    expect(useDetectionStore.getState().detections).toEqual([])
    expect(useBibleStore.getState().selectedVerse).toBeNull()
    expect(useBroadcastStore.getState().liveItem).toBeNull()
    expect(useBroadcastStore.getState().previewItem).toBeNull()
  })

  it("does not add reject-authorization incomplete citations", async () => {
    await handleVerseDetections([
      makeDetection({
        verse_ref: "Genesis 3:1",
        book_name: "Genesis",
        book_number: 1,
        chapter: 3,
        verse: 1,
        confidence: 0.92,
        source: "direct",
        auto_queued: false,
        is_chapter_only: true,
        authorization: "reject",
        job: "citation",
        transcript_snippet: "Genesis three",
      }),
    ])

    expect(useDetectionStore.getState().detections).toEqual([])
    expect(useBroadcastStore.getState().liveItem).toBeNull()
  })

  it("keeps a high-overlap quotation auto-live path intact", async () => {
    await handleVerseDetections([
      makeDetection({
        verse_ref: "John 3:16",
        verse_text:
          "For God so loved the world, that he gave his only begotten Son.",
        book_name: "John",
        book_number: 43,
        chapter: 3,
        verse: 16,
        confidence: 0.95,
        source: "semantic",
        auto_queued: false,
        authorization: "live-authorized",
        job: "quotation",
        has_lexical_quote: true,
        transcript_snippet:
          "For God so loved the world that he gave his only begotten Son",
      }),
    ])

    expect(useBroadcastStore.getState().isLive).toBe(true)
    expect(useBroadcastStore.getState().liveItem?.reference).toBe(
      "John 3:16 (KJV)"
    )
  })
})
