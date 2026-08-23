// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest"
import { useQueueStore } from "./queue-store"
import type { QueueItem } from "@/types"

function makeItem(id: string, verse: number): QueueItem {
  return {
    id,
    presentation: {
      kind: "scripture" as const,
      verse: {
        id: verse,
        translation_id: 1,
        book_number: 43,
        book_name: "John",
        book_abbreviation: "John",
        chapter: 3,
        verse,
        text: `Verse ${verse}`,
      },
      reference: `John 3:${verse}`,
    },
    confidence: 0.95,
    source: "manual",
    added_at: Date.now(),
  }
}

function makeChapterOnlyItem(id: string, chapter: number): QueueItem {
  return {
    id,
    presentation: {
      kind: "scripture" as const,
      verse: {
        id: 1,
        translation_id: 1,
        book_number: 43,
        book_name: "John",
        book_abbreviation: "John",
        chapter,
        verse: 1,
        text: "",
      },
      reference: `John ${chapter}`,
    },
    confidence: 0.9,
    source: "ai-direct",
    added_at: Date.now(),
    is_chapter_only: true,
  }
}

function seed(items: QueueItem[], activeIndex: number | null = null): void {
  useQueueStore.getState().addItems(items)
  if (activeIndex !== null) useQueueStore.getState().setActive(activeIndex)
}

