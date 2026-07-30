import { describe, expect, it } from "vitest"
import { parseAuthoredSongPages } from "./song-slide-pages"

describe("parseAuthoredSongPages", () => {
  it("keeps four-line verses and refrains on their own authored pages", () => {
    const pages = parseAuthoredSongPages(`Verse 1
Line one
Line two
Line three
Line four

Refrain
Sing one
Sing two
Sing three
Sing four`)

    expect(pages).toEqual([
      ["Verse 1", "Line one", "Line two", "Line three", "Line four"],
      ["Refrain", "Sing one", "Sing two", "Sing three", "Sing four"],
    ])
  })

  it("accepts an explicit separator without creating empty pages", () => {
    expect(parseAuthoredSongPages("First page\n\n---\n\nSecond page")).toEqual([
      ["First page"],
      ["Second page"],
    ])
  })
})
