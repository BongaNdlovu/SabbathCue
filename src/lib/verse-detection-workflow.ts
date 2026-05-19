import { bibleActions } from "@/hooks/use-bible"
import {
  useBibleStore,
  useDetectionStore,
  useQueueStore,
} from "@/stores"
import type { DetectionResult, ReadingAdvance } from "@/types"

function selectDetectedVerse({
  book_number,
  book_name,
  chapter,
  verse,
  verse_text,
}: {
  book_number: number
  book_name: string
  chapter: number
  verse: number
  verse_text: string
}) {
  bibleActions.selectVerse({
    id: 0,
    translation_id: useBibleStore.getState().activeTranslationId,
    book_number,
    book_name,
    book_abbreviation: "",
    chapter,
    verse,
    text: verse_text,
  })
  useBibleStore.getState().setPendingNavigation({
    bookNumber: book_number,
    chapter,
    verse,
  })
}

export function handleVerseDetections(detections: DetectionResult[]) {
  useDetectionStore.getState().addDetections(detections)

  const directHit = detections.find(
    (d) => d.source === "direct" && !d.is_chapter_only
  )
  if (directHit && directHit.book_number > 0) {
    selectDetectedVerse(directHit)
  }

  for (const d of detections) {
    if (
      !d.is_chapter_only &&
      d.source === "direct" &&
      useQueueStore
        .getState()
        .updateEarlyRef(
          d.book_number,
          d.chapter,
          d.verse,
          d.verse_ref,
          d.verse_text
        )
    ) {
      continue
    }

    if (d.auto_queued) {
      const queue = useQueueStore.getState()
      queue.addOrFlashDetectionItem({
        id: crypto.randomUUID(),
        verse: {
          id: 0,
          translation_id: useBibleStore.getState().activeTranslationId,
          book_number: d.book_number,
          book_name: d.book_name,
          book_abbreviation: "",
          chapter: d.chapter,
          verse: d.verse,
          text: d.verse_text,
        },
        reference: d.verse_ref,
        confidence: d.confidence,
        source: d.source === "direct" ? "ai-direct" : "ai-semantic",
        added_at: Date.now(),
        is_chapter_only: d.is_chapter_only,
      })
    }
  }
}

export function handleReadingAdvance(advance: ReadingAdvance) {
  if (advance.book_number <= 0) return

  selectDetectedVerse({
    book_number: advance.book_number,
    book_name: advance.book_name,
    chapter: advance.chapter,
    verse: advance.verse,
    verse_text: advance.verse_text,
  })
}
