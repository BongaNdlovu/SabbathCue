// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest"
import {
  MAX_COALESCED_SEGMENT_WORDS,
  SPEECHMATICS_COALESCE_GAP_MS,
  useTranscriptStore,
} from "./transcript-store"
import type { TranscriptSegment } from "@/types"

function speechmaticsFinal(
  id: string,
  text: string,
  wordCount: number,
  timestamp: number,
): TranscriptSegment {
  return {
    id,
    text,
    is_final: true,
    confidence: 0.9,
    words: Array.from({ length: wordCount }, (_, index) => ({
      text: `w${index}`,
      start: 0,
      end: 0.1,
      confidence: 0.9,
      punctuated: `w${index}`,
    })),
    timestamp,
    provider: "speechmatics",
  }
}

describe("transcript-store Speechmatics coalescing", () => {
  beforeEach(() => {
    useTranscriptStore.getState().clearTranscript()
    useTranscriptStore.setState({ lastIssue: null })
  })

  it("coalesces back-to-back speechmatics finals below the word budget", () => {
    const store = useTranscriptStore.getState()
    store.addSegment(speechmaticsFinal("a", "first part", 2, 1_000))
    store.addSegment(speechmaticsFinal("b", "second part", 3, 2_000))

    const segments = useTranscriptStore.getState().segments
    expect(segments).toHaveLength(1)
    expect(segments[0].text).toBe("first part second part")
    expect(segments[0].words).toHaveLength(5)
  })

  it("splits a new segment once the coalesced word budget would be exceeded", () => {
    // Continuous preaching used to merge the entire sermon into ONE segment:
    // finals always arrived inside the coalesce gap, so the 100-segment cap
    // never applied and text/words grew without bound.
    const half = Math.floor(MAX_COALESCED_SEGMENT_WORDS / 2)
    const store = useTranscriptStore.getState()
    store.addSegment(speechmaticsFinal("a", "half one", half, 1_000))
    store.addSegment(speechmaticsFinal("b", "half two", half, 2_000))
    store.addSegment(speechmaticsFinal("c", "overflow", 5, 3_000))

    const segments = useTranscriptStore.getState().segments
    expect(segments).toHaveLength(2)
    expect(segments[0].words).toHaveLength(MAX_COALESCED_SEGMENT_WORDS)
    expect(segments[1].text).toBe("overflow")
  })

  it("does not coalesce across the gap or across providers", () => {
    const store = useTranscriptStore.getState()
    store.addSegment(speechmaticsFinal("a", "one", 1, 1_000))
    // Outside the coalesce gap.
    store.addSegment(
      speechmaticsFinal("b", "two", 1, 1_000 + SPEECHMATICS_COALESCE_GAP_MS + 1),
    )
    // Different provider inside the gap.
    store.addSegment({
      ...speechmaticsFinal("c", "three", 1, 1_000 + SPEECHMATICS_COALESCE_GAP_MS + 2),
      provider: "deepgram",
    })

    expect(useTranscriptStore.getState().segments).toHaveLength(3)
  })

  it("caps metadata-only Speechmatics text when word arrays are empty", () => {
    const longText = Array.from({ length: 120 }, (_, index) => `word${index}`).join(" ")
    const store = useTranscriptStore.getState()
    store.addSegment(speechmaticsFinal("a", longText, 0, 1_000))
    store.addSegment(speechmaticsFinal("b", longText, 0, 2_000))

    const segments = useTranscriptStore.getState().segments
    expect(segments).toHaveLength(2)
    expect(segments[0].text.split(/\s+/)).toHaveLength(120)
    expect(segments[1].text.split(/\s+/)).toHaveLength(120)
  })
})
