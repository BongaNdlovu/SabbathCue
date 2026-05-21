import { describe, expect, it } from "vitest"
import { generateHymnScreens } from "./generate-hymn-screens"
import { getHymnByNumber, searchHymns } from "./hymnal-repository"

describe("hymnal services", () => {
  it("loads the generated SDA hymnal data", async () => {
    const hymn = await getHymnByNumber(1)

    expect(hymn).toMatchObject({
      number: 1,
      title: "Praise to the Lord",
    })
    expect(hymn?.sections.length).toBeGreaterThan(0)
  })

  it("searches hymns by number and title", () => {
    expect(searchHymns("1", 1)[0]).toMatchObject({ number: 1 })
    expect(searchHymns("Praise to the Lord", 1)[0]).toMatchObject({ number: 1 })
  })

  it("generates multi-screen hymn presentation chunks", async () => {
    const hymn = await getHymnByNumber(1)
    expect(hymn).not.toBeNull()

    const firstSectionId = hymn!.sections[0].id
    const screens = generateHymnScreens({
      hymn: hymn!,
      selectedSectionIds: [firstSectionId],
      maxLinesPerScreen: 2,
    })

    expect(screens.length).toBeGreaterThan(1)
    expect(screens[0]).toMatchObject({
      hymnNumber: 1,
      sectionLabel: "Verse 1",
      totalScreens: screens.length,
      lines: expect.arrayContaining(["Praise to the Lord, the Almighty, the King of creation!"]),
    })
  })

  it("preserves repeated hymn sections as separate screens", async () => {
    const hymn = await getHymnByNumber(1)
    expect(hymn).not.toBeNull()

    const sectionId = hymn!.sections[0].id
    const screens = generateHymnScreens({
      hymn: hymn!,
      selectedSectionIds: [sectionId, sectionId],
      maxLinesPerScreen: 10,
    })

    expect(screens).toHaveLength(2)
    expect(screens.map((screen) => screen.id)).toEqual([
      `${sectionId}-repeat-1-screen-1`,
      `${sectionId}-repeat-2-screen-1`,
    ])
  })
})
