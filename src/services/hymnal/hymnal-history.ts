import { SDA_HYMNAL_INDEX } from "@/data/sda-hymnal-index"

const RECENT_HYMNS_KEY = "rhema_recent_hymns"
const FAVORITE_HYMNS_KEY = "rhema_favorite_hymns"
const MAX_RECENT_HYMNS = 12

function normalizeHymnId(id: string): string | null {
  const normalized = SDA_HYMNAL_INDEX.find((hymn) => hymn.id === id)
  return normalized ? normalized.id : null
}

function getStoredIds(key: string): string[] {
  try {
    const stored = localStorage.getItem(key)
    if (!stored) return []
    const parsed = JSON.parse(stored)
    if (!Array.isArray(parsed)) return []
    return parsed
      .map((item) => {
        if (typeof item === "string") {
          return normalizeHymnId(item)
        }
        if (item && typeof item === "object" && "id" in item) {
          return normalizeHymnId(item.id)
        }
        return null
      })
      .filter((id): id is string => id !== null)
  } catch {
    return []
  }
}

function setStoredIds(key: string, ids: string[]): void {
  try {
    localStorage.setItem(key, JSON.stringify(ids))
  } catch {
    // Silently fail if localStorage is unavailable
  }
}

export function getRecentHymns(): string[] {
  return getStoredIds(RECENT_HYMNS_KEY)
}

export function addRecentHymn(hymnId: string): void {
  const normalizedId = normalizeHymnId(hymnId)
  if (!normalizedId) return

  const current = getRecentHymns()
  const filtered = current.filter((id) => id !== normalizedId)
  const updated = [normalizedId, ...filtered].slice(0, MAX_RECENT_HYMNS)
  setStoredIds(RECENT_HYMNS_KEY, updated)
}

export function getFavoriteHymns(): string[] {
  return getStoredIds(FAVORITE_HYMNS_KEY)
}

export function toggleFavoriteHymn(hymnId: string): boolean {
  const normalizedId = normalizeHymnId(hymnId)
  if (!normalizedId) return false

  const current = getFavoriteHymns()
  const isFavorite = current.includes(normalizedId)

  if (isFavorite) {
    const updated = current.filter((id) => id !== normalizedId)
    setStoredIds(FAVORITE_HYMNS_KEY, updated)
    return false
  } else {
    const updated = [...current, normalizedId]
    setStoredIds(FAVORITE_HYMNS_KEY, updated)
    return true
  }
}

export function isFavoriteHymn(hymnId: string): boolean {
  const normalizedId = normalizeHymnId(hymnId)
  if (!normalizedId) return false
  return getFavoriteHymns().includes(normalizedId)
}
