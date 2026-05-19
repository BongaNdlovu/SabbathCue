import { describe, expect, it } from "vitest"
import type { Verse } from "@/types"
import { toVerseRenderData } from "./use-broadcast"

const sampleVerse: Verse = {
  id: 1,
  translation_id: 1,
  book_number: 1,
  book_name: "Genesis",
  book_abbreviation: "Gen",
  chapter: 1,
  verse: 2,
  text: "The earth was without form and void.",
}

describe("toVerseRenderData", () => {
  it("formats a verse for preview and live rendering", () => {
    const result = toVerseRenderData(sampleVerse, "NKJV")

    expect(result).toEqual({
      reference: "Genesis 1:2 (NKJV)",
      segments: [{ verseNumber: 2, text: "The earth was without form and void." }],
    })
  })
})