describe("queue-store", () => {
  beforeEach(() => {
    vi.useFakeTimers()
    localStorage.clear()
    useQueueStore.persist?.clearStorage?.()
    useQueueStore.getState().clearQueue()
    useQueueStore.setState({
      items: [],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })
  })

  it("keeps the same active item after removing an earlier item", () => {
    useQueueStore.setState({
      items: [makeItem("a", 16), makeItem("b", 17), makeItem("c", 18)],
      activeIndex: 1,
      highlightedId: null,
      highlightedIds: [],
    })

    useQueueStore.getState().removeItem("a")

    expect(useQueueStore.getState().items.map((i) => i.id)).toEqual(["b", "c"])
    expect(useQueueStore.getState().activeIndex).toBe(0)
  })

  it("keeps the same active item after reorder", () => {
    useQueueStore.setState({
      items: [makeItem("a", 16), makeItem("b", 17), makeItem("c", 18)],
      activeIndex: 1,
      highlightedId: null,
      highlightedIds: [],
    })

    useQueueStore.getState().reorderItems(1, 0)

    expect(useQueueStore.getState().items.map((i) => i.id)).toEqual(["b", "a", "c"])
    expect(useQueueStore.getState().activeIndex).toBe(0)
  })

  it("appends multiple items to the end without reversing their order", () => {
    useQueueStore.setState({
      items: [makeItem("existing", 19)],
      activeIndex: 0,
      highlightedId: null,
      highlightedIds: [],
    })

    useQueueStore.getState().addItems([
      makeItem("a", 16),
      makeItem("b", 17),
      makeItem("c", 18),
    ])

    expect(useQueueStore.getState().items.map((i) => i.id)).toEqual([
      "existing",
      "a",
      "b",
      "c",
    ])
    // Appends never shift activeIndex — the existing active item stays put.
    expect(useQueueStore.getState().activeIndex).toBe(0)
  })

  it("keeps active item attached when another item is dragged around it", () => {
    useQueueStore.setState({
      items: [makeItem("a", 16), makeItem("b", 17), makeItem("c", 18)],
      activeIndex: 1,
      highlightedId: null,
      highlightedIds: [],
    })

    useQueueStore.getState().reorderItems(2, 0)

    expect(useQueueStore.getState().items.map((i) => i.id)).toEqual(["c", "a", "b"])
    expect(useQueueStore.getState().activeIndex).toBe(2)
  })

  it("ignores invalid reorder requests", () => {
    useQueueStore.setState({
      items: [makeItem("a", 16), makeItem("b", 17)],
      activeIndex: 0,
      highlightedId: null,
      highlightedIds: [],
    })

    useQueueStore.getState().reorderItems(-1, 1)
    useQueueStore.getState().reorderItems(0, 2)

    expect(useQueueStore.getState().items.map((i) => i.id)).toEqual(["a", "b"])
    expect(useQueueStore.getState().activeIndex).toBe(0)
  })

  it("flashes instead of adding duplicate detection item", () => {
    useQueueStore.setState({
      items: [makeItem("a", 16)],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })

    const result = useQueueStore.getState().addOrFlashDetectionItem(makeItem("dup", 16))

    expect(result).toBe("duplicate")
    expect(useQueueStore.getState().items).toHaveLength(1)
    expect(useQueueStore.getState().highlightedId).toBe("a")
    expect(useQueueStore.getState().highlightedIds).toEqual(["a"])
  })

  it("duplicate detection flash does not move the active selection", () => {
    useQueueStore.setState({
      items: [makeItem("a", 16), makeItem("b", 17), makeItem("c", 18)],
      activeIndex: 2,
      highlightedId: null,
      highlightedIds: [],
    })

    const result = useQueueStore.getState().addOrFlashDetectionItem(makeItem("dup", 16))

    expect(result).toBe("duplicate")
    // A re-cited duplicate must not silently jump the queue cursor.
    expect(useQueueStore.getState().activeIndex).toBe(2)
    expect(useQueueStore.getState().highlightedId).toBe("a")
  })

  it("updateEarlyRef fallback never mixes placeholder chapter with new verse text", () => {
    // "turn to John 10" queues a chapter-only placeholder; a later "John
    // 3:16" citation must NOT overwrite that placeholder with chapter 10 +
    // John 3:16's text (mislabeled "John 3:16" that navigates to John 10:17).
    const placeholder = makeChapterOnlyItem("p10", 10)
    useQueueStore.setState({
      items: [placeholder],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })

    const updated = useQueueStore.getState().updateEarlyRef(
      43,
      3,
      16,
      "John 3:16",
      "For God so loved the world...",
    )

    expect(updated).toBe(false)
    const state = useQueueStore.getState()
    expect(state.items).toHaveLength(1)
    expect(state.items[0].is_chapter_only).toBe(true)
    expect(state.items[0].presentation.kind === "scripture" &&
      state.items[0].presentation.verse.chapter).toBe(10)
  })

  it("updateEarlyRef refines a book-only placeholder (default chapter 1)", () => {
    useQueueStore.setState({
      items: [makeChapterOnlyItem("p1", 1)],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })

    const updated = useQueueStore.getState().updateEarlyRef(
      43,
      10,
      16,
      "John 10:16",
      "Refined verse text",
    )

    expect(updated).toBe(true)
    const item = useQueueStore.getState().items[0]
    expect(item.is_chapter_only).toBe(false)
    if (item.presentation.kind === "scripture") {
      expect(item.presentation.verse.chapter).toBe(10)
      expect(item.presentation.verse.verse).toBe(16)
      expect(item.presentation.verse.text).toBe("Refined verse text")
      expect(item.presentation.reference).toBe("John 10:16")
    }
  })

  it("updateEarlyRef reports failure for non-scripture matches without updating", () => {
    useQueueStore.setState({
      items: [makeChapterOnlyItem("p1", 1)],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })
    // Corrupt the presentation kind so the scripture guard trips.
    const hymnPresentation = {
      kind: "hymn" as const,
      hymnId: "h1",
      hymnNumber: 1,
      hymnTitle: "Test Hymn",
      screenId: "s1",
      slideIndex: 0,
      slideCount: 1,
      reference: "Hymn 1",
      segments: [],
    }
    useQueueStore.setState((state) => ({
      items: state.items.map((item) =>
        item.id === "p1" ? { ...item, presentation: hymnPresentation } : item,
      ),
    }))

    const updated = useQueueStore
      .getState()
      .updateEarlyRef(43, 10, 16, "John 10:16", "text")

    // Guard must report not-found, not success.
    expect(updated).toBe(false)
  })

  it("keeps multiple duplicate queue items highlighted independently", () => {
    useQueueStore.setState({
      items: [makeItem("a", 16), makeItem("b", 17)],
      activeIndex: null,
      highlightedId: null,
      highlightedIds: [],
    })

    useQueueStore.getState().flashItem("a")
    useQueueStore.getState().flashItem("b")

    expect(useQueueStore.getState().highlightedIds).toEqual(["a", "b"])

    vi.advanceTimersByTime(1500)

    expect(useQueueStore.getState().highlightedIds).toEqual([])
    expect(useQueueStore.getState().highlightedId).toBeNull()
  })

  describe("append ordering", () => {
    const itemA = makeItem("a", 16)
    const itemB = makeItem("b", 17)
    const itemC = makeItem("c", 18)

    it("appends new items at the end and never shifts activeIndex", () => {
      const store = useQueueStore.getState()
      store.addItem(itemA)
      store.addItem(itemB)
      useQueueStore.getState().setActive(0)
      store.addItem(itemC)
      const state = useQueueStore.getState()
      expect(state.items.map((i) => i.id)).toEqual([itemA.id, itemB.id, itemC.id])
      expect(state.activeIndex).toBe(0)
    })
  })

  describe("bulk operations", () => {
    const itemA = makeItem("a", 16)
    const itemB = makeItem("b", 17)
    const itemC = makeItem("c", 18)
    const itemD = makeItem("d", 19)

    it("removeItems drops all given ids and keeps the active item active", () => {
      seed([itemA, itemB, itemC, itemD], 2) // active = itemC
      useQueueStore.getState().removeItems([itemA.id, itemD.id])
      const state = useQueueStore.getState()
      expect(state.items.map((i) => i.id)).toEqual([itemB.id, itemC.id])
      expect(state.activeIndex).toBe(1)
    })

    it("moveItems moves a selection as a block preserving relative order", () => {
      seed([itemA, itemB, itemC, itemD])
      useQueueStore.getState().moveItems([itemA.id, itemC.id], 2)
      expect(useQueueStore.getState().items.map((i) => i.id)).toEqual([
        itemB.id,
        itemD.id,
        itemA.id,
        itemC.id,
      ])
    })
  })

  describe("persistence", () => {
    const itemA = makeItem("a", 16)

    it("persists items and activeIndex but not highlight state", () => {
      seed([itemA])
      useQueueStore.getState().flashItem(itemA.id)
      const raw = localStorage.getItem("sabbathcue-queue-v1")
      expect(raw).toBeTruthy()
      const persisted = JSON.parse(raw!)
      expect(persisted.state.items).toHaveLength(1)
      expect(persisted.state.highlightedIds).toBeUndefined()
    })

    it("queue actions still update state when storage quota is exceeded", () => {
      seed([itemA, makeItem("b", 17)])
      const setItemSpy = vi
        .spyOn(Storage.prototype, "setItem")
        .mockImplementation(() => {
          throw new DOMException("quota exceeded", "QuotaExceededError")
        })
      try {
        expect(() => useQueueStore.getState().setActive(1)).not.toThrow()
        expect(useQueueStore.getState().activeIndex).toBe(1)
      } finally {
        setItemSpy.mockRestore()
      }
    })

    it("drops malformed persisted items instead of crashing", async () => {
      localStorage.setItem(
        "sabbathcue-queue-v1",
        JSON.stringify({
          state: { items: [{ bogus: true }], activeIndex: 5 },
          version: 1,
        }),
      )
      await useQueueStore.persist.rehydrate()
      expect(useQueueStore.getState().items).toEqual([])
      expect(useQueueStore.getState().activeIndex).toBeNull()
    })
  })
})
