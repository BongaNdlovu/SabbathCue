// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { DetectionsPanel } from "./detections-panel"
import { CollectedDetectionsPanel } from "./collected-detections-panel"
import { useDetectionStore } from "@/stores/detection-store"
import { useSettingsStore } from "@/stores/settings-store"
import { useCollectedDetectionsStore } from "@/stores/collected-detections-store"
import type { DetectionResult, Verse } from "@/types"

const {
  addOrFlashItemMock,
  clearDetectionsMock,
  detection,
  hymnDetection,
  detectionsRef,
  presentVerseMock,
  selectPreviewVerseMock,
  presentHymnMock,
  previewHymnMock,
  queueHymnMock,
  verse,
} = vi.hoisted(() => {
  const detection: DetectionResult = {
    verse_ref: "John 3:16",
    verse_text: "For God so loved the world.",
    book_name: "John",
    book_number: 43,
    chapter: 3,
    verse: 16,
    confidence: 0.96,
    source: "direct",
    auto_queued: false,
    transcript_snippet: "John chapter three verse sixteen",
    is_chapter_only: false,
  }

  const hymnDetection: DetectionResult = {
    content_type: "hymn",
    verse_ref: "Hymn 46",
    verse_text: "Holy, Holy, Holy",
    book_name: "Hymn",
    book_number: 0,
    chapter: 0,
    verse: 46,
    confidence: 1,
    source: "direct",
    auto_queued: false,
    transcript_snippet: "",
    is_chapter_only: false,
    hymn: { number: 46, id: "sda-46", title: "Holy, Holy, Holy" },
  }

  const verse: Verse = {
    id: 0,
    translation_id: 1,
    book_number: 43,
    book_name: "John",
    book_abbreviation: "",
    chapter: 3,
    verse: 16,
    text: "For God so loved the world.",
  }

  return {
    addOrFlashItemMock: vi.fn(),
    clearDetectionsMock: vi.fn(),
    detection,
    hymnDetection,
    detectionsRef: { current: [detection] as DetectionResult[] },
    presentVerseMock: vi.fn(),
    selectPreviewVerseMock: vi.fn(),
    presentHymnMock: vi.fn(),
    previewHymnMock: vi.fn(),
    queueHymnMock: vi.fn(),
    verse,
  }
})

vi.mock("@/hooks/use-detection", () => ({
  useDetection: () => ({ detections: detectionsRef.current }),
  detectionActions: {
    clearDetections: clearDetectionsMock,
    getDetectionStatus: vi.fn(async () => ({
      has_semantic: true,
      paraphrase_enabled: false,
      semantic_detection_enabled: true,
    })),
  },
}))

vi.mock("@/lib/presentation-workflow", () => ({
  createEgwQueueItem: vi.fn(() => ({ id: "egw-queue-item" })),
  createScriptureQueueItem: vi.fn(() => ({ id: "queue-item" })),
  detectionToVerse: vi.fn(() => verse),
  presentVerse: (...args: unknown[]) => presentVerseMock(...args),
  presentEgwParagraph: vi.fn(),
  previewEgwParagraph: vi.fn(),
  selectPreviewVerse: (...args: unknown[]) => selectPreviewVerseMock(...args),
}))

vi.mock("@/services/hymnal/hymn-voice-control", () => ({
  presentHymnByNumber: (...args: unknown[]) => presentHymnMock(...args),
  previewHymnByNumber: (...args: unknown[]) => previewHymnMock(...args),
  queueHymnByNumber: (...args: unknown[]) => queueHymnMock(...args),
}))

vi.mock("@/stores/queue-store", () => ({
  useQueueStore: {
    getState: () => ({
      addOrFlashItem: addOrFlashItemMock,
    }),
  },
}))

