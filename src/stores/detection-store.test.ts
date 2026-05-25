import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { useDetectionStore } from "./detection-store"
import type { DetectionResult } from "@/types"

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

describe("detection store", () => {
  let now = new Date("2026-05-19T00:00:00Z").getTime()

  beforeEach(() => {
    now = new Date("2026-05-19T00:00:00Z").getTime()
    vi.spyOn(Date, "now").mockImplementation(() => now)
    useDetectionStore.setState({
      detections: [],
      autoMode: false,
      confidenceThreshold: 0.8,
    })
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it("higher-confidence detection stays above a newer weaker detection", () => {
    const store = useDetectionStore.getState()
    
    // Add older high-confidence detection
    store.addDetection(makeDetection({ verse_ref: "Romans 8:1", confidence: 0.99 }))
    
    // Advance time
    now = new Date("2026-05-19T00:00:01Z").getTime()
    
    // Add newer lower-confidence detection
    store.addDetection(makeDetection({ verse_ref: "John 3:16", confidence: 0.85 }))
    
    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("Romans 8:1")
    expect(detections[1].verse_ref).toBe("John 3:16")
  })

  it("stronger semantic hit outranks weaker direct hit", () => {
    const store = useDetectionStore.getState()

    store.addDetection(makeDetection({
      verse_ref: "Strong Semantic",
      confidence: 0.91,
      source: "semantic",
    }))

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({
      verse_ref: "Weak Direct",
      confidence: 0.84,
      source: "direct",
    }))

    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("Strong Semantic")
    expect(detections[1].verse_ref).toBe("Weak Direct")
  })

  it("near-tied direct hit wins over semantic", () => {
    const store = useDetectionStore.getState()

    store.addDetection(makeDetection({
      verse_ref: "Near Semantic",
      confidence: 0.90,
      source: "semantic",
    }))

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({
      verse_ref: "Near Direct",
      confidence: 0.87,
      source: "direct",
    }))

    // 0.87 + 0.04 = 0.91 vs 0.90 → direct wins by narrow margin
    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("Near Direct")
    expect(detections[1].verse_ref).toBe("Near Semantic")
  })

  it("duplicate verse refreshes recency and keeps best confidence", () => {
    const store = useDetectionStore.getState()
    
    // Add first detection with lower confidence
    store.addDetection(makeDetection({ verse_ref: "John 3:16", confidence: 0.85 }))
    
    // Add duplicate with higher confidence
    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({ verse_ref: "John 3:16", confidence: 0.96 }))
    
    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].confidence).toBe(0.96)
  })

  it("duplicate verse preserves text when new detection has empty text", () => {
    const store = useDetectionStore.getState()
    
    // Add first detection with text
    store.addDetection(makeDetection({ verse_ref: "John 3:16", verse_text: "For God so loved the world" }))
    
    // Add duplicate with empty text
    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({ verse_ref: "John 3:16", verse_text: "", confidence: 0.97 }))
    
    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].verse_text).toBe("For God so loved the world")
    expect(detections[0].confidence).toBe(0.97)
  })

  it("sorts by relevance score before recency", () => {
    const store = useDetectionStore.getState()
    
    store.addDetection(makeDetection({ verse_ref: "A", confidence: 0.9 }))
    
    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({ verse_ref: "B", confidence: 0.8 }))
    
    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({ verse_ref: "C", confidence: 0.85 }))
    
    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("A")
    expect(detections[1].verse_ref).toBe("C")
    expect(detections[2].verse_ref).toBe("B")
  })

  it("duplicate semantic hit cannot replace a direct source label", () => {
    const store = useDetectionStore.getState()

    store.addDetection(makeDetection({
      verse_ref: "John 3:16",
      confidence: 0.9,
      source: "direct",
    }))

    now = new Date("2026-05-19T00:00:01Z").getTime()
    store.addDetection(makeDetection({
      verse_ref: "John 3:16",
      confidence: 0.95,
      source: "semantic",
    }))

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].source).toBe("direct")
    expect(detections[0].confidence).toBe(0.95)
  })

  it("keeps at most 8 detections", () => {
    const store = useDetectionStore.getState()

    for (let i = 0; i < 9; i += 1) {
      now = new Date("2026-05-19T00:00:00Z").getTime() + i
      store.addDetection(makeDetection({ verse_ref: `Ref ${i}`, confidence: 0.8 }))
    }

    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(8)
    expect(detections[0].verse_ref).toBe("Ref 8")
    expect(detections.some((d) => d.verse_ref === "Ref 0")).toBe(false)
  })
})
