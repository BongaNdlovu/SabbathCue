import { create } from "zustand"
import { useSettingsStore } from "@/stores/settings-store"
import type { DetectionResult } from "@/types"

interface DetectionWithMeta {
  detection: DetectionResult
  received_at: number
}

interface DetectionResultWithMeta extends DetectionResult {
  received_at?: number
}

export interface DetectionContextEntry {
  key: string
  reference: string
  detail: string
  confidence: number
  source: DetectionResult["source"]
}

export interface HeldReferenceCandidate {
  detection: DetectionResultWithMeta
  reason: string
}

const MAX_RECENT_DETECTIONS = 5
// Keep matches actionable through a short live-speaking window, then clear old context.
const DETECTION_TTL_MS = 11_000
const NUMBER_TOKEN_PATTERN = /\d+/g
const VERSE_REF_PATTERN = /(\d+)\s*:\s*(\d+)/g

interface DetectionState {
  detections: DetectionResultWithMeta[]
  /** Display-only: "book:chapter:verse" key of the detection the AI ranker
   *  picked. Never consulted by preview/auto-live selection. */
  aiSuggestedKey: string | null

  addDetection: (detection: DetectionResult) => void
  addDetections: (detections: DetectionResult[]) => void
  setDetections: (detections: DetectionResult[]) => void
  removeDetection: (verseRef: string) => void
  clearDetections: () => void
  markAiSuggested: (key: string | null) => void
  evictStale: (now?: number) => void
}

function sourcePriority(detection: DetectionResultWithMeta): number {
  return detection.source === "direct" ? 1 : 0
}

// EGW quotes are scored on their own scale (shared-phrase length, not verse
// cosine), so they carry their own floor. An operator lowering the Bible slider
// for verse recall must not simultaneously open the EGW gate.
const EGW_SEMANTIC_MIN_CONFIDENCE = 0.75

function isHiddenBySemanticSettings(detection: DetectionResult): boolean {
  if (detection.source !== "semantic") return false

  const { semanticDetectionEnabled, semanticConfidenceThreshold } =
    useSettingsStore.getState()
  if (!semanticDetectionEnabled) return true

  const floor =
    detection.content_type === "egw"
      ? Math.max(semanticConfidenceThreshold, EGW_SEMANTIC_MIN_CONFIDENCE)
      : semanticConfidenceThreshold

  return detection.confidence < floor
}

function isBibleDetection(detection: DetectionResult): boolean {
  return (
    detection.content_type !== "egw" &&
    detection.content_type !== "hymn" &&
    detection.book_number > 0 &&
    detection.chapter > 0
  )
}

function compareDetectionRecency(
  a: { detection: DetectionResultWithMeta; index: number },
  b: { detection: DetectionResultWithMeta; index: number }
): number {
  const aTime = a.detection.received_at
  const bTime = b.detection.received_at

  if (
    typeof aTime === "number" &&
    typeof bTime === "number" &&
    aTime !== bTime
  ) {
    return bTime - aTime
  }
  if (typeof aTime === "number" && typeof bTime !== "number") return -1
  if (typeof bTime === "number" && typeof aTime !== "number") return 1
  return a.index - b.index
}

export function buildDetectionContextStack(
  detections: DetectionResultWithMeta[]
): DetectionContextEntry[] {
  const seen = new Set<string>()
  const entries: DetectionContextEntry[] = []

  for (const { detection } of detections
    .map((detection, index) => ({ detection, index }))
    .sort(compareDetectionRecency)) {
    if (!isBibleDetection(detection)) continue

    const key = `${detection.book_number}:${detection.chapter}`
    if (seen.has(key)) continue
    seen.add(key)
    entries.push({
      key,
      reference: `${detection.book_name} ${detection.chapter}`,
      detail: detection.is_chapter_only
        ? "Chapter context"
        : `Last hit ${detection.verse_ref}`,
      confidence: detection.confidence,
      source: detection.source,
    })
    if (entries.length === 4) break
  }

  return entries
}

export function buildHeldReferenceCandidates(
  detections: DetectionResultWithMeta[],
  confidenceThreshold: number,
  semanticConfidenceThreshold: number
): HeldReferenceCandidate[] {
  return detections.flatMap((detection) => {
    if (!isBibleDetection(detection)) return []
    if (detection.is_chapter_only)
      return [{ detection, reason: "Waiting for verse" }]

    const threshold =
      detection.source === "semantic"
        ? Math.max(confidenceThreshold, semanticConfidenceThreshold)
        : confidenceThreshold

    return detection.confidence < threshold
      ? [{ detection, reason: "Below auto-live threshold" }]
      : []
  })
}

// "Recent detections" retains one freshly spoken EGW paragraph so a quote is
// never crowded out of the box by stale Bible hits. Survivors are then ordered
// for display by confidence.
function isEgwDetection(detection: DetectionResult): boolean {
  return detection.content_type === "egw"
}