describe("DetectionsPanel", () => {
  beforeEach(() => {
    detectionsRef.current = [detection]
    selectPreviewVerseMock.mockClear()
    presentVerseMock.mockClear()
    clearDetectionsMock.mockClear()
    addOrFlashItemMock.mockClear()
    presentHymnMock.mockClear()
    previewHymnMock.mockClear()
    queueHymnMock.mockClear()
    useCollectedDetectionsStore.getState().clear()
    useDetectionStore.setState({ aiSuggestedKey: null })
    useSettingsStore.setState({ autoMode: true })
  })

  afterEach(() => {
    cleanup()
  })

  it("previews a detection without navigating the Bible search panel", () => {
    render(<DetectionsPanel />)

    fireEvent.click(screen.getByRole("button", { name: /preview/i }))

    expect(selectPreviewVerseMock).toHaveBeenCalledWith(verse)
    expect(selectPreviewVerseMock).not.toHaveBeenCalledWith(
      verse,
      expect.objectContaining({ navigate: true })
    )
  })

  it("does not render the automatic preview switch in the panel header", () => {
    render(<DetectionsPanel />)

    expect(
      screen.queryByRole("switch", { name: /auto preview detections/i })
    ).toBeNull()
  })

  it("does not render collected detections inside the detections panel", () => {
    useCollectedDetectionsStore.getState().record(detection)

    render(<DetectionsPanel />)

    expect(screen.queryByText("Collected for this service")).toBeNull()
  })

  it("presents a detection without navigating the Bible search panel", () => {
    render(<DetectionsPanel />)

    fireEvent.click(screen.getByRole("button", { name: /present/i }))

    expect(presentVerseMock).toHaveBeenCalledWith(verse)
    expect(presentVerseMock).not.toHaveBeenCalledWith(
      verse,
      expect.objectContaining({ navigate: true })
    )
  })

  it("manually collects a Bible detection and disables its collect button", () => {
    render(<DetectionsPanel />)

    fireEvent.click(screen.getByRole("button", { name: /collect john 3:16/i }))

    expect(useCollectedDetectionsStore.getState().items[0]?.kind).toBe(
      "scripture"
    )
    expect(
      screen.getByRole("button", { name: /john 3:16 collected/i })
    ).toHaveProperty("disabled", true)
    expect(screen.getByText("Collected \u2713")).toBeTruthy()
  })

  it("collects detections that are presented or queued, but not previewed", () => {
    render(
      <>
        <DetectionsPanel />
        <CollectedDetectionsPanel />
      </>
    )

    fireEvent.click(screen.getByRole("button", { name: /preview/i }))
    expect(screen.getByText(/verses you present or queue/i)).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: /present/i }))
    expect(screen.getByText("Collected for this service")).toBeTruthy()
    expect(screen.getAllByText("John 3:16")).toHaveLength(2)

    fireEvent.click(screen.getAllByRole("button", { name: /^queue$/i })[0])
    expect(screen.getByText("x2")).toBeTruthy()
  })

  it("removes and clears collected detections", () => {
    render(
      <>
        <DetectionsPanel />
        <CollectedDetectionsPanel />
      </>
    )

    fireEvent.click(screen.getByRole("button", { name: /present/i }))
    fireEvent.click(screen.getByRole("button", { name: /remove john 3:16/i }))
    expect(screen.queryByText("x1")).toBeNull()
    expect(screen.getByText(/verses you present or queue/i)).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: /present/i }))
    fireEvent.click(screen.getByRole("button", { name: /^clear$/i }))
    expect(screen.getByText(/verses you present or queue/i)).toBeTruthy()
  })

  it("renders an Ellen White detection card in the box", () => {
    const egwDetection: DetectionResult = {
      content_type: "egw",
      verse_ref: "Steps to Christ p.9 par.2",
      verse_text: "Nature and revelation alike testify of God's love.",
      book_name: "Steps to Christ",
      book_number: 2,
      chapter: 9,
      verse: 2,
      confidence: 0.94,
      source: "direct",
      auto_queued: false,
      transcript_snippet: "steps to christ page nine paragraph two",
      is_chapter_only: false,
      egw_paragraph: {
        id: 1,
        book_number: 2,
        book_title: "Steps to Christ",
        chapter: 1,
        chapter_title: "God's Love for Man",
        paragraph: 2,
        page: 9,
        page_paragraph: 2,
        text: "Nature and revelation alike testify of God's love.",
      },
    }
    detectionsRef.current = [egwDetection]
    render(<DetectionsPanel />)

    expect(screen.getByText("Steps to Christ p.9 par.2")).toBeTruthy()
    expect(
      screen.getByText("Nature and revelation alike testify of God's love.")
    ).toBeTruthy()

    fireEvent.click(
      screen.getByRole("button", {
        name: /collect steps to christ p\.9 par\.2/i,
      })
    )
    expect(useCollectedDetectionsStore.getState().items[0]?.kind).toBe("egw")
    expect(
      screen.getByRole("button", {
        name: /steps to christ p\.9 par\.2 collected/i,
      })
    ).toHaveProperty("disabled", true)
  })

  it("renders a spoken hymn as a card and sends it live", async () => {
    detectionsRef.current = [hymnDetection]
    render(<DetectionsPanel />)

    expect(screen.getByText("Hymn 46")).toBeTruthy()
    expect(screen.getByText("Holy, Holy, Holy")).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: /collect hymn 46/i }))
    expect(useCollectedDetectionsStore.getState().items[0]?.kind).toBe("hymn")
    expect(
      screen.getByRole("button", { name: /hymn 46 collected/i })
    ).toHaveProperty("disabled", true)

    fireEvent.click(screen.getByRole("button", { name: /present/i }))

    await waitFor(() => expect(presentHymnMock).toHaveBeenCalledWith(46))
    expect(presentVerseMock).not.toHaveBeenCalled()
  })

  it("shows the AI suggested badge only on the matching card", () => {
    const actsDetection: DetectionResult = {
      ...detection,
      verse_ref: "Acts 16:25",
      book_name: "Acts",
      book_number: 44,
      chapter: 16,
      verse: 25,
      confidence: 0.78,
      source: "semantic",
    }
    detectionsRef.current = [detection, actsDetection]
    // Key format must match detectionCandidateId's "book:chapter:verse".
    useDetectionStore.setState({ aiSuggestedKey: "44:16:25" })

    render(<DetectionsPanel />)

    expect(screen.getAllByText("AI suggested")).toHaveLength(1)
    const badge = screen.getByText("AI suggested")
    const actsCard = screen.getByText("Acts 16:25").closest(".queue-item")
    expect(actsCard).toBeTruthy()
    expect(actsCard?.contains(badge)).toBe(true)
  })

  it("renders no AI badge when no suggestion is active", () => {
    render(<DetectionsPanel />)

    expect(screen.queryByText("AI suggested")).toBeNull()
  })

  it("shows context stack and held references for operator review", () => {
    detectionsRef.current = [
      {
        ...detection,
        verse_ref: "Romans 4:22",
        book_name: "Romans",
        book_number: 45,
        chapter: 4,
        verse: 22,
        confidence: 0.84,
      },
      {
        ...detection,
        verse_ref: "John 3",
        verse: 1,
        confidence: 1,
        is_chapter_only: true,
      },
      {
        ...detection,
        verse_ref: "Revelation 14:7",
        book_name: "Revelation",
        book_number: 66,
        chapter: 14,
        verse: 7,
        confidence: 0.98,
      },
    ]
    useSettingsStore.setState({
      confidenceThreshold: 0.85,
      semanticConfidenceThreshold: 0.7,
    })

    render(<DetectionsPanel />)

    expect(screen.getByText("Context stack")).toBeTruthy()
    expect(screen.getByText("Held references")).toBeTruthy()
    expect(screen.getByText("Below auto-live threshold")).toBeTruthy()
    expect(screen.getByText("Waiting for verse")).toBeTruthy()
    expect(screen.getByText("Recent trusted detections")).toBeTruthy()
    expect(screen.getByText("Revelation 14:7")).toBeTruthy()
  })
})
