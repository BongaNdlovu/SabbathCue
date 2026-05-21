import type { PresentationItem } from "./presentation"
import {
  getPresentationReference,
  getScriptureVerse,
} from "./presentation"

export interface QueueItem {
  id: string
  presentation: PresentationItem
  confidence: number
  source: "manual" | "hymn" | "ai-direct" | "ai-semantic" | "ai-cloud"
  added_at: number
  /** True when queued from a chapter-only detection (verse defaults to 1, may be refined). */
  is_chapter_only?: boolean
  /** Optional grouping metadata for hymn queue items to display grouped screens in the queue. */
  hymnGroup?: {
    /** Unique group identifier for all screens from the same hymn. */
    groupId: string
    /** Display label for the group (e.g., "#1 Praise to the Lord - 6 screens"). */
    groupLabel: string
    /** 1-based index of this item within the group. */
    itemIndex: number
    /** Total number of items in the group. */
    itemCount: number
  }
}

export function getVerseFromItem(item: QueueItem) {
  return getScriptureVerse(item.presentation)
}

export function getReferenceFromItem(item: QueueItem) {
  return getPresentationReference(item.presentation)
}
