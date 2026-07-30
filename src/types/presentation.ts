import type { Verse } from "./bible"
import type { EgwParagraph } from "./egw"

export type VideoSourceKind = "local" | "url" | "youtube"

export type PresentationItemKind =
  "scripture" | "hymn" | "media" | "slideDeck" | "egw" | "video"

export interface PresentationSegment {
  verseNumber?: number
  text: string
}

export interface PresentationRenderData {
  reference: string
  segments: PresentationSegment[]
  hymnTitle?: string
  kind?: PresentationItemKind
  scripture?: Verse
  egwParagraph?: EgwParagraph
  slideImageUrl?: string
  /** When true, imported slides keep the theme background in letterbox bars and the theme image tint over the slide. */
  applyTheme?: boolean
  video?: VideoPresentationSource
  hymnSlide?: {
    screenId: string
    slideIndex: number
    slideCount: number
    sectionId?: string
    sectionLabel?: string
    sectionKind?: HymnPresentationSectionKind
    sectionScreenIndex?: number
    sectionScreenCount?: number
  }
}

export interface VideoPresentationSource {
  source: VideoSourceKind
  videoId: string
  title: string
  videoPath?: string
  url?: string
  youtubeId?: string
  poster?: string
  loop?: boolean
  durationMs?: number
  width?: number
  height?: number
  mimeType?: string
}

export interface ScripturePresentationItemData {
  kind: "scripture"
  verse: Verse
  reference: string
}

export interface EgwPresentationItemData {
  kind: "egw"
  paragraph: EgwParagraph
  reference: string
  segments: PresentationSegment[]
  slideId: string
  slideIndex: number
  slideCount: number
}

export interface HymnPresentationItemData {
  kind: "hymn"
  hymnId: string
  hymnNumber: number
  hymnTitle: string
  screenId: string
  slideIndex: number
  slideCount: number
  sectionId?: string
  sectionLabel?: string
  sectionKind?: HymnPresentationSectionKind
  sectionScreenIndex?: number
  sectionScreenCount?: number
  reference: string
  segments: PresentationSegment[]
}

export type HymnPresentationSectionKind =
  "verse" | "refrain" | "chorus" | "bridge" | "ending" | "custom"

export interface MediaPresentationItemData {
  kind: "media"
  mediaId: string
  title: string
  mediaKind: "media" | "slide" | "document" | "deck"
  reference: string
  segments: PresentationSegment[]
}

export interface VideoPresentationItemData {
  kind: "video"
  videoId: string
  title: string
  source: VideoSourceKind
  videoPath?: string
  url?: string
  youtubeId?: string
  poster?: string
  loop?: boolean
  durationMs?: number
  width?: number
  height?: number
  mimeType?: string
  reference: string
  segments: PresentationSegment[]
}

export type SlideDeckSectionKind =
  "intro" | "verse" | "chorus" | "bridge" | "ending" | "custom"

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
  /** Per-deck toggle: render the slide inside the active theme rather than full-bleed on black. */
  applyTheme?: boolean
  extractedTextLines?: string[]
}

export type PresentationItem =
  | ScripturePresentationItemData
  | HymnPresentationItemData
  | MediaPresentationItemData
  | SlideDeckPresentationItemData
  | EgwPresentationItemData
  | VideoPresentationItemData

export function getPresentationRenderData(
  item: PresentationItem
): PresentationRenderData {
  if (item.kind === "scripture") {
    return {
      kind: "scripture",
      reference: item.reference,
      segments: [{ verseNumber: item.verse.verse, text: item.verse.text }],
      scripture: item.verse,
    }
  }

  if (item.kind === "hymn") {
    return {
      kind: "hymn",
      reference: item.reference,
      hymnTitle: item.hymnTitle,
      segments: item.segments,
      hymnSlide: {
        screenId: item.screenId,
        slideIndex: item.slideIndex,
        slideCount: item.slideCount,
        sectionId: item.sectionId,
        sectionLabel: item.sectionLabel,
        sectionKind: item.sectionKind,
        sectionScreenIndex: item.sectionScreenIndex,
        sectionScreenCount: item.sectionScreenCount,
      },
    }
  }

  if (item.kind === "slideDeck") {
    return {
      kind: "slideDeck",
      reference: item.reference,
      segments: item.segments,
      slideImageUrl: item.slidePath,
      applyTheme: item.applyTheme,
      hymnSlide: {
        screenId: item.slideId,
        slideIndex: item.slideIndex,
        slideCount: item.slideCount,
      },
    }
  }

  if (item.kind === "egw") {
    return {
      kind: "egw",
      reference: item.reference,
      segments: item.segments,
      egwParagraph: item.paragraph,
      hymnSlide: {
        screenId: item.slideId,
        slideIndex: item.slideIndex,
        slideCount: item.slideCount,
      },
    }
  }

  if (item.kind === "video") {
    return {
      kind: "video",
      reference: item.reference,
      segments: item.segments,
      video: {
        source: item.source,
        videoId: item.videoId,
        title: item.title,
        videoPath: item.videoPath,
        url: item.url,
        youtubeId: item.youtubeId,
        poster: item.poster,
        loop: item.loop,
        durationMs: item.durationMs,
        width: item.width,
        height: item.height,
        mimeType: item.mimeType,
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
