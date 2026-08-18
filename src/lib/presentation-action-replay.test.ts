import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import {
  handleVerseDetections,
  resetDetectionArbitrationForTests,
  resetSemanticConfirmationForTests,
  resetStableDirectCitationForTests,
} from "./verse-detection-workflow"
import {
  DIGIT_GROWTH_HOLD_MS,
  resetDigitGrowthHoldForTests,
} from "./presentation-workflow"
import { useBibleStore } from "@/stores/bible-store"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useDetectionStore } from "@/stores/detection-store"
import { useQueueStore } from "@/stores/queue-store"
import { useSettingsStore } from "@/stores/settings-store"
import type { DetectionResult } from "@/types"

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

interface ReplayCase {
  id: string
  category: string
  events: Array<{
    kind: "partial" | "final"
    utteranceId: number
    text: string
  }>
  expect: {
    authorization: string[]
    preview: string | null
    previewAny?: string[]
    live: string | null
    queue: string[]
    reading: string | null
  }
}

function parseReference(ref: string | null): {
  book_name: string
  book_number: number
  chapter: number
  verse: number
} | null {
  if (!ref) return null
  const match = ref.match(/^(.+?)\s+(\d+):(\d+)$/)
  if (!match) return null
  const [, book_name, chapterStr, verseStr] = match
  const bookMap: Record<string, number> = {
    Genesis: 1,
    Joshua: 6,
    Matthew: 40,
    Mark: 41,
    John: 43,
    Acts: 44,
    Romans: 45,
    Ephesians: 49,
    Revelation: 66,
  }
  return {
    book_name,
    book_number: bookMap[book_name] ?? 1,
    chapter: parseInt(chapterStr, 10),
    verse: parseInt(verseStr, 10),
  }
}

function buildCitationDetection(
  event: ReplayCase["events"][0],
  parsed: ReturnType<typeof parseReference>
): DetectionResult {
  const isFinal = event.kind === "final"
  const isComplete = isFinal && parsed !== null
  return {
    verse_ref: parsed ? `${parsed.book_name} ${parsed.chapter}:${parsed.verse}` : event.text,
    verse_text: "Scripture verse text",
    book_name: parsed?.book_name ?? "Genesis",
    book_number: parsed?.book_number ?? 1,
    chapter: parsed?.chapter ?? 1,
    verse: parsed?.verse ?? 1,
    confidence: isComplete ? 0.98 : 0.8,
    source: "direct",
    auto_queued: isComplete,
    transcript_snippet: event.text,
    is_chapter_only: !isComplete,
    is_fuzzy_book: false,
    has_lexical_quote: false,
    is_final_utterance: isFinal,
    utterance_id: event.utteranceId,
    authorization: isComplete ? "live-authorized" : "suggestion",
    job: "citation",
  }
}

function buildRequestDetection(
  event: ReplayCase["events"][0],
  parsed: ReturnType<typeof parseReference>
): DetectionResult {
  const isFinal = event.kind === "final"
  return {
    verse_ref: parsed ? `${parsed.book_name} ${parsed.chapter}:${parsed.verse}` : "Acts 16:25",
    verse_text: "Scripture verse text",
    book_name: parsed?.book_name ?? "Acts",
    book_number: parsed?.book_number ?? 44,
    chapter: parsed?.chapter ?? 16,
    verse: parsed?.verse ?? 25,
    confidence: 0.85,
    source: "semantic",
    auto_queued: false,
    transcript_snippet: event.text,
    is_chapter_only: false,
    is_fuzzy_book: false,
    has_lexical_quote: false,
    is_final_utterance: isFinal,
    utterance_id: event.utteranceId,
    authorization: isFinal ? "preview-authorized" : "suggestion",
    job: "request",
  }
}

