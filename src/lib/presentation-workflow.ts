import { bibleActions } from "@/hooks/use-bible"
import { toVerseRenderData } from "@/hooks/use-broadcast"
import { invokeTauri } from "@/lib/tauri-runtime"
import { useBibleStore } from "@/stores/bible-store"
import { getBroadcastLiveStore } from "@/stores/broadcast/live-store"
import type {
  DetectionResult,
  Verse,
  QueueItem,
  PresentationItem,
  PresentationRenderData,
  ScripturePresentationItemData,
  EgwParagraph,
} from "@/types"
import { getPresentationRenderData, getScriptureVerse } from "@/types"
import {
  createEgwDeckItems,
  createEgwPresentationItem,
  egwReference,
} from "@/lib/egw-deck"
import { useEgwSlideStore } from "@/stores/egw-slide-store"

export { createEgwDeckItems, createEgwPresentationItem, egwReference }
import {
  recordWorkflowTrace,
  tracePresentationDetails,
  traceVerseDetails,
} from "@/lib/workflow-trace"

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

export function createPresentationItem(
  verse: Verse,
  reference?: string
): ScripturePresentationItemData {
  return {
    kind: "scripture",
    verse,
    reference:
      reference ?? `${verse.book_name} ${verse.chapter}:${verse.verse}`,
  }
}

export function createScriptureQueueItem(
  verse: Verse,
  options?: {
    reference?: string
    confidence?: number
    source?: QueueItem["source"]
    is_chapter_only?: boolean
  }
): QueueItem {
  return {
    id: crypto.randomUUID(),
    presentation: createPresentationItem(verse, options?.reference),
    confidence: options?.confidence ?? 1,
    source: options?.source ?? "manual",
    added_at: Date.now(),
    is_chapter_only: options?.is_chapter_only,
  }
}

export function selectPreviewVerse(
  verse: Verse,
  options?: { navigate?: boolean }
) {
  const item = createPresentationItem(verse)
  const renderData = toScriptureRenderData(item)
  getBroadcastLiveStore().setPreviewItem(renderData)
  bibleActions.selectVerse(verse)
  recordWorkflowTrace("preview.selected", "Verse selected for preview", {
    navigate: Boolean(options?.navigate),
    verse: traceVerseDetails(verse),
    preview: tracePresentationDetails(renderData),
  })

  if (options?.navigate && verse.book_number > 0) {
    bibleActions.navigateToVerse(verse.book_number, verse.chapter, verse.verse)
  }
}

export function selectPreviewItem(
  item: PresentationItem,
  options?: { navigate?: boolean }
) {
  const verse = getScriptureVerse(item)
  const renderData = toPresentationRenderData(item)
  getBroadcastLiveStore().setPreviewItem(renderData)
  recordWorkflowTrace("preview.selected", "Item selected for preview", {
    navigate: Boolean(options?.navigate),
    preview: tracePresentationDetails(renderData),
  })

  if (verse) {
    bibleActions.selectVerse(verse)
    if (options?.navigate && verse.book_number > 0) {
      bibleActions.navigateToVerse(
        verse.book_number,
        verse.chapter,
        verse.verse
      )
    }
  }
}

function toScriptureRenderData(
  item: ScripturePresentationItemData
): PresentationRenderData {
  return toVerseRenderData(item.verse, activeTranslationLabel())
}

function toPresentationRenderData(
  item: PresentationItem
): PresentationRenderData {
  if (item.kind === "scripture") return toScriptureRenderData(item)
  return getPresentationRenderData(item)
}

function commitRenderDataToLive(
  renderData: PresentationRenderData,
  options?: { makeLive?: boolean }
) {
  const broadcast = getBroadcastLiveStore()
  recordWorkflowTrace("live.commit", "Presentation committed to live", {
    makeLive: options?.makeLive ?? true,
    liveWasOn: broadcast.isLive,
    live: tracePresentationDetails(renderData),
  })
  getBroadcastLiveStore().commitLiveItem(renderData, options)
}

export function commitVerseToLive(
  verse: Verse,
  options?: { makeLive?: boolean }
) {
  const renderData = toPresentationRenderData(createPresentationItem(verse))
  commitRenderDataToLive(renderData, options)
}

