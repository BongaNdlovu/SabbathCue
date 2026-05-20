import { bibleActions } from "@/hooks/use-bible"
import { toVerseRenderData } from "@/hooks/use-broadcast"
import { useBibleStore } from "@/stores/bible-store"
import { useBroadcastStore } from "@/stores/broadcast-store"
import type { DetectionResult, Verse } from "@/types"

function activeTranslationLabel(): string {
  const bible = useBibleStore.getState()
  return (
    bible.translations.find((t) => t.id === bible.activeTranslationId)
      ?.abbreviation ?? "KJV"
  )
}

export function detectionToVerse(detection: DetectionResult): Verse {
  return {
    id: 0,
    translation_id: useBibleStore.getState().activeTranslationId,
    book_number: detection.book_number,
    book_name: detection.book_name,
    book_abbreviation: "",
    chapter: detection.chapter,
    verse: detection.verse,
    text: detection.verse_text,
  }
}

export function selectPreviewVerse(verse: Verse, options?: { navigate?: boolean }) {
  bibleActions.selectVerse(verse)

  if (options?.navigate && verse.book_number > 0) {
    bibleActions.navigateToVerse(
      verse.book_number,
      verse.chapter,
      verse.verse,
    )
  }
}

export function commitVerseToLive(verse: Verse, options?: { makeLive?: boolean }) {
  const renderData = toVerseRenderData(verse, activeTranslationLabel())
  const broadcast = useBroadcastStore.getState()

  console.info("[pipeline] commit_live", { reference: renderData.reference })
  useBroadcastStore.setState({ liveVerse: renderData })

  if (options?.makeLive ?? true) {
    broadcast.setLive(true)
  } else {
    broadcast.syncBroadcastOutput()
  }
}

export function commitPreviewToLive(): boolean {
  const verse = useBibleStore.getState().selectedVerse
  if (!verse) return false

  commitVerseToLive(verse, { makeLive: true })
  return true
}

export function presentVerse(verse: Verse, options?: { navigate?: boolean }) {
  selectPreviewVerse(verse, { navigate: options?.navigate })
  commitVerseToLive(verse, { makeLive: true })
}

export function previewVerseAndMaybeAutoLive(
  verse: Verse,
  options?: {
    navigate?: boolean
    autoLiveWhenAlreadyOn?: boolean
  },
) {
  const broadcast = useBroadcastStore.getState()
  const shouldAutoLive =
    options?.autoLiveWhenAlreadyOn &&
    broadcast.isLive &&
    broadcast.readingModeAutoLive

  if (shouldAutoLive) {
    commitVerseToLive(verse, { makeLive: false })
  }

  selectPreviewVerse(verse, { navigate: options?.navigate })
  console.info("[pipeline] preview", { reference: `${verse.book_name} ${verse.chapter}:${verse.verse}` })
}
