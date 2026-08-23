import { create } from "zustand"
import { createJSONStorage, persist } from "zustand/middleware"
import {
  getReferenceFromItem,
  getVerseFromItem,
  type QueueItem,
} from "@/types"
import { notifyAction } from "@/lib/action-notifications"

interface QueueState {
  items: QueueItem[]
  activeIndex: number | null
  /** ID of the queue item currently being flash-highlighted (null = none). */
  highlightedId: string | null
  highlightedIds: string[]

  addItem: (item: QueueItem) => void
  addItems: (items: QueueItem[]) => void
  addOrFlashItem: (item: QueueItem) => "added" | "duplicate"
  addOrFlashDetectionItem: (item: QueueItem) => "added" | "duplicate"
  removeItem: (id: string) => void
  /**
   * Bulk-remove every item whose id is in `ids`. The currently active item
   * stays active (its index is recomputed); if it was removed, the active
   * position is clamped into the remaining list (or cleared when empty).
   */
  removeItems: (ids: string[]) => void
  /**
   * Move the items identified by `ids` as a contiguous block, preserving their
   * relative order, so they are inserted at `toIndex` — the insertion position
   * within the REMAINING items after the selection is pulled out. The active
   * item stays active (index recomputed).
   */
  moveItems: (ids: string[], toIndex: number) => void
  reorderItems: (fromIndex: number, toIndex: number) => void
  setActive: (index: number | null) => void
  clearQueue: () => void
  /** Flash-highlight a queue item briefly (1.5 s). */
  flashItem: (id: string) => void
  /** Find an existing item by book+chapter+verse. Returns its index or -1. */
  findDuplicate: (bookNumber: number, chapter: number, verse: number) => number
  /** Update a chapter-only queue item in place when the verse is refined. */
  updateEarlyRef: (bookNumber: number, chapter: number, verse: number, reference: string, verseText: string) => boolean
}

// Queue items can carry multi-megabyte slide-image data URLs, so a persist
// write can exceed the localStorage quota. That must degrade to "queue not
// persisted" instead of throwing out of every queue action that writes state
// (e.g. the preview/present buttons calling setActive).
const quotaSafeQueueStorage = {
  getItem: (name: string) => window.localStorage.getItem(name),
  setItem: (name: string, value: string) => {
    try {
      window.localStorage.setItem(name, value)
    } catch {
      console.warn(
        "[queue] Skipped persisting queue state: storage quota exceeded"
      )
    }
  },
  removeItem: (name: string) => window.localStorage.removeItem(name),
}

const flashTimers = new Map<string, ReturnType<typeof setTimeout>>()

function clearFlashTimer(id: string): void {
  const existing = flashTimers.get(id)
  if (existing) clearTimeout(existing)
  flashTimers.delete(id)
}

function clearAllFlashTimers(): void {
  flashTimers.forEach(clearTimeout)
  flashTimers.clear()
}

