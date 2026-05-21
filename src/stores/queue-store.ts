import { create } from "zustand"
import {
  getVerseFromItem,
  type QueueItem,
} from "@/types"

interface QueueState {
  items: QueueItem[]
  activeIndex: number | null
  /** ID of the queue item currently being flash-highlighted (null = none). */
  highlightedId: string | null

  addItem: (item: QueueItem) => void
  addItems: (items: QueueItem[]) => void
  addOrFlashItem: (item: QueueItem) => "added" | "duplicate"
  addOrFlashDetectionItem: (item: QueueItem) => "added" | "duplicate"
  removeItem: (id: string) => void
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

let flashTimer: ReturnType<typeof setTimeout> | null = null

export const useQueueStore = create<QueueState>((set, get) => ({
  items: [],
  activeIndex: null,
  highlightedId: null,

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
      return { items: [item, ...state.items] }
    }),
  addItems: (items) =>
    set((state) => {
      if (items.length === 0) return state

      const existingIds = new Set(state.items.map((item) => item.id))
      const newItems = items.filter((item) => !existingIds.has(item.id))
      if (newItems.length === 0) return state

      return {
        items: [...newItems, ...state.items],
        activeIndex:
          state.activeIndex === null ? null : state.activeIndex + newItems.length,
      }
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
        if (!item.is_chapter_only) get().setActive(duplicateIndex)
      }
      return "duplicate"
    }

    get().addItem(item)
    return "added"
  },
  removeItem: (id) =>
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
  clearQueue: () => set({ items: [], activeIndex: null }),
  flashItem: (id) => {
    if (flashTimer) clearTimeout(flashTimer)
    set({ highlightedId: id })
    flashTimer = setTimeout(() => set({ highlightedId: null }), 1500)
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
      // Fallback: same book, any chapter (book-only detection guessed chapter 1)
      if (idx === -1) {
        idx = state.items.findIndex(
          (i) => {
            const iv = getVerseFromItem(i)
            return (
              i.is_chapter_only &&
              iv?.book_number === bookNumber
            )
          },
        )
      }
      if (idx === -1) return state
      found = true
      const items = [...state.items]
      const item = { ...items[idx] }
      const itemVerse = getVerseFromItem(item)
      if (!itemVerse || item.presentation.kind !== "scripture") return state
      const updatedVerse = { ...itemVerse, verse, text: verseText }
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
}))
