import { describe, expect, it } from "vitest"

import { getBroadcastRenderKey } from "./broadcast-render-key"
import type { BroadcastTheme, VerseRenderData } from "@/types"

function makeTheme(overrides: Partial<BroadcastTheme> = {}): BroadcastTheme {
  return {
    id: "theme-1",
    name: "Operator Theme",
    builtin: false,
    pinned: false,
    createdAt: 100,
    updatedAt: 200,
    resolution: { width: 1920, height: 1080 },
    background: {
      type: "solid",
      color: "#101010",
      gradient: null,
      image: null,
    },
    textBox: {
      enabled: true,
      color: "#000000",
      opacity: 0.72,
      borderRadius: 8,
      padding: 32,
    },
    verseText: {
      fontFamily: "Inter",
      fontSize: 72,
      fontWeight: 700,
      color: "#ffffff",
      lineHeight: 1.18,
      letterSpacing: 0,
      shadow: { color: "#000000", blur: 12, x: 0, y: 4 },
      outline: null,
    },
    verseNumbers: {
      visible: true,
      fontSize: 32,
      color: "#f1f5f9",
      superscript: true,
    },
    reference: {
      fontFamily: "Inter",
      fontSize: 38,
      fontWeight: 600,
      color: "#f8fafc",
      uppercase: true,
      letterSpacing: 1,
      position: "below",
    },
    layout: {
      anchor: "bottom-center",
      offsetX: 0,
      offsetY: -80,
      padding: { top: 40, right: 64, bottom: 40, left: 64 },
      textAlign: "center",
      backgroundWidth: 1480,
      backgroundHeight: 420,
      textAreaWidth: 1320,
      textAreaHeight: 280,
      referenceGap: 24,
    },
    transition: {
      type: "fade",
      duration: 250,
      easing: "ease-out",
      direction: "up",
    },
    ...overrides,
  }
}

const verse: VerseRenderData = {
  reference: "John 3:16",
  segments: [{ verseNumber: 16, text: "For God so loved the world" }],
}

describe("getBroadcastRenderKey", () => {
  it("changes when visible theme render settings change", () => {
    const baseKey = getBroadcastRenderKey(makeTheme(), verse)
    const changedKey = getBroadcastRenderKey(
      makeTheme({
        verseText: {
          ...makeTheme().verseText,
          color: "#ffcc00",
        },
      }),
      verse
    )

    expect(changedKey).not.toBe(baseKey)
  })

  it("changes when verse content changes", () => {
    const baseKey = getBroadcastRenderKey(makeTheme(), verse)
    const changedKey = getBroadcastRenderKey(makeTheme(), {
      reference: "Romans 8:28",
      segments: [{ verseNumber: 28, text: "All things work together" }],
    })

    expect(changedKey).not.toBe(baseKey)
  })

  it("ignores library-only metadata that does not affect rendered pixels", () => {
    const baseKey = getBroadcastRenderKey(makeTheme(), verse)
    const metadataOnlyKey = getBroadcastRenderKey(
      makeTheme({
        name: "Renamed In Library",
        pinned: true,
        createdAt: 999,
        transition: {
          type: "slide",
          duration: 900,
          easing: "ease-in-out",
          direction: "left",
        },
      }),
      verse
    )

    expect(metadataOnlyKey).toBe(baseKey)
  })

  it("distinguishes a cleared verse from a rendered verse", () => {
    expect(getBroadcastRenderKey(makeTheme(), null)).not.toBe(
      getBroadcastRenderKey(makeTheme(), verse)
    )
  })
})
