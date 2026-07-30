import { describe, expect, it } from "vitest"
import { BUILTIN_THEMES } from "@/lib/builtin-themes"
import { resolveHymnSectionTheme } from "./hymn-theme-style"
import type { BroadcastTheme, PresentationRenderData } from "@/types"

function baseTheme(): BroadcastTheme {
  const theme = BUILTIN_THEMES[0]
  return {
    ...theme,
    hymnPresentation: {
      sectionStyles: {
        refrain: {
          verseText: {
            color: "#ffd79b",
            fontStyle: "italic",
            fontSize: 82,
          },
        },
      },
    },
  }
}

function hymnData(
  sectionKind: NonNullable<
    NonNullable<PresentationRenderData["hymnSlide"]>["sectionKind"]
  >
): PresentationRenderData {
  return {
    kind: "hymn",
    reference: "Hymn",
    segments: [{ text: "Line" }],
    hymnSlide: {
      screenId: "screen-1",
      slideIndex: 0,
      slideCount: 2,
      sectionKind,
    },
  }
}

describe("resolveHymnSectionTheme", () => {
  it("applies refrain typography without mutating the stored theme", () => {
    const theme = baseTheme()
    const resolved = resolveHymnSectionTheme(theme, hymnData("refrain"))

    expect(resolved.verseText).toMatchObject({
      color: "#ffd79b",
      fontStyle: "italic",
      fontSize: 82,
    })
    expect(theme.verseText.color).not.toBe("#ffd79b")
  })

  it("uses refrain styling for chorus pages", () => {
    expect(
      resolveHymnSectionTheme(baseTheme(), hymnData("chorus")).verseText.color
    ).toBe("#ffd79b")
  })

  it("returns the original theme for non-hymn content", () => {
    const theme = baseTheme()
    expect(
      resolveHymnSectionTheme(theme, {
        kind: "scripture",
        reference: "John 3:16",
        segments: [{ text: "Text" }],
      })
    ).toBe(theme)
  })
})
