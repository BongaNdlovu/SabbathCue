import {
  previewVerseAndMaybeAutoLive,
  selectPreviewVerse,
} from "@/lib/presentation-workflow"
import {
  useBibleStore,
  useDetectionStore,
  useQueueStore,
} from "@/stores"
import type { DetectionResult, ReadingAdvance, Verse } from "@/types"

function detectionLikeToVerse({
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
}): Verse {
  return {
    id: 0,
    translation_id: useBibleStore.getState().activeTranslationId,
    book_number,
    book_name,
    book_abbreviation: "",
    chapter,
    verse,
    text: verse_text,
  }
}

function selectDetectedVerse(args: {
  book_number: number
  book_name: string
  chapter: number
  verse: number
  verse_text: string
}) {
  const verse = detectionLikeToVerse(args)
  selectPreviewVerse(verse)
  useBibleStore.getState().setPendingNavigation({
    bookNumber: args.book_number,
    chapter: args.chapter,
    verse: args.verse,
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

  const verse = detectionLikeToVerse({
    book_number: advance.book_number,
    book_name: advance.book_name,
    chapter: advance.chapter,
    verse: advance.verse,
    verse_text: advance.verse_text,
  })

  previewVerseAndMaybeAutoLive(verse, {
    autoLiveWhenAlreadyOn: true,
  })

  useBibleStore.getState().setPendingNavigation({
    bookNumber: advance.book_number,
    chapter: advance.chapter,
    verse: advance.verse,
  })
}