export function commitPreviewToLive(): boolean {
  const previewItem =
    getBroadcastLiveStore().previewItem ??
    (() => {
      const verse = useBibleStore.getState().selectedVerse
      return verse
        ? toPresentationRenderData(createPresentationItem(verse))
        : null
    })()
  if (!previewItem) return false

  commitRenderDataToLive(previewItem)
  return true
}

export function presentItem(
  item: PresentationItem,
  options?: { navigate?: boolean }
) {
  selectPreviewItem(item, { navigate: options?.navigate })
  const renderData = toPresentationRenderData(item)
  commitRenderDataToLive(renderData)
}

export function presentVerse(verse: Verse, options?: { navigate?: boolean }) {
  selectPreviewVerse(verse, { navigate: options?.navigate })
  commitVerseToLive(verse, { makeLive: true })
}

/**
 * How long to wait before staging a single-digit verse to preview/live.
 *
 * Soniox/Deepgram partials often emit the first digit of a multi-digit verse
 * ("Matthew chapter 6 verse 3…" → Matthew 6:3) before the full number arrives
 * ("…thirty three" → Matthew 6:33). Committing immediately makes the wrong
 * verse flash first. Single-digit citations are held briefly so a digit-prefix
 * extension can replace them; multi-digit verses commit immediately.
 */
export const DIGIT_GROWTH_HOLD_MS = 900

/** Bible verses never exceed 176, so a single digit 1–9 can still grow. */
export function verseDigitsCouldGrow(verse: number): boolean {
  return verse > 0 && verse <= 9
}

/** True when `longer` is `shorter` with extra trailing digits (3→33, 1→15). */
export function isDigitPrefixExtension(shorter: number, longer: number): boolean {
  if (shorter <= 0 || longer <= shorter) return false
  const shortText = String(shorter)
  const longText = String(longer)
  return longText.startsWith(shortText) && longText.length > shortText.length
}

type PendingDigitGrowth = {
  book_number: number
  chapter: number
  verse: number
  verseData: Verse
  options?: {
    navigate?: boolean
    autoLive?: boolean
  }
  timer: ReturnType<typeof setTimeout>
}

let pendingDigitGrowth: PendingDigitGrowth | null = null

function clearPendingDigitGrowthTimer(): void {
  if (!pendingDigitGrowth) return
  clearTimeout(pendingDigitGrowth.timer)
  pendingDigitGrowth = null
}

/** Test helper — cancel any hold so cases do not leak timers. */
export function resetDigitGrowthHoldForTests(): void {
  clearPendingDigitGrowthTimer()
}

function commitVersePreviewAndMaybeAutoLive(
  verse: Verse,
  options?: {
    navigate?: boolean
    autoLive?: boolean
  }
): void {
  const broadcast = getBroadcastLiveStore()

  // Auto-live turns the live output on (and keeps it following) when the
  // operator has the auto-live toggle enabled.
  if (options?.autoLive && broadcast.readingModeAutoLive) {
    recordWorkflowTrace("live.auto_commit", "Auto-live committed verse live", {
      liveWasOn: broadcast.isLive,
      readingModeAutoLive: broadcast.readingModeAutoLive,
      verse: traceVerseDetails(verse),
    })
    commitVerseToLive(verse, { makeLive: true })
    void invokeTauri("set_reading_mode_reference", {
      bookNumber: verse.book_number,
      bookName: verse.book_name,
      chapter: verse.chapter,
      verse: verse.verse,
    }).catch((error) => {
      console.warn("[reading-mode] Could not align live verse context", error)
    })
  }

  selectPreviewVerse(verse, { navigate: options?.navigate })
}