export const useQueueStore = create<QueueState>()(
  persist(
    (set, get) => ({
  items: [],
  activeIndex: null,
  highlightedId: null,
  highlightedIds: [],

  addItem: (item) =>
    set((state) => {
      const itemVerse = getVerseFromItem(item)
      const duplicate = itemVerse
        ? state.items.some((i) => {
            const iv = getVerseFromItem(i)
            return (
              iv?.book_number === itemVerse.book_number &&
              iv.chapter === itemVerse.chapter &&
              iv.verse === itemVerse.verse
            )
          })
        : state.items.some((i) => i.id === item.id)
      if (duplicate) return state
      return { items: [...state.items, item] }
    }),
  addItems: (items) =>
    set((state) => {
      if (items.length === 0) return state

      const existingIds = new Set(state.items.map((item) => item.id))
      const newItems = items.filter((item) => !existingIds.has(item.id))
      if (newItems.length === 0) return state

      // Appends go to the end and NEVER shift activeIndex — an existing item's
      // position is stable when new items are added (PowerPoint model).
      return { items: [...state.items, ...newItems] }
    }),
  addOrFlashItem: (item) => {
    const itemVerse = getVerseFromItem(item)
    const duplicateIndex = itemVerse
      ? get().findDuplicate(
          itemVerse.book_number,
          itemVerse.chapter,
          itemVerse.verse,
        )
      : get().items.findIndex((i) => i.id === item.id)

    if (duplicateIndex !== -1) {
      const existing = get().items[duplicateIndex]
      if (existing) get().flashItem(existing.id)
      return "duplicate"
    }

    get().addItem(item)
    notifyAction("Added to queue", getReferenceFromItem(item))
    return "added"
  },
  addOrFlashDetectionItem: (item) => {
    const itemVerse = getVerseFromItem(item)
    if (!itemVerse) return get().addOrFlashItem(item)
    const duplicateIndex = item.is_chapter_only
      ? get().items.findIndex(
          (i) => {
            const iv = getVerseFromItem(i)
            return (
              iv?.book_number === itemVerse.book_number &&
              iv.chapter === itemVerse.chapter
            )
          },
        )
      : get().findDuplicate(
          itemVerse.book_number,
          itemVerse.chapter,
          itemVerse.verse,
        )

    if (duplicateIndex !== -1) {
      const existing = get().items[duplicateIndex]
      if (existing) {
        get().flashItem(existing.id)
        // A re-detected duplicate must NOT move the operator's active
        // selection: the speaker re-citing an earlier verse silently jumped
        // the queue cursor, so the next "next" presented the wrong slide.
        // (The store's own model: appends never shift activeIndex.)
      }
      return "duplicate"
    }

    get().addItem(item)
    return "added"
  },
  removeItem: (id) => {
    const existed = get().items.some((i) => i.id === id)
    set((state) => {
      const removedIndex = state.items.findIndex((i) => i.id === id)
      const items = state.items.filter((i) => i.id !== id)

      let activeIndex = state.activeIndex
      if (removedIndex !== -1 && activeIndex !== null) {
        if (items.length === 0) {
          activeIndex = null
        } else if (removedIndex === activeIndex) {
          activeIndex = Math.min(activeIndex, items.length - 1)
        } else if (removedIndex < activeIndex) {
          activeIndex -= 1
        }
      }

      return { items, activeIndex }
    })
    if (existed) notifyAction("Removed from queue")
  },
  removeItems: (ids) =>
    set((state) => {
      const idSet = new Set(ids)
      const activeItem =
        state.activeIndex === null ? null : state.items[state.activeIndex] ?? null
      const items = state.items.filter((i) => !idSet.has(i.id))
      let activeIndex: number | null = null
      if (activeItem) {
        const keptIndex = items.findIndex((i) => i.id === activeItem.id)
        activeIndex =
          keptIndex !== -1
            ? keptIndex
            : items.length > 0
              ? Math.min(state.activeIndex ?? 0, items.length - 1)
              : null
      }
      return { items, activeIndex }
    }),
  moveItems: (ids, toIndex) =>
    set((state) => {
      const idSet = new Set(ids)
      const moving = state.items.filter((i) => idSet.has(i.id))
      if (moving.length === 0) return state
      const activeItem =
        state.activeIndex === null ? null : state.items[state.activeIndex] ?? null
      const remaining = state.items.filter((i) => !idSet.has(i.id))
      const insertAt = Math.max(0, Math.min(toIndex, remaining.length))
      const items = [
        ...remaining.slice(0, insertAt),
        ...moving,
        ...remaining.slice(insertAt),
      ]
      const activeIndex = activeItem
        ? items.findIndex((i) => i.id === activeItem.id)
        : null
      return { items, activeIndex: activeIndex === -1 ? null : activeIndex }
    }),
  reorderItems: (fromIndex, toIndex) =>
    set((state) => {
      if (
        fromIndex < 0 ||
        toIndex < 0 ||
        fromIndex >= state.items.length ||
        toIndex >= state.items.length ||
        fromIndex === toIndex
      ) {
        return state
      }

      const activeItem =
        state.activeIndex === null ? null : state.items[state.activeIndex] ?? null

      const items = [...state.items]
      const [moved] = items.splice(fromIndex, 1)
      items.splice(toIndex, 0, moved)

      const activeIndex = activeItem
        ? items.findIndex((item) => item.id === activeItem.id)
        : null

      return {
        items,
        activeIndex: activeIndex === -1 ? null : activeIndex,
      }
    }),
  setActive: (activeIndex) => set({ activeIndex }),
  clearQueue: () => {
    const hadItems = get().items.length > 0
    clearAllFlashTimers()
    set({ items: [], activeIndex: null, highlightedId: null, highlightedIds: [] })
    if (hadItems) notifyAction("Queue cleared")
  },
  flashItem: (id) => {
    clearFlashTimer(id)
    set((state) => ({
      highlightedId: id,
      highlightedIds: state.highlightedIds.includes(id)
        ? state.highlightedIds
        : [...state.highlightedIds, id],
    }))
    const timer = setTimeout(() => {
      flashTimers.delete(id)
      set((state) => {
        const highlightedIds = state.highlightedIds.filter((itemId) => itemId !== id)
        return {
          highlightedIds,
          highlightedId:
            state.highlightedId === id
              ? highlightedIds[highlightedIds.length - 1] ?? null
              : state.highlightedId,
        }
      })
    }, 1500)
    flashTimers.set(id, timer)
  },
  findDuplicate: (bookNumber, chapter, verse) =>
    get().items.findIndex(
      (i) => {
        const iv = getVerseFromItem(i)
        return (
          iv?.book_number === bookNumber &&
          iv.chapter === chapter &&
          iv.verse === verse
        )
      },
    ),
  updateEarlyRef: (bookNumber, chapter, verse, reference, verseText) => {
    let found = false
    set((state) => {
      // First try exact match: same book + same chapter
      let idx = state.items.findIndex(
        (i) => {
          const iv = getVerseFromItem(i)
          return (
            i.is_chapter_only &&
            iv?.book_number === bookNumber &&
            iv.chapter === chapter
          )
        },
      )
      // Fallback: same book, chapter-only placeholder whose chapter is the
      // default 1 (book-only detection guessed it). Matching a placeholder
      // with a real spoken chapter ("John 10" + later "John 3:16") produced
      // a verse whose chapter and text disagreed.
      if (idx === -1) {
        idx = state.items.findIndex(
          (i) => {
            const iv = getVerseFromItem(i)
            return (
              i.is_chapter_only &&
              iv?.book_number === bookNumber &&
              iv.chapter === 1
            )
          },
        )
      }
      if (idx === -1) return state
      const items = [...state.items]
      const item = { ...items[idx] }
      const itemVerse = getVerseFromItem(item)
      if (!itemVerse || item.presentation.kind !== "scripture") return state
      found = true
      // Adopt the newly spoken chapter along with the verse and text so the
      // updated item is internally consistent.
      const updatedVerse = {
        ...itemVerse,
        chapter,
        verse,
        text: verseText,
      }
      item.presentation = {
        ...item.presentation,
        verse: updatedVerse,
        reference,
      }
      item.is_chapter_only = false
      items[idx] = item
      return { items }
    })
    return found
  },
    }),
    {
      name: "sabbathcue-queue-v1",
      version: 1,
      storage: createJSONStorage(() => quotaSafeQueueStorage),
      partialize: (state) => ({
        items: state.items,
        activeIndex: state.activeIndex,
      }),
      merge: (persisted, current) => {
        const p = persisted as
          | { items?: unknown; activeIndex?: unknown }
          | undefined
        const items = Array.isArray(p?.items)
          ? (p.items as QueueItem[]).filter(
              (i) =>
                i != null &&
                typeof i.id === "string" &&
                typeof i.presentation?.kind === "string",
            )
          : []
        const activeIndex =
          typeof p?.activeIndex === "number" &&
          p.activeIndex >= 0 &&
          p.activeIndex < items.length
            ? p.activeIndex
            : null
        return { ...current, items, activeIndex }
      },
    },
  ),
)
