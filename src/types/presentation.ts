import type { Verse } from "./bible"

export type PresentationItemKind = "scripture" | "hymn" | "media" | "slideDeck"

export interface PresentationSegment {
  verseNumber?: number
  text: string
}

export interface PresentationRenderData {
  reference: string
  segments: PresentationSegment[]
  kind?: PresentationItemKind
  hymnSlide?: {
    screenId: string
    slideIndex: number
    slideCount: number
  }
}

export interface ScripturePresentationItemData {
  kind: "scripture"
  verse: Verse
  reference: string
}

export interface HymnPresentationItemData {
  kind: "hymn"
  hymnId: string
  hymnNumber: number
  hymnTitle: string
  screenId: string
  slideIndex: number
  slideCount: number
  reference: string
  segments: PresentationSegment[]
}

export interface MediaPresentationItemData {
  kind: "media"
  mediaId: string
  title: string
  mediaKind: "media" | "slide" | "document" | "deck"
  reference: string
  segments: PresentationSegment[]
}

export type SlideDeckSectionKind =
  | "intro"
  | "verse"
  | "chorus"
  | "bridge"
  | "ending"
  | "custom"

export interface SlideDeckSection {
  id: string
  kind: SlideDeckSectionKind
  label: string
  slideIndexes: number[]
}

export interface SlideDeckSlide {
  id: string
  index: number
  label: string
  path: string
  thumbnailUrl?: string
}

export interface SlideDeck {
  id: string
  title: string
  sourceType: "images" | "pdf" | "powerpoint-export" | "builtin-hymn"
  slides: SlideDeckSlide[]
  sections: SlideDeckSection[]
}

export interface SlideDeckPresentationItemData {
  kind: "slideDeck"
  deckId: string
  deckTitle: string
  slideId: string
  slideIndex: number
  slideCount: number
  slidePath: string
  sectionId?: string
  sectionLabel?: string
  reference: string
  segments: PresentationSegment[]
}

export type PresentationItem =
  | ScripturePresentationItemData
  | HymnPresentationItemData
  | MediaPresentationItemData
  | SlideDeckPresentationItemData

export function getPresentationReference(item: PresentationItem): string {
  return item.reference
}

export function getPresentationRenderData(item: PresentationItem): PresentationRenderData {
  if (item.kind === "scripture") {
    return {
      kind: "scripture",
      reference: item.reference,
      segments: [{ verseNumber: item.verse.verse, text: item.verse.text }],
    }
  }

  if (item.kind === "hymn") {
    return {
      kind: "hymn",
      reference: item.reference,
      segments: item.segments,
      hymnSlide: {
        screenId: item.screenId,
        slideIndex: item.slideIndex,
        slideCount: item.slideCount,
      },
    }
  }

  if (item.kind === "slideDeck") {
    return {
      kind: "slideDeck",
      reference: item.reference,
      segments: item.segments,
      hymnSlide: {
        screenId: item.slideId,
        slideIndex: item.slideIndex,
        slideCount: item.slideCount,
      },
    }
  }

  return {
    kind: "media",
    reference: item.reference,
    segments: item.segments,
  }
}

export function getScriptureVerse(item: PresentationItem): Verse | null {
  return item.kind === "scripture" ? item.verse : null
}