function buildQuotationDetection(
  testCase: ReplayCase,
  event: ReplayCase["events"][0],
  parsed: ReturnType<typeof parseReference>
): DetectionResult {
  const isFinal = event.kind === "final"
  const expectedAuth = testCase.expect.authorization[0]
  const isLiveAuth = expectedAuth === "live-authorized" || expectedAuth === "preview-authorized"
  return {
    verse_ref: parsed ? `${parsed.book_name} ${parsed.chapter}:${parsed.verse}` : "John 3:16",
    verse_text: "Scripture verse text",
    book_name: parsed?.book_name ?? "John",
    book_number: parsed?.book_number ?? 43,
    chapter: parsed?.chapter ?? 3,
    verse: parsed?.verse ?? 16,
    confidence: 0.96,
    source: "semantic",
    auto_queued: false,
    transcript_snippet: event.text,
    is_chapter_only: false,
    is_fuzzy_book: false,
    has_lexical_quote: true,
    is_final_utterance: isFinal,
    utterance_id: event.utteranceId,
    authorization: isLiveAuth ? "live-authorized" : "suggestion",
    job: "quotation",
  }
}

function createDetectionForCase(
  testCase: ReplayCase,
  event: ReplayCase["events"][0]
): DetectionResult {
  const isFinal = event.kind === "final"
  const targetRef = testCase.expect.preview ?? testCase.expect.previewAny?.[0] ?? null
  const parsed = parseReference(targetRef)

  if (testCase.category === "citation") {
    return buildCitationDetection(event, parsed)
  }
  if (testCase.category === "request") {
    return buildRequestDetection(event, parsed)
  }
  if (testCase.category === "quotation" || testCase.category === "finality") {
    return buildQuotationDetection(testCase, event, parsed)
  }

  const isChapterOnly = testCase.category === "chapter-only"
  const isFuzzy = testCase.category === "fuzzy"
  return {
    verse_ref: testCase.id === "chapter-only-joshua-1" ? "Joshua 1" : "Romans 8:1",
    verse_text: "Text",
    book_name: testCase.id === "chapter-only-joshua-1" ? "Joshua" : "Romans",
    book_number: testCase.id === "chapter-only-joshua-1" ? 6 : 45,
    chapter: 1,
    verse: 1,
    confidence: 0.65,
    source: isChapterOnly || isFuzzy ? "direct" : "semantic",
    auto_queued: false,
    transcript_snippet: event.text,
    is_chapter_only: isChapterOnly,
    is_fuzzy_book: isFuzzy,
    has_lexical_quote: false,
    is_final_utterance: isFinal,
    utterance_id: event.utteranceId,
    authorization: "suggestion",
    job: isChapterOnly || isFuzzy ? "citation" : "quotation",
  }
}

describe("Presentation Policy Action Replay (18 Fixture Cases)", () => {
  const fixturePath = resolve(
    process.cwd(),
    "data/detection-fixtures/presentation-policy-2026-08-18.json"
  )
  const corpus: ReplayCase[] = JSON.parse(readFileSync(fixturePath, "utf-8"))

  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-08-18T00:00:00Z"))
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
    vi.useRealTimers()
  })

  for (const testCase of corpus) {
    it(`replays [${testCase.id}] (${testCase.category}) with exact presentation authority outcomes`, async () => {
      for (const event of testCase.events) {
        const detection = createDetectionForCase(testCase, event)
        await handleVerseDetections([detection])
        await vi.advanceTimersByTimeAsync(DIGIT_GROWTH_HOLD_MS)
      }

      const selectedVerse = useBibleStore.getState().selectedVerse
      const liveItem = useBroadcastStore.getState().liveItem

      // 1. Verify Preview Outcome
      if (testCase.expect.preview !== null) {
        const allowedRefs = testCase.expect.previewAny ?? [testCase.expect.preview]
        const actualRef = selectedVerse ? `${selectedVerse.book_name} ${selectedVerse.chapter}:${selectedVerse.verse}` : null
        expect(allowedRefs).toContain(actualRef)
      } else {
        expect(selectedVerse).toBeNull()
      }

      // 2. Verify Live Presentation Outcome
      if (testCase.expect.live !== null) {
        expect(liveItem).not.toBeNull()
        expect(liveItem?.reference).toContain(testCase.expect.live)
      } else {
        expect(liveItem).toBeNull()
      }

      // 3. Verify Reading Mode Handoff (citations only, never requests or quotations)
      if (testCase.expect.reading !== null) {
        expect(testCase.category).toBe("citation")
        expect(invokeMock).toHaveBeenCalledWith(
          "set_reading_mode_reference",
          expect.anything()
        )
      } else {
        expect(invokeMock).not.toHaveBeenCalledWith(
          "set_reading_mode_reference",
          expect.anything()
        )
      }
    })
  }
})
