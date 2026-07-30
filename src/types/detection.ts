import type { EgwParagraph } from "./egw"

/** Spoken-hymn detection payload (frontend-only; populated by hymn voice control). */
export interface HymnDetection {
  number: number
  id: string
  title: string
}

export interface DetectionResult {
  content_type?: "bible" | "egw" | "hymn"
  verse_ref: string
  verse_text: string
  book_name: string
  book_number: number
  chapter: number
  verse: number
  confidence: number
  /** Internal ordering evidence; confidence remains operator-facing match strength. */
  rank_score?: number
  source: "direct" | "semantic"
  auto_queued: boolean
  transcript_snippet: string
  /** True when detected from a chapter-only reference (verse defaults to 1, may be refined). */
  is_chapter_only: boolean
  egw_paragraph?: EgwParagraph | null
  hymn?: HymnDetection | null
}

export interface ReadingAdvance {
  book_number: number
  book_name: string
  chapter: number
  verse: number
  verse_text: string
  reference: string
  confidence: number
}

export interface DetectionStatus {
  has_direct: boolean
  has_semantic: boolean
  paraphrase_enabled: boolean
  bible_detection_enabled: boolean
  semantic_detection_enabled: boolean
}

export interface SemanticSearchResult {
  verse_ref: string
  verse_text: string
  book_name: string
  book_number: number
  chapter: number
  verse: number
  similarity: number
}
