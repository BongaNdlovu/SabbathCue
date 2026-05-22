import { bibleActions } from "@/hooks/use-bible"
import { createScriptureQueueItem } from "@/lib/presentation-workflow"
import {
  createHymnQueueItem,
  defaultSelectedSectionIds,
} from "@/services/hymnal/hymn-presentation"
import { generateHymnScreens } from "@/services/hymnal/generate-hymn-screens"
import { getHymnByNumber } from "@/services/hymnal/hymnal-repository"
import { useBibleStore } from "@/stores/bible-store"
import { useQueueStore } from "@/stores/queue-store"
import type { QueueItem } from "@/types/queue"
import type { PresentationItem } from "@/types/presentation"
import type { MediaRef, ServiceAttachment, ServiceItem, ScriptureRef } from "@/types/service-plan"
import type { Verse } from "@/types"

const PLAN_PREFIX = "[Plan]"

function planReference(label: string): string {
  return `${PLAN_PREFIX} ${label}`
}

function createPlaceholderScripturePresentation(
  label: string,
  detail: string,
): PresentationItem {
  const translationId = useBibleStore.getState().activeTranslationId
  const verse: Verse = {
    id: 0,
    translation_id: translationId,
    book_number: 0,
    book_name: "Service Plan",
    book_abbreviation: "Plan",
    chapter: 0,
    verse: 0,
    text: detail,
  }
  return {
    kind: "scripture",
    verse,
    reference: planReference(label),
  }
}

function createMediaPresentation(
  input: Pick<ServiceAttachment, "id" | "kind" | "label"> | MediaRef,
  kind: "media" | "slide" | "document" | "deck",
): PresentationItem {
  const title = input.label
  return {
    kind: "media",
    mediaId: "id" in input ? input.id : input.attachmentId,
    mediaKind: kind,
    title,
    reference: planReference(`${kind === "deck" ? "Deck" : kind === "slide" ? "Slide" : "Media"} - ${title}`),
    segments: [
      {
        text:
          kind === "deck"
            ? "Prepared slide deck. Open from the Service Plan to preview the selected deck."
            : kind === "slide"
              ? "Prepared slide attachment. Open from the Service Plan to preview the selected file."
              : "Prepared media attachment. Open from the Service Plan to preview the selected file.",
      },
    ],
  }
}

async function resolveScriptureRef(ref: ScriptureRef): Promise<Verse | null> {
  if (!ref.chapter) return null

  let books = useBibleStore.getState().books
  if (books.length === 0) {
    books = await bibleActions.loadBooks()
  }

  const bookQuery = (ref.book ?? ref.reference ?? "").trim().toLowerCase()
  if (!bookQuery) return null

  const book = books.find(
    (candidate) =>
      candidate.name.toLowerCase() === bookQuery ||
      candidate.abbreviation.toLowerCase() === bookQuery,
  )
  if (!book) return null

  const verseNumber = ref.verse ?? 1
  return bibleActions.fetchVerse(book.book_number, ref.chapter, verseNumber)
}

function queuePreparedItem(presentation: PresentationItem): void {
  const item: QueueItem = {
    id: crypto.randomUUID(),
    presentation,
    confidence: 1,
    source: "service-plan",
    added_at: Date.now(),
  }
  useQueueStore.getState().addItem(item)
}

export async function enqueuePreparedResourcesForItem(item: ServiceItem): Promise<number> {
  let queued = 0

  for (const hymnRef of item.hymnRefs) {
    if (!hymnRef.hymnNumber) continue
    try {
      const hymn = await getHymnByNumber(hymnRef.hymnNumber)
      if (!hymn) continue
      const screens = generateHymnScreens({
        hymn,
        selectedSectionIds: defaultSelectedSectionIds(hymn),
        maxLinesPerScreen: 4,
      })
      const first = screens[0]
      if (!first) continue
      const queueItem = createHymnQueueItem(first)
      queuePreparedItem({
        ...queueItem.presentation,
        reference: planReference(queueItem.presentation.reference),
      })
      queued += 1
    } catch {
      // Failed hymn loads must not block other resources.
    }
  }

  for (const scriptureRef of item.scriptureRefs) {
    try {
      const verse = await resolveScriptureRef(scriptureRef)
      if (verse) {
        const queueItem = createScriptureQueueItem(verse, {
          reference: planReference(
            scriptureRef.reference ??
              `${verse.book_name} ${verse.chapter}:${verse.verse}`,
          ),
          source: "service-plan",
        })
        useQueueStore.getState().addItem(queueItem)
        queued += 1
        continue
      }

      const label =
        scriptureRef.reference ??
        [scriptureRef.book, scriptureRef.chapter, scriptureRef.verse].filter(Boolean).join(" ")
      if (!label) continue
      queuePreparedItem(
        createPlaceholderScripturePresentation(label, "Scripture reference pending lookup."),
      )
      queued += 1
    } catch {
      // Keep going when one scripture ref fails.
    }
  }

  for (const media of item.mediaRefs) {
    queuePreparedItem(createMediaPresentation(media, "media"))
    queued += 1
  }

  for (const attachment of item.attachments) {
    if (attachment.kind !== "media" && attachment.kind !== "slide" && attachment.kind !== "deck") continue
    queuePreparedItem(createMediaPresentation(attachment, attachment.kind))
    queued += 1
  }

  return queued
}
