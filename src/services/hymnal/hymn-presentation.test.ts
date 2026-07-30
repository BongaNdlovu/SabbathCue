import { describe, expect, it } from "vitest"
import { createGroupedHymnQueueItems } from "./hymn-presentation"
import type { HymnScreen } from "@/types"

function makeScreen(index: number): HymnScreen {
  return {
    id: `screen-${index}`,
    hymnId: "hymn-12",
    hymnNumber: 12,
    hymnTitle: "Joyful, Joyful",
    sectionId: "v1",
    sectionLabel: "Verse 1",
    sectionKind: "verse",
    screenIndex: index,
    sectionScreenIndex: index,
    sectionScreenCount: 2,
    totalScreens: 2,
    lines: [`Line ${index + 1}`],
  }
}

describe("hymn presentation queue items", () => {
  it("stores the full hymn deck on every grouped queue item", () => {
    const items = createGroupedHymnQueueItems([makeScreen(0), makeScreen(1)])

    expect(items).toHaveLength(2)
    expect(items[0].hymnDeck).toHaveLength(2)
    expect(items[1].hymnDeck).toBe(items[0].hymnDeck)
    expect(items[0].hymnDeck?.map((slide) => slide.screenId)).toEqual([
      "screen-0",
      "screen-1",
    ])
    expect(items[1].presentation.kind).toBe("hymn")
    if (items[1].presentation.kind !== "hymn") return
    expect(items[1].presentation.screenId).toBe("screen-1")
  })

  it("retains authored hymn section identity in every presentation page", () => {
    const verse = makeScreen(0)
    const refrain: HymnScreen = {
      ...makeScreen(1),
      id: "refrain-screen",
      sectionId: "r1",
      sectionLabel: "Refrain",
      sectionKind: "refrain",
      sectionScreenIndex: 0,
      sectionScreenCount: 1,
    }

    const items = createGroupedHymnQueueItems([verse, refrain])
    const sectionMetadata = items.map((item) => {
      if (item.presentation.kind !== "hymn") return null
      return {
        sectionId: Reflect.get(item.presentation, "sectionId"),
        sectionLabel: Reflect.get(item.presentation, "sectionLabel"),
        sectionKind: Reflect.get(item.presentation, "sectionKind"),
        sectionScreenIndex: Reflect.get(
          item.presentation,
          "sectionScreenIndex"
        ),
        sectionScreenCount: Reflect.get(
          item.presentation,
          "sectionScreenCount"
        ),
      }
    })

    expect(sectionMetadata).toEqual([
      {
        sectionId: "v1",
        sectionLabel: "Verse 1",
        sectionKind: "verse",
        sectionScreenIndex: 0,
        sectionScreenCount: 2,
      },
      {
        sectionId: "r1",
        sectionLabel: "Refrain",
        sectionKind: "refrain",
        sectionScreenIndex: 0,
        sectionScreenCount: 1,
      },
    ])
  })
})
