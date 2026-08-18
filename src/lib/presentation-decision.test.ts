import { describe, expect, it } from "vitest"
import {
  mayAutoQueue,
  mayGoLive,
  mayPreview,
  mayStartReading,
} from "./presentation-decision"
import type { DetectionResult } from "@/types"

function detection(
  overrides: Partial<DetectionResult> = {}
): DetectionResult {
  return {
    verse_ref: "John 3:16",
    verse_text: "For God so loved the world",
    book_name: "John",
    book_number: 43,
    chapter: 3,
    verse: 16,
    confidence: 0.99,
    source: "direct",
    auto_queued: true,
    transcript_snippet: "John 3:16",
    is_chapter_only: false,
    authorization: "live-authorized",
    job: "citation",
    ...overrides,
  }
}

describe("presentation decision grants", () => {
  it("does not let missing authorization start reading or go live", () => {
    const bare = detection({ authorization: undefined, job: undefined })
    expect(mayStartReading(bare)).toBe(false)
    expect(mayGoLive(bare)).toBe(false)
    expect(mayPreview(bare)).toBe(false)
  })

  it("never starts reading from a quotation even when live-authorized", () => {
    const quote = detection({
      source: "semantic",
      authorization: "live-authorized",
      job: "quotation",
      has_lexical_quote: true,
    })
    expect(mayGoLive(quote)).toBe(true)
    expect(mayPreview(quote)).toBe(true)
    expect(mayStartReading(quote)).toBe(false)
    expect(mayAutoQueue(quote)).toBe(false)
  })

  it("never starts reading from a request", () => {
    const request = detection({
      source: "semantic",
      authorization: "preview-authorized",
      job: "request",
    })
    expect(mayPreview(request)).toBe(true)
    expect(mayStartReading(request)).toBe(false)
    expect(mayGoLive(request)).toBe(false)
  })

  it("allows a complete citation to start reading and go live", () => {
    const citation = detection()
    expect(mayStartReading(citation)).toBe(true)
    expect(mayGoLive(citation)).toBe(true)
    expect(mayAutoQueue(citation)).toBe(true)
  })
})