function capForDisplay(
  list: DetectionResultWithMeta[]
): DetectionResultWithMeta[] {
  const ranked = [...list].sort(compareDetections)
  // Keep one semantic EGW quote available for review, while still allowing
  // it to take the first row when its confidence is the strongest match.
  const egw = ranked.filter(isEgwDetection).slice(0, 1)
  const others = ranked
    .filter((detection) => !isEgwDetection(detection))
    .slice(0, MAX_RECENT_DETECTIONS - egw.length)
  return [...others, ...egw]
    .sort(compareDetections)
    .slice(0, MAX_RECENT_DETECTIONS)
}

function compareDetections(
  a: DetectionResultWithMeta,
  b: DetectionResultWithMeta
): number {
  const confidenceDiff = b.confidence - a.confidence
  if (Math.abs(confidenceDiff) > Number.EPSILON) return confidenceDiff

  const rankDiff =
    (b.rank_score ?? b.confidence) - (a.rank_score ?? a.confidence)
  if (Math.abs(rankDiff) > Number.EPSILON) return rankDiff

  const sourceDiff = sourcePriority(b) - sourcePriority(a)
  if (sourceDiff !== 0) return sourceDiff

  const aTime = a.received_at ?? 0
  const bTime = b.received_at ?? 0
  if (bTime !== aTime) return bTime - aTime

  return b.confidence - a.confidence
}

function mergeDetection(
  existing: DetectionResult,
  incoming: DetectionResult
): DetectionResult {
  const preferred =
    incoming.source === "direct" || existing.source !== "direct"
      ? incoming
      : existing
  const fallback = preferred === incoming ? existing : incoming

  return {
    ...preferred,
    confidence: Math.max(existing.confidence, incoming.confidence),
    source:
      existing.source === "direct" || incoming.source === "direct"
        ? "direct"
        : "semantic",
    verse_text:
      incoming.verse_text.length > 0
        ? incoming.verse_text
        : existing.verse_text,
    transcript_snippet:
      incoming.transcript_snippet.length > 0
        ? incoming.transcript_snippet
        : existing.transcript_snippet,
    auto_queued: existing.auto_queued || incoming.auto_queued,
    is_chapter_only: existing.is_chapter_only && incoming.is_chapter_only,
    book_name: preferred.book_name || fallback.book_name,
    // 0 is the "unresolved" sentinel — only use the preferred value when it is non-zero.
    book_number:
      preferred.book_number !== 0
        ? preferred.book_number
        : fallback.book_number,
    chapter: preferred.chapter !== 0 ? preferred.chapter : fallback.chapter,
    verse: preferred.verse !== 0 ? preferred.verse : fallback.verse,
    content_type: preferred.content_type ?? fallback.content_type,
    egw_paragraph: preferred.egw_paragraph ?? fallback.egw_paragraph,
  }
}

function normalizeVerseRef(verseRef: string): string {
  return verseRef
    .toLowerCase()
    .replace(/\s+/g, " ")
    .replace(/\s*:\s*/g, ":")
    .trim()
}

function numberTokenMatches(value: string, target: number): boolean {
  NUMBER_TOKEN_PATTERN.lastIndex = 0
  return [...value.matchAll(NUMBER_TOKEN_PATTERN)].some(
    ([token]) => Number(token) === target
  )
}

function verseRefMatches(
  value: string,
  chapter: number,
  verse: number
): boolean {
  VERSE_REF_PATTERN.lastIndex = 0
  return [...value.matchAll(VERSE_REF_PATTERN)].some(
    ([, refChapter, refVerse]) =>
      Number(refChapter) === chapter && Number(refVerse) === verse
  )
}

function detectionKey(detection: DetectionResult): string {
  if (detection.content_type === "egw" && detection.egw_paragraph) {
    const paragraph = detection.egw_paragraph
    return `egw:${paragraph.book_number}:${paragraph.page}:${paragraph.page_paragraph}`
  }

  const normalizedRef = normalizeVerseRef(detection.verse_ref)

  if (
    detection.book_number > 0 &&
    detection.chapter > 0 &&
    (detection.is_chapter_only
      ? numberTokenMatches(normalizedRef, detection.chapter)
      : verseRefMatches(normalizedRef, detection.chapter, detection.verse))
  ) {
    if (detection.is_chapter_only) {
      return `chapter:${detection.book_number}:${detection.chapter}`
    }
    if (detection.verse > 0) {
      return `verse:${detection.book_number}:${detection.chapter}:${detection.verse}`
    }
  }

  return `ref:${normalizedRef}`
}

function detectionMatchesRemovalKey(
  detection: DetectionResult,
  key: string
): boolean {
  return (
    detectionKey(detection) === key ||
    detection.verse_ref === key ||
    normalizeVerseRef(detection.verse_ref) === normalizeVerseRef(key)
  )
}

