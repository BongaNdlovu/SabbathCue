import type { Hymn, HymnScreen, QueueItem } from "@/types"
import type { HymnPresentationItemData } from "@/types"

export function createHymnPresentationItem(
  screen: HymnScreen
): HymnPresentationItemData {
  const label =
    screen.sectionScreenCount > 1
      ? `${screen.sectionLabel} ${screen.sectionScreenIndex + 1} of ${screen.sectionScreenCount}`
      : screen.sectionLabel

  return {
    kind: "hymn",
    hymnId: screen.hymnId,
    hymnNumber: screen.hymnNumber,
    hymnTitle: screen.hymnTitle,
    screenId: screen.id,
    slideIndex: screen.screenIndex,
    slideCount: screen.totalScreens,
    sectionId: screen.sectionId,
    sectionLabel: screen.sectionLabel,
    sectionKind: screen.sectionKind,
    sectionScreenIndex: screen.sectionScreenIndex,
    sectionScreenCount: screen.sectionScreenCount,
    reference: `#${screen.hymnNumber} ${screen.hymnTitle} - ${label}`,
    segments: [{ text: screen.lines.join("\n") }],
  }
}

export function createHymnQueueItem(screen: HymnScreen): QueueItem {
  return {
    id: `hymn-${screen.hymnNumber}-${screen.id}-${crypto.randomUUID()}`,
    presentation: createHymnPresentationItem(screen),
    confidence: 1,
    source: "hymn",
    added_at: Date.now(),
  }
}

export function createHymnDeckQueueItems(
  deck: HymnPresentationItemData[],
  options?: {
    groupId?: string
    groupLabel?: string
    source?: QueueItem["source"]
    idPrefix?: string
  }
): QueueItem[] {
  if (deck.length === 0) return []

  const firstSlide = deck[0]
  const groupId =
    options?.groupId ?? `hymn-group-${firstSlide.hymnId}-${Date.now()}`
  const groupLabel =
    options?.groupLabel ??
    `#${firstSlide.hymnNumber} ${firstSlide.hymnTitle} - ${deck.length} screens`
  const idPrefix = options?.idPrefix ?? "hymn"

  return deck.map((presentation, index) => ({
    id: `${idPrefix}-${presentation.hymnId}-${presentation.screenId}-${crypto.randomUUID()}`,
    presentation,
    confidence: 1,
    source: options?.source ?? "hymn",
    added_at: Date.now(),
    hymnGroup: {
      groupId,
      groupLabel,
      itemIndex: index + 1,
      itemCount: deck.length,
    },
    hymnDeck: deck,
  }))
}

export function createGroupedHymnQueueItems(
  screens: HymnScreen[]
): QueueItem[] {
  if (screens.length === 0) return []

  const firstScreen = screens[0]
  const groupId = `hymn-group-${firstScreen.hymnId}-${Date.now()}`
  const groupLabel = `#${firstScreen.hymnNumber} ${firstScreen.hymnTitle} - ${screens.length} screens`
  const deck = screens.map(createHymnPresentationItem)

  return createHymnDeckQueueItems(deck, { groupId, groupLabel })
}

export function defaultSelectedSectionIds(hymn: Hymn): string[] {
  return hymn.sections.map((section) => section.id)
}