function scheduleDigitGrowthHold(
  verse: Verse,
  options?: {
    navigate?: boolean
    autoLive?: boolean
  }
): void {
  clearPendingDigitGrowthTimer()
  const book_number = verse.book_number
  const chapter = verse.chapter
  const verseNum = verse.verse
  recordWorkflowTrace(
    "live.digit_growth_hold",
    "Holding single-digit verse for possible STT digit growth",
    {
      holdMs: DIGIT_GROWTH_HOLD_MS,
      verse: traceVerseDetails(verse),
      autoLive: Boolean(options?.autoLive),
    }
  )
  const timer = setTimeout(() => {
    const pending = pendingDigitGrowth
    if (
      !pending ||
      pending.book_number !== book_number ||
      pending.chapter !== chapter ||
      pending.verse !== verseNum
    ) {
      return
    }
    pendingDigitGrowth = null
    commitVersePreviewAndMaybeAutoLive(pending.verseData, pending.options)
  }, DIGIT_GROWTH_HOLD_MS)
  pendingDigitGrowth = {
    book_number,
    chapter,
    verse: verseNum,
    verseData: verse,
    options,
    timer,
  }
}

export function previewVerseAndMaybeAutoLive(
  verse: Verse,
  options?: {
    navigate?: boolean
    autoLive?: boolean
  }
) {
  // Auto-path only: manual present/select is intentional and must stay instant.
  if (
    options?.autoLive &&
    verse.book_number > 0 &&
    verse.chapter > 0 &&
    verse.verse > 0
  ) {
    const pending = pendingDigitGrowth
    if (
      pending &&
      pending.book_number === verse.book_number &&
      pending.chapter === verse.chapter
    ) {
      if (verse.verse === pending.verse) {
        // Same citation refreshed — keep the existing hold.
        pending.verseData = verse
        pending.options = options
        return
      }
      if (isDigitPrefixExtension(pending.verse, verse.verse)) {
        // e.g. 6:3 → 6:33. Drop the intermediate and either hold again or commit.
        clearPendingDigitGrowthTimer()
        if (verseDigitsCouldGrow(verse.verse)) {
          scheduleDigitGrowthHold(verse, options)
          return
        }
        commitVersePreviewAndMaybeAutoLive(verse, options)
        return
      }
      // Same chapter, unrelated verse (3 then 16) — abandon the hold.
      clearPendingDigitGrowthTimer()
    } else if (pending) {
      // New book/chapter while a hold is open — drop the stale hold so it
      // cannot commit after the operator has already moved on.
      clearPendingDigitGrowthTimer()
    }

    if (verseDigitsCouldGrow(verse.verse)) {
      scheduleDigitGrowthHold(verse, options)
      return
    }
  } else if (pendingDigitGrowth) {
    clearPendingDigitGrowthTimer()
  }

  commitVersePreviewAndMaybeAutoLive(verse, options)
}

// When the active translation changes (e.g. a "read in NIV" voice command),
// the live output holds a snapshot in the old translation. Re-fetch the live
// verse in the new translation and re-commit it without toggling live on/off,
// mirroring how the preview panel refreshes itself on translation change.
export async function refreshLiveTranslation(): Promise<void> {
  const broadcast = getBroadcastLiveStore()
  const live = broadcast.liveItem
  if (!broadcast.isLive || live?.kind !== "scripture" || !live.scripture) {
    return
  }
  const { book_number, chapter, verse } = live.scripture
  if (book_number <= 0 || chapter <= 0 || verse <= 0) return

  const refreshed = await bibleActions.fetchVerse(book_number, chapter, verse)
  if (refreshed) {
    commitVerseToLive(refreshed, { makeLive: false })
  }
}

function loadEgwDeck(p: EgwParagraph, activeIndex = 0) {
  const deck = createEgwDeckItems(p)
  useEgwSlideStore.getState().setDeck(deck, activeIndex)
  return deck
}

export function createEgwQueueItem(
  p: EgwParagraph,
  options?: {
    confidence?: number
    source?: QueueItem["source"]
  }
): QueueItem {
  return {
    id: crypto.randomUUID(),
    presentation: createEgwPresentationItem(p),
    confidence: options?.confidence ?? 1,
    source: options?.source ?? "manual",
    added_at: Date.now(),
  }
}

export function previewEgwParagraph(p: EgwParagraph) {
  const deck = loadEgwDeck(p, 0)
  const first = deck[0]
  if (first) selectPreviewItem(first)
}

export function presentEgwParagraph(p: EgwParagraph) {
  const deck = loadEgwDeck(p, 0)
  const first = deck[0]
  if (first) presentItem(first)
}
