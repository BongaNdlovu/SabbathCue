import { describe, expect, it, beforeEach, afterEach, vi } from "vitest"
import {
  getRecentHymns,
  addRecentHymn,
  getFavoriteHymns,
  toggleFavoriteHymn,
  isFavoriteHymn,
} from "./hymnal-history"

const storage = new Map<string, string>()

const localStorageMock = {
  getItem: vi.fn((key: string) => storage.get(key) ?? null),
  setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
  clear: vi.fn(() => storage.clear()),
}

describe("hymnal history", () => {
  beforeEach(() => {
    storage.clear()
    vi.stubGlobal("localStorage", localStorageMock)
  })

  afterEach(() => {
    storage.clear()
    vi.unstubAllGlobals()
  })

  describe("recent hymns", () => {
    it("returns empty list when no recent hymns", () => {
      expect(getRecentHymns()).toEqual([])
    })

    it("adds hymn to recent list", () => {
      addRecentHymn("sda-1")
      expect(getRecentHymns()).toEqual(["sda-1"])
    })

    it("deduplicates hymns when adding", () => {
      addRecentHymn("sda-1")
      addRecentHymn("sda-2")
      addRecentHymn("sda-1")
      expect(getRecentHymns()).toEqual(["sda-1", "sda-2"])
    })

    it("caps recent hymns to limit", () => {
      for (let i = 1; i <= 15; i++) {
        addRecentHymn(`sda-${i}`)
      }
      expect(getRecentHymns().length).toBe(12)
    })

    it("ignores invalid hymn ids", () => {
      addRecentHymn("invalid-id")
      expect(getRecentHymns()).toEqual([])
    })

    it("handles malformed localStorage data", () => {
      localStorage.setItem("rhema_recent_hymns", "not-json")
      expect(getRecentHymns()).toEqual([])
    })

    it("handles non-array localStorage data", () => {
      localStorage.setItem("rhema_recent_hymns", JSON.stringify({}))
      expect(getRecentHymns()).toEqual([])
    })
  })

  describe("favorite hymns", () => {
    it("returns empty list when no favorites", () => {
      expect(getFavoriteHymns()).toEqual([])
    })

    it("toggles hymn as favorite", () => {
      expect(toggleFavoriteHymn("sda-1")).toBe(true)
      expect(getFavoriteHymns()).toEqual(["sda-1"])
    })

    it("removes hymn from favorites when toggling again", () => {
      toggleFavoriteHymn("sda-1")
      expect(toggleFavoriteHymn("sda-1")).toBe(false)
      expect(getFavoriteHymns()).toEqual([])
    })

    it("checks if hymn is favorite", () => {
      expect(isFavoriteHymn("sda-1")).toBe(false)
      toggleFavoriteHymn("sda-1")
      expect(isFavoriteHymn("sda-1")).toBe(true)
    })

    it("ignores invalid hymn ids when toggling", () => {
      expect(toggleFavoriteHymn("invalid-id")).toBe(false)
      expect(getFavoriteHymns()).toEqual([])
    })

    it("returns false for invalid hymn ids when checking", () => {
      expect(isFavoriteHymn("invalid-id")).toBe(false)
    })

    it("handles malformed localStorage data", () => {
      localStorage.setItem("rhema_favorite_hymns", "not-json")
      expect(getFavoriteHymns()).toEqual([])
    })
  })
})