function detectionsAreEquivalent(
  a: DetectionResult,
  b: DetectionResult
): boolean {
  return (
    detectionKey(a) === detectionKey(b) ||
    normalizeVerseRef(a.verse_ref) === normalizeVerseRef(b.verse_ref)
  )
}

function findMapEntryKey(
  map: Map<string, DetectionWithMeta>,
  detection: DetectionResult
): string | undefined {
  for (const [key, item] of map) {
    if (detectionsAreEquivalent(item.detection, detection)) {
      return key
    }
  }
  return undefined
}

function withReceivedAt(
  detection: DetectionResult,
  fallback = 0
): DetectionResultWithMeta {
  return {
    ...detection,
    received_at:
      "received_at" in detection && typeof detection.received_at === "number"
        ? detection.received_at
        : fallback,
  }
}

function removeSupersededChapterOnlyPlaceholders(
  detections: DetectionResultWithMeta[]
): DetectionResultWithMeta[] {
  const explicitChapters = new Set<string>()
  for (const detection of detections) {
    if (
      !detection.is_chapter_only &&
      detection.book_number > 0 &&
      detection.chapter > 0
    ) {
      explicitChapters.add(`${detection.book_number}:${detection.chapter}`)
    }
  }
  if (explicitChapters.size === 0) return detections

  return detections.filter(
    (detection) =>
      !detection.is_chapter_only ||
      !explicitChapters.has(`${detection.book_number}:${detection.chapter}`)
  )
}

export const useDetectionStore = create<DetectionState>((set) => ({
  detections: [],
  aiSuggestedKey: null,

  addDetection: (detection) =>
    set((state) => {
      if (isHiddenBySemanticSettings(detection)) return state
      const now = Date.now()
      const existingIndex = state.detections.findIndex((d) =>
        detectionsAreEquivalent(d, detection)
      )

      if (existingIndex >= 0) {
        const existing = withReceivedAt(state.detections[existingIndex])
        const updated: DetectionResultWithMeta = {
          ...mergeDetection(existing, detection),
          received_at: now,
        }
        const newDetections = [...state.detections]
        newDetections[existingIndex] = updated
        return {
          detections: capForDisplay(
            removeSupersededChapterOnlyPlaceholders(newDetections)
          ),
        }
      }

      // New detection
      const withMeta: DetectionResultWithMeta = {
        ...detection,
        received_at: now,
      }
      const newDetections = [withMeta, ...state.detections]
      return {
        detections: capForDisplay(
          removeSupersededChapterOnlyPlaceholders(newDetections)
        ),
      }
    }),
  addDetections: (incoming) =>
    set((state) => {
      const now = Date.now()
      const map = new Map<string, DetectionWithMeta>()

      // Add incoming with received_at
      for (const d of incoming) {
        if (isHiddenBySemanticSettings(d)) continue
        const key = findMapEntryKey(map, d) ?? detectionKey(d)
        const existing = map.get(key)
        if (!existing) {
          map.set(key, { detection: d, received_at: now })
        } else {
          map.set(key, {
            detection: mergeDetection(existing.detection, d),
            received_at: now,
          })
        }
      }

      // Merge existing detections
      for (const d of state.detections) {
        if (isHiddenBySemanticSettings(d)) continue
        const key = findMapEntryKey(map, d) ?? detectionKey(d)
        const existing = map.get(key)
        const dWithMeta = withReceivedAt(d)
        const dReceivedAt = dWithMeta.received_at ?? 0
        if (!existing) {
          map.set(key, { detection: dWithMeta, received_at: dReceivedAt })
        } else {
          map.set(key, {
            detection: mergeDetection(d, existing.detection),
            received_at: Math.max(existing.received_at, dReceivedAt),
          })
        }
      }

      const withMeta = [...map.values()].map((item) => ({
        ...item.detection,
        received_at: item.received_at,
      }))

      return {
        detections: capForDisplay(
          removeSupersededChapterOnlyPlaceholders(withMeta)
        ),
      }
    }),
  setDetections: (detections) =>
    set(() => {
      const now = Date.now()
      const withMeta = detections.map((detection) =>
        withReceivedAt(detection, now)
      )
      return {
        detections: capForDisplay(
          removeSupersededChapterOnlyPlaceholders(withMeta)
        ),
      }
    }),
  removeDetection: (key) =>
    set((state) => {
      return {
        detections: state.detections.filter(
          (d) => !detectionMatchesRemovalKey(d, key)
        ),
      }
    }),
  clearDetections: () => set({ detections: [], aiSuggestedKey: null }),
  markAiSuggested: (aiSuggestedKey) => set({ aiSuggestedKey }),
  evictStale: (now = Date.now()) =>
    set((state) => {
      const fresh = state.detections.filter(
        (d) => now - (d.received_at ?? 0) < DETECTION_TTL_MS
      )

      return fresh.length === state.detections.length
        ? state
        : { detections: fresh }
    }),
}))
