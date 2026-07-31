import { beforeEach, describe, expect, it, vi } from "vitest"

const invokeTauri = vi.fn()
vi.mock("@/lib/tauri-runtime", () => ({
  invokeTauri: (...args: unknown[]) => invokeTauri(...args),
  isTauriRuntime: () => true,
}))

import {
  buildRankingCandidates,
  pickRankingTranscript,
  rankSemanticDetections,
  resetRankerForTests,
  shouldRankDetections,
} from "./deepseek-ranker"
import type { DetectionResult } from "@/types"

function semantic(overrides: Partial<DetectionResult> = {}): DetectionResult {
  return {
    content_type: "bible",
    verse_ref: "Acts 16:25",
    verse_text: "And at midnight Paul and Silas prayed...",
    book_name: "Acts",
    book_number: 44,
    chapter: 16,
    verse: 25,
    confidence: 0.78,
    source: "semantic",
    auto_queued: false,
    transcript_snippet: "the passage where paul and silas sang in prison",
    is_chapter_only: false,
    egw_paragraph: null,
    ...overrides,
  }
}

const gate = {
  deepseekRankingEnabled: true,
  hasDeepseekApiKey: true,
  confidenceThreshold: 0.9,
}

beforeEach(() => {
  invokeTauri.mockReset()
  resetRankerForTests()
})

describe("buildRankingCandidates", () => {
  it("keeps only resolvable semantic detections, deduped, capped at 5, compact summaries", () => {
    const detections = [
      semantic({ verse_text: "x".repeat(300) }),
      semantic(), // duplicate — same book:chapter:verse
      semantic({ source: "direct" }),
      semantic({ book_number: 0 }),
      semantic({ is_chapter_only: true }),
      semantic({ verse: 26 }),
      semantic({ verse: 27 }),
      semantic({ verse: 28 }),
      semantic({ verse: 29 }),
      semantic({ verse: 30 }),
    ]
    const candidates = buildRankingCandidates(detections)
    expect(candidates.length).toBe(5)
    expect(candidates[0].id).toBe("44:16:25")
    expect(candidates[0].summary.length).toBeLessThanOrEqual(80)
    expect(candidates[0].summary).toContain("Acts 16:25")
  })
})

describe("shouldRankDetections", () => {
  const two = [semantic(), semantic({ verse: 26 })]

  it("requires toggle, key, no strong direct hit, and 2+ semantic candidates", () => {
    expect(shouldRankDetections(two, gate)).toBe(true)
    expect(
      shouldRankDetections(two, { ...gate, deepseekRankingEnabled: false })
    ).toBe(false)
    expect(
      shouldRankDetections(two, { ...gate, hasDeepseekApiKey: false })
    ).toBe(false)
    expect(shouldRankDetections([semantic()], gate)).toBe(false)
    const withDirect = [
      ...two,
      semantic({ source: "direct", confidence: 0.95 }),
    ]
    expect(shouldRankDetections(withDirect, gate)).toBe(false)
  })
})

describe("pickRankingTranscript", () => {
  it("uses the longest semantic snippet, clamped to 500 chars", () => {
    const detections = [
      semantic({ transcript_snippet: "short" }),
      semantic({ verse: 26, transcript_snippet: "x".repeat(600) }),
    ]
    const transcript = pickRankingTranscript(detections)
    expect(transcript.length).toBe(500)
  })
})

describe("rankSemanticDetections", () => {
  const two = [semantic(), semantic({ verse: 26 })]

  it("returns the detection whose id Rust selected", async () => {
    invokeTauri.mockResolvedValue("44:16:26")
    const winner = await rankSemanticDetections(two, gate)
    expect(winner?.verse).toBe(26)
    expect(invokeTauri).toHaveBeenCalledWith(
      "rank_detection_candidates",
      expect.objectContaining({
        transcript: expect.any(String),
        candidates: expect.any(Array),
      })
    )
  })

  it("returns null on abstention", async () => {
    invokeTauri.mockResolvedValue(null)
    expect(await rankSemanticDetections(two, gate)).toBeNull()
  })

  it("returns null when Rust returns an id not in the batch (belt-and-braces)", async () => {
    invokeTauri.mockResolvedValue("1:1:1")
    expect(await rankSemanticDetections(two, gate)).toBeNull()
  })

  it("is single-flight: a second call while one is pending returns null without invoking", async () => {
    let resolveCall: (v: unknown) => void = () => {}
    invokeTauri.mockReturnValue(
      new Promise((r) => {
        resolveCall = r
      })
    )
    const first = rankSemanticDetections(two, gate)
    const second = await rankSemanticDetections(two, gate)
    expect(second).toBeNull()
    expect(invokeTauri).toHaveBeenCalledTimes(1)
    resolveCall(null)
    await first
  })

  it("opens the circuit breaker after 3 consecutive failures (incl. timeouts)", async () => {
    invokeTauri.mockRejectedValue(
      new Error("DeepSeek ranking timed out after 1800 ms")
    )
    await rankSemanticDetections(two, gate)
    await rankSemanticDetections(two, gate)
    await rankSemanticDetections(two, gate)
    invokeTauri.mockClear()
    await rankSemanticDetections(two, gate)
    expect(invokeTauri).not.toHaveBeenCalled()
  })

  it("a success resets the failure count", async () => {
    invokeTauri.mockRejectedValueOnce(new Error("boom"))
    await rankSemanticDetections(two, gate)
    invokeTauri.mockResolvedValueOnce(null)
    await rankSemanticDetections(two, gate)
    invokeTauri.mockRejectedValue(new Error("boom"))
    await rankSemanticDetections(two, gate)
    await rankSemanticDetections(two, gate)
    invokeTauri.mockClear()
    invokeTauri.mockResolvedValue(null)
    await rankSemanticDetections(two, gate)
    expect(invokeTauri).toHaveBeenCalledTimes(1) // breaker NOT open (2 fails, not 3)
  })
})
