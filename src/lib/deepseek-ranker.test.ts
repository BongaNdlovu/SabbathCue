import { beforeEach, describe, expect, it, vi } from "vitest"

const invokeTauri = vi.fn()
vi.mock("@/lib/tauri-runtime", () => ({
  invokeTauri: (...args: unknown[]) => invokeTauri(...args),
  isTauriRuntime: () => true,
}))

import {
  buildRankingCandidates,
  noteBatchForGating,
  pickRankingTranscript,
  rankSemanticDetections,
  resetRankerForTests,
  retrievalIsDecisive,
  scheduleRanking,
  shouldRankDetections,
  preferSpokenBook,
  type RankingGate,
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

const gate: RankingGate = {
  aiRankingEnabled: true,
  aiRankingProvider: "deepseek",
  hasDeepseekApiKey: true,
  hasCerebrasApiKey: false,
  confidenceThreshold: 0.9,
}

const cerebrasGate: RankingGate = {
  aiRankingEnabled: true,
  aiRankingProvider: "cerebras",
  hasDeepseekApiKey: false,
  hasCerebrasApiKey: true,
  confidenceThreshold: 0.9,
}

beforeEach(() => {
  invokeTauri.mockReset()
  resetRankerForTests()
})

describe("buildRankingCandidates", () => {
  it("keeps only resolvable semantic detections, deduped, capped at 8, with reference and verseText", () => {
    const detections = [
      semantic({ verse_text: "x".repeat(300) }),
      semantic(), // duplicate — same book:chapter:verse
      semantic({ source: "direct" }),
      semantic({ book_number: 0 }),
      semantic({ is_chapter_only: true }),
      semantic({ verse: 26, verse_ref: "Acts 16:26" }),
      semantic({ verse: 27, verse_ref: "Acts 16:27" }),
      semantic({ verse: 28, verse_ref: "Acts 16:28" }),
      semantic({ verse: 29, verse_ref: "Acts 16:29" }),
      semantic({ verse: 30, verse_ref: "Acts 16:30" }),
      semantic({ verse: 31, verse_ref: "Acts 16:31" }),
      semantic({ verse: 32, verse_ref: "Acts 16:32" }),
      semantic({ verse: 33, verse_ref: "Acts 16:33" }),
    ]
    const candidates = buildRankingCandidates(detections)
    expect(candidates.length).toBe(8)
    expect(candidates[0].id).toBe("44:16:25")
    expect(candidates[0].reference).toBe("Acts 16:25")
    expect(candidates[0].verseText).toBe("x".repeat(300))
    expect(candidates[0].confidence).toBe(0.78)
    expect(candidates[0].evidence.rankScore).toBe(0.78)
    expect(candidates[0].evidence.namedBookMatch).toBe(false)
    expect(candidates[0].evidence.exactPhraseMatch).toBe(false)
  })

  it("passes local confidence through to the ranking candidates", () => {
    const detections = [
      semantic({
        source: "semantic",
        book_number: 17,
        chapter: 4,
        verse: 14,
        confidence: 0.7,
        verse_ref: "Esther 4:14",
        verse_text: "for such a time as this",
        book_name: "Esther",
      }),
      semantic({
        source: "semantic",
        book_number: 30,
        chapter: 5,
        verse: 13,
        confidence: 0.7,
        verse_ref: "Amos 5:13",
        verse_text: "it is an evil time",
        book_name: "Amos",
      }),
    ]
    const candidates = buildRankingCandidates(detections, "such a time as this")
    expect(candidates).toHaveLength(2)
    expect(candidates[0].confidence).toBe(0.7)
    expect(candidates[0].reference).toBe("Esther 4:14")
    expect(candidates[0].verseText).toBe("for such a time as this")
    expect(candidates[0].evidence.exactPhraseMatch).toBe(true)
    expect(candidates[0].evidence.overlapTerms).toEqual(["such", "time"])
  })

  it("keeps EGW statements out of Bible candidate ranking", () => {
    const candidates = buildRankingCandidates([
      semantic(),
      semantic({
        content_type: "egw",
        verse_ref: "Patriarchs and Prophets p.74 par.1",
      }),
    ])

    expect(candidates).toHaveLength(1)
    expect(candidates[0].id).toBe("44:16:25")
  })
})

describe("shouldRankDetections", () => {
  const two = [semantic(), semantic({ verse: 26 })]

  it("requires toggle, key, no strong direct hit, and 2+ semantic candidates for DeepSeek", () => {
    expect(shouldRankDetections(two, gate)).toBe(true)
    expect(
      shouldRankDetections(two, { ...gate, aiRankingEnabled: false })
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

  it("requires toggle, key, no strong direct hit, and 2+ semantic candidates for Cerebras", () => {
    expect(shouldRankDetections(two, cerebrasGate)).toBe(true)
    expect(
      shouldRankDetections(two, { ...cerebrasGate, aiRankingEnabled: false })
    ).toBe(false)
    expect(
      shouldRankDetections(two, { ...cerebrasGate, hasCerebrasApiKey: false })
    ).toBe(false)
  })
})

describe("gate: recent strong direct hit", () => {
  const two = [semantic(), semantic({ verse: 26 })]

  it("suppresses ranking after a strong direct hit in a previous batch", async () => {
    noteBatchForGating(
      [
        semantic({
          source: "direct",
          confidence: 0.92,
          verse_ref: "Genesis 2:1",
          book_name: "Genesis",
          book_number: 1,
          chapter: 2,
          verse: 1,
        }),
      ],
      gate
    )

    expect(await rankSemanticDetections(two, gate)).toBeNull()
    expect(invokeTauri).not.toHaveBeenCalled()
  })

  it("resumes ranking once the suppression window has passed", async () => {
    vi.useFakeTimers()
    try {
      noteBatchForGating(
        [semantic({ source: "direct", confidence: 0.92 })],
        gate
      )
      vi.advanceTimersByTime(4001)
      invokeTauri.mockResolvedValue(null)

      await rankSemanticDetections(two, gate)

      expect(invokeTauri).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it("does not suppress after a weak direct hit", async () => {
    noteBatchForGating([semantic({ source: "direct", confidence: 0.5 })], gate)
    invokeTauri.mockResolvedValue(null)

    await rankSemanticDetections(two, gate)

    expect(invokeTauri).toHaveBeenCalledTimes(1)
  })
})

describe("gate: decisive retrieval", () => {
  it("skips a candidate with decisive confidence", () => {
    const decisive = [
      semantic({ confidence: 0.94 }),
      semantic({ verse: 26, confidence: 0.72 }),
    ]
    expect(retrievalIsDecisive(decisive)).toBe(true)
    expect(shouldRankDetections(decisive, gate)).toBe(false)
  })

  it("skips a shortlist with a wide confidence margin", () => {
    const wide = [
      semantic({ confidence: 0.88 }),
      semantic({ verse: 26, confidence: 0.71 }),
    ]
    expect(retrievalIsDecisive(wide)).toBe(true)
    expect(shouldRankDetections(wide, gate)).toBe(false)
  })

  it("still ranks an ambiguous shortlist", () => {
    const ambiguous = [
      semantic({ confidence: 0.78 }),
      semantic({ verse: 26, confidence: 0.74 }),
    ]
    expect(retrievalIsDecisive(ambiguous)).toBe(false)
    expect(shouldRankDetections(ambiguous, gate)).toBe(true)
  })
})

describe("ranking result cache", () => {
  const two = [semantic(), semantic({ verse: 26 })]

  it("reuses an identical transcript and candidate-id set", async () => {
    invokeTauri.mockResolvedValue("44:16:26")

    const first = await rankSemanticDetections(two, gate)
    const second = await rankSemanticDetections([two[1], two[0]], gate)

    expect(first?.verse).toBe(26)
    expect(second?.verse).toBe(26)
    expect(invokeTauri).toHaveBeenCalledTimes(1)
  })

  it("partitions cache between providers", async () => {
    invokeTauri.mockResolvedValue("44:16:26")

    await rankSemanticDetections(two, gate)
    await rankSemanticDetections(two, cerebrasGate)

    expect(invokeTauri).toHaveBeenCalledTimes(2)
  })

  it("caches abstentions too", async () => {
    invokeTauri.mockResolvedValue(null)

    await rankSemanticDetections(two, gate)
    await rankSemanticDetections(two, gate)

    expect(invokeTauri).toHaveBeenCalledTimes(1)
  })

  it("calls again when the shortlist changes", async () => {
    invokeTauri.mockResolvedValue(null)
    await rankSemanticDetections(two, gate)

    await rankSemanticDetections(
      [...two, semantic({ verse: 27, confidence: 0.7 })],
      gate
    )

    expect(invokeTauri).toHaveBeenCalledTimes(2)
  })

  it("calls again when candidate confidence changes", async () => {
    invokeTauri.mockResolvedValue(null)
    await rankSemanticDetections(two, gate)

    await rankSemanticDetections(
      two.map((detection, index) =>
        index === 0 ? { ...detection, confidence: 0.76 } : detection
      ),
      gate
    )

    expect(invokeTauri).toHaveBeenCalledTimes(2)
  })

  it("calls again when candidate verse text changes", async () => {
    invokeTauri.mockResolvedValue(null)
    await rankSemanticDetections(two, gate)

    await rankSemanticDetections(
      two.map((detection, index) =>
        index === 0
          ? { ...detection, verse_text: `${detection.verse_text} updated` }
          : detection
      ),
      gate
    )

    expect(invokeTauri).toHaveBeenCalledTimes(2)
  })

  it("does not cache failures", async () => {
    invokeTauri.mockRejectedValueOnce(new Error("network down"))
    await rankSemanticDetections(two, gate)

    invokeTauri.mockResolvedValue(null)
    await rankSemanticDetections(two, gate)

    expect(invokeTauri).toHaveBeenCalledTimes(2)
  })
})

describe("scheduleRanking stability window", () => {
  const two = [semantic(), semantic({ verse: 26 })]

  it("only ranks the final batch of a rapid burst", async () => {
    vi.useFakeTimers()
    try {
      invokeTauri.mockResolvedValue(null)
      const first = scheduleRanking(two, gate)
      const second = scheduleRanking(two, gate)
      const third = scheduleRanking(two, gate)

      await vi.advanceTimersByTimeAsync(401)
      await Promise.all([first, second, third])

      expect(invokeTauri).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it("reranks the newest batch after an earlier request settles", async () => {
    vi.useFakeTimers()
    try {
      let resolveFirst: (value: unknown) => void = () => {}
      invokeTauri
        .mockReturnValueOnce(
          new Promise((resolve) => {
            resolveFirst = resolve
          })
        )
        .mockResolvedValueOnce(null)

      const first = scheduleRanking(two, gate)
      await vi.advanceTimersByTimeAsync(401)
      expect(invokeTauri).toHaveBeenCalledTimes(1)

      const newer = scheduleRanking(
        two.map((d) => ({
          ...d,
          transcript_snippet: `${d.transcript_snippet} later`,
        })),
        gate
      )
      expect(await first).toBeNull()

      await vi.advanceTimersByTimeAsync(401)
      expect(invokeTauri).toHaveBeenCalledTimes(1)

      resolveFirst(null)
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
      await newer

      expect(invokeTauri).toHaveBeenCalledTimes(2)
    } finally {
      vi.useRealTimers()
    }
  })

  it("does not call before the quiet period elapses", async () => {
    vi.useFakeTimers()
    try {
      invokeTauri.mockResolvedValue(null)
      const pending = scheduleRanking(two, gate)

      await vi.advanceTimersByTimeAsync(399)
      expect(invokeTauri).not.toHaveBeenCalled()

      await vi.advanceTimersByTimeAsync(2)
      await pending
      expect(invokeTauri).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it("records a direct hit even when that batch is superseded", async () => {
    vi.useFakeTimers()
    try {
      void scheduleRanking(
        [semantic({ source: "direct", confidence: 0.92 })],
        gate
      )
      const later = scheduleRanking(two, gate)

      await vi.advanceTimersByTimeAsync(401)
      await later

      expect(invokeTauri).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })
})

describe("preferSpokenBook", () => {
  const exodus = semantic({
    verse_ref: "Exodus 20:8",
    book_name: "Exodus",
    book_number: 2,
    chapter: 20,
    verse: 8,
    confidence: 0.72,
  })
  const deut = semantic({
    verse_ref: "Deuteronomy 5:12",
    book_name: "Deuteronomy",
    book_number: 5,
    chapter: 5,
    verse: 12,
    confidence: 0.81,
  })

  it("promotes the named book without removing other candidates", () => {
    const kept = preferSpokenBook(
      [deut, exodus],
      "a verse in the book of exodus that talks about keeping the sabbath holy"
    )
    expect(kept.map((d) => d.book_number)).toEqual([2, 5])
  })

  it("returns everything when the named book is absent", () => {
    expect(
      preferSpokenBook(
        [deut],
        "a verse in the book of exodus about the sabbath"
      )
    ).toEqual([deut])
  })

  it("does not scope an ambiguous two-book mention", () => {
    expect(
      preferSpokenBook(
        [deut, exodus],
        "compare exodus with deuteronomy on the sabbath"
      )
    ).toEqual([deut, exodus])
  })

  it("does not match a book name inside a longer word", () => {
    expect(preferSpokenBook([deut, exodus], "the exodusing of israel")).toEqual(
      [deut, exodus]
    )
  })

  it("feeds a stable, boosted order to the request", async () => {
    invokeTauri.mockResolvedValue(null)
    await rankSemanticDetections(
      [deut, exodus].map((d) => ({
        ...d,
        transcript_snippet: "a verse in the book of exodus about the sabbath",
      })),
      gate
    )

    const sent = invokeTauri.mock.calls[0][1] as {
      candidates: { id: string }[]
    }
    expect(sent.candidates.map((c) => c.id)).toEqual(["2:20:8", "5:5:12"])
  })
})

describe("pickRankingTranscript", () => {
  it("uses the most common normalized semantic snippet, clamped to 500 chars", () => {
    const detections = [
      semantic({ transcript_snippet: "Peter in prison" }),
      semantic({ verse: 26, transcript_snippet: "  Peter   in prison " }),
      semantic({ verse: 27, transcript_snippet: "unrelated longer context" }),
    ]
    const transcript = pickRankingTranscript(detections)
    expect(transcript).toBe("peter in prison")
  })
})

describe("rankSemanticDetections", () => {
  const two = [semantic(), semantic({ verse: 26 })]

  it("returns the detection whose id Rust selected for DeepSeek", async () => {
    invokeTauri.mockResolvedValue("44:16:26")
    const winner = await rankSemanticDetections(two, gate)
    expect(winner?.verse).toBe(26)
    expect(invokeTauri).toHaveBeenCalledWith(
      "rank_detection_candidates",
      expect.objectContaining({
        transcript: expect.any(String),
        candidates: expect.any(Array),
        provider: "deepseek",
      })
    )
  })

  it("returns the detection whose id Rust selected for Cerebras", async () => {
    invokeTauri.mockResolvedValue("44:16:26")
    const winner = await rankSemanticDetections(two, cerebrasGate)
    expect(winner?.verse).toBe(26)
    expect(invokeTauri).toHaveBeenCalledWith(
      "rank_detection_candidates",
      expect.objectContaining({
        transcript: expect.any(String),
        candidates: expect.any(Array),
        provider: "cerebras",
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

  it("isolates circuit breakers between providers", async () => {
    invokeTauri.mockRejectedValue(new Error("DeepSeek server down"))
    await rankSemanticDetections(two, gate)
    await rankSemanticDetections(two, gate)
    await rankSemanticDetections(two, gate)

    // DeepSeek is now circuit-broken
    invokeTauri.mockClear()
    await rankSemanticDetections(two, gate)
    expect(invokeTauri).not.toHaveBeenCalled()

    // Cerebras should NOT be circuit-broken
    invokeTauri.mockResolvedValue("44:16:26")
    const cerebrasWinner = await rankSemanticDetections(two, cerebrasGate)
    expect(cerebrasWinner?.verse).toBe(26)
    expect(invokeTauri).toHaveBeenCalledTimes(1)
  })

  it("a success resets the failure count", async () => {
    let attempt = 0
    const nextBatch = () =>
      two.map((detection) => ({
        ...detection,
        transcript_snippet: `${detection.transcript_snippet} attempt ${attempt++}`,
      }))

    invokeTauri.mockRejectedValueOnce(new Error("boom"))
    await rankSemanticDetections(nextBatch(), gate)
    invokeTauri.mockResolvedValueOnce(null)
    await rankSemanticDetections(nextBatch(), gate)
    invokeTauri.mockRejectedValue(new Error("boom"))
    await rankSemanticDetections(nextBatch(), gate)
    await rankSemanticDetections(nextBatch(), gate)
    invokeTauri.mockClear()
    invokeTauri.mockResolvedValue(null)
    await rankSemanticDetections(nextBatch(), gate)
    expect(invokeTauri).toHaveBeenCalledTimes(1) // breaker NOT open (2 fails, not 3)
  })
})
