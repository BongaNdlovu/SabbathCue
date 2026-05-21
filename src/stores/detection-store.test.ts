import { beforeEach, describe, expect, it, vi } from "vitest"
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
  beforeEach(() => {
    useDetectionStore.setState({
      detections: [],
      autoMode: false,
      confidenceThreshold: 0.8,
    })
  })

  it("newer detection appears above older higher-confidence detection", () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-05-19T00:00:00Z"))
    
    const store = useDetectionStore.getState()
    
    // Add older high-confidence detection
    store.addDetection(makeDetection({ verse_ref: "Romans 8:1", confidence: 0.99 }))
    
    // Advance time
    vi.setSystemTime(new Date("2026-05-19T00:00:01Z"))
    
    // Add newer lower-confidence detection
    store.addDetection(makeDetection({ verse_ref: "John 3:16", confidence: 0.85 }))
    
    const detections = useDetectionStore.getState().detections
    expect(detections[0].verse_ref).toBe("John 3:16")
    expect(detections[1].verse_ref).toBe("Romans 8:1")
    
    vi.useRealTimers()
  })

  it("duplicate verse refreshes recency and keeps best confidence", () => {
    const store = useDetectionStore.getState()
    
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-05-19T00:00:00Z"))
    
    // Add first detection with lower confidence
    store.addDetection(makeDetection({ verse_ref: "John 3:16", confidence: 0.85 }))
    
    // Add duplicate with higher confidence
    vi.setSystemTime(new Date("2026-05-19T00:00:01Z"))
    store.addDetection(makeDetection({ verse_ref: "John 3:16", confidence: 0.96 }))
    
    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].confidence).toBe(0.96)
    
    vi.useRealTimers()
  })

  it("duplicate verse preserves text when new detection has empty text", () => {
    const store = useDetectionStore.getState()
    
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-05-19T00:00:00Z"))
    
    // Add first detection with text
    store.addDetection(makeDetection({ verse_ref: "John 3:16", verse_text: "For God so loved the world" }))
    
    // Add duplicate with empty text
    vi.setSystemTime(new Date("2026-05-19T00:00:01Z"))
    store.addDetection(makeDetection({ verse_ref: "John 3:16", verse_text: "", confidence: 0.97 }))
    
    const detections = useDetectionStore.getState().detections
    expect(detections).toHaveLength(1)
    expect(detections[0].verse_text).toBe("For God so loved the world")
    expect(detections[0].confidence).toBe(0.97)
    
    vi.useRealTimers()
  })

  it("sorts by received_at descending, then confidence descending", () => {
    const store = useDetectionStore.getState()
    
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-05-19T00:00:00Z"))
    
    store.addDetection(makeDetection({ verse_ref: "A", confidence: 0.9 }))
    
    vi.setSystemTime(new Date("2026-05-19T00:00:01Z"))
    store.addDetection(makeDetection({ verse_ref: "B", confidence: 0.8 }))
    
    vi.setSystemTime(new Date("2026-05-19T00:00:01Z"))
    store.addDetection(makeDetection({ verse_ref: "C", confidence: 0.85 }))
    
    const detections = useDetectionStore.getState().detections
    // B and C have same received_at, so C (higher confidence) should come first
    expect(detections[0].verse_ref).toBe("C")
    expect(detections[1].verse_ref).toBe("B")
    expect(detections[2].verse_ref).toBe("A")
    
    vi.useRealTimers()
  })
})
