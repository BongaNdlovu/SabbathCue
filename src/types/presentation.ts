import type { Verse } from "./bible"

export type PresentationItemKind = "scripture" | "hymn" | "media"

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
  mediaKind: "media" | "slide" | "document"
  reference: string
  segments: PresentationSegment[]
}

export type PresentationItem =
  | ScripturePresentationItemData
  | HymnPresentationItemData
  | MediaPresentationItemData

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

  return {
    kind: "media",
    reference: item.reference,
    segments: item.segments,
  }
}

export function getScriptureVerse(item: PresentationItem): Verse | null {
  return item.kind === "scripture" ? item.verse : null
}
