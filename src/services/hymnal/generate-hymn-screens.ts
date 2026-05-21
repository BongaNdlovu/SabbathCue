import type { Hymn, HymnScreen } from "@/types"

export interface GenerateHymnScreensOptions {
  hymn: Hymn
  selectedSectionIds: string[]
  maxLinesPerScreen?: number
}

export function generateHymnScreens({
  hymn,
  selectedSectionIds,
  maxLinesPerScreen = 4,
}: GenerateHymnScreensOptions): HymnScreen[] {
  const screens: HymnScreen[] = []
  const sectionsById = new Map(hymn.sections.map((section) => [section.id, section]))
  const occurrenceBySectionId = new Map<string, number>()

  for (const sectionId of selectedSectionIds) {
    const section = sectionsById.get(sectionId)
    if (!section) continue
    const occurrence = (occurrenceBySectionId.get(section.id) ?? 0) + 1
    occurrenceBySectionId.set(section.id, occurrence)
    const chunks = chunkLines(section.lines, maxLinesPerScreen)

    chunks.forEach((lines, index) => {
      screens.push({
        id: `${section.id}-repeat-${occurrence}-screen-${index + 1}`,
        hymnId: hymn.id,
        hymnNumber: hymn.number,
        hymnTitle: hymn.title,
        sectionId: section.id,
        sectionLabel: section.label,
        sectionKind: section.kind,
        screenIndex: screens.length,
        sectionScreenIndex: index,
        sectionScreenCount: chunks.length,
        totalScreens: 0,
        lines,
      })
    })
  }

  return screens.map((screen) => ({
    ...screen,
    totalScreens: screens.length,
  }))
}

function chunkLines(lines: string[], maxLinesPerScreen: number): string[][] {
  const size = Math.max(1, maxLinesPerScreen)
  const chunks: string[][] = []
  for (let index = 0; index < lines.length; index += size) {
    chunks.push(lines.slice(index, index + size))
  }
  return chunks.length > 0 ? chunks : [[]]
}
