import { invokeTauri } from "@/lib/tauri-runtime"
import type { DetectionResult } from "@/types"
import type { RankingCandidate } from "@/types/ai-ranking"

const MAX_CANDIDATES = 8
const MAX_TRANSCRIPT_CHARS = 500
const SUMMARY_TEXT_CHARS = 240
const CIRCUIT_BREAKER_FAILURES = 3
const DIRECT_SUPPRESSION_MS = 4000
const DECISIVE_SEMANTIC_CONFIDENCE = 0.9
const MIN_AMBIGUITY_MARGIN = 0.15
const STABILITY_WINDOW_MS = 400
const RANKING_CACHE_MAX = 16

export interface RankingGate {
  deepseekRankingEnabled: boolean
  hasDeepseekApiKey: boolean
  confidenceThreshold: number
}

type ScheduledRanking = {
  generation: number
  detections: DetectionResult[]
  gate: RankingGate
  resolve: (value: DetectionResult | null) => void
}

let inFlight = false
let consecutiveFailures = 0
let lastStrongDirectAt = 0
const rankingCache = new Map<string, string | null>()
let debounceTimer: ReturnType<typeof setTimeout> | null = null
let pendingDebounced: ScheduledRanking | null = null
let pendingAfterFlight: ScheduledRanking | null = null
let activeScheduled: ScheduledRanking | null = null
let scheduleGeneration = 0

function resolveScheduled(
  entry: ScheduledRanking,
  value: DetectionResult | null
) {
  entry.resolve(value)
}

function supersedeScheduledWork(): void {
  if (pendingDebounced) {
    resolveScheduled(pendingDebounced, null)
    pendingDebounced = null
  }
  if (pendingAfterFlight) {
    resolveScheduled(pendingAfterFlight, null)
    pendingAfterFlight = null
  }
  if (activeScheduled) {
    resolveScheduled(activeScheduled, null)
    activeScheduled = null
  }
}

function drainAfterFlight(): void {
  const pending = pendingAfterFlight
  pendingAfterFlight = null
  if (!pending) return
  if (pending.generation !== scheduleGeneration) {
    resolveScheduled(pending, null)
    return
  }
  runScheduledRanking(pending)
}

function runScheduledRanking(entry: ScheduledRanking): void {
  if (entry.generation !== scheduleGeneration) {
    resolveScheduled(entry, null)
    return
  }
  if (inFlight) {
    pendingAfterFlight = entry
    return
  }

  activeScheduled = entry
  void rankSemanticDetections(entry.detections, entry.gate).then(
    (winner) => {
      if (activeScheduled === entry) activeScheduled = null
      resolveScheduled(
        entry,
        entry.generation === scheduleGeneration ? winner : null
      )
    },
    () => {
      if (activeScheduled === entry) activeScheduled = null
      resolveScheduled(entry, null)
    }
  )
}

export function resetRankerForTests(): void {
  inFlight = false
  consecutiveFailures = 0
  lastStrongDirectAt = 0
  rankingCache.clear()
  scheduleGeneration += 1
  if (debounceTimer) clearTimeout(debounceTimer)
  debounceTimer = null
  supersedeScheduledWork()
}

export function detectionCandidateId(detection: DetectionResult): string {
  return `${detection.book_number}:${detection.chapter}:${detection.verse}`
}

function rankableSemantic(detections: DetectionResult[]): DetectionResult[] {
  const seen = new Set<string>()
  const out: DetectionResult[] = []
  for (const detection of detections) {
    if (
      detection.source !== "semantic" ||
      detection.content_type === "egw" ||
      detection.book_number <= 0 ||
      detection.is_chapter_only
    ) {
      continue
    }
    const id = detectionCandidateId(detection)
    if (seen.has(id)) continue
    seen.add(id)
    out.push(detection)
  }
  out.sort((a, b) => b.confidence - a.confidence)
  return out.slice(0, MAX_CANDIDATES)
}

/** Promote candidates from one unambiguous book named in the spoken snippet.
 *  This is a preference, not a filter: candidates from other books remain
 *  available when the speaker is comparing passages or when the book mention
 *  is incidental. Upstream Rust retrieval also applies the same preference
 *  before its five-candidate cap. */
export function preferSpokenBook(
  detections: DetectionResult[],
  transcript: string
): DetectionResult[] {
  const haystack = transcript.toLowerCase()
  const namedBooks = new Set<number>()

  for (const detection of detections) {
    const book = detection.book_name.trim().toLowerCase()
    if (!book) continue
    const escaped = book.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    const pattern = new RegExp(`\\b${escaped}\\b`)
    if (pattern.test(haystack)) namedBooks.add(detection.book_number)
  }

  if (namedBooks.size !== 1) return detections
  const [bookNumber] = [...namedBooks]
  const matching = detections.filter((d) => d.book_number === bookNumber)
  if (matching.length === 0 || matching.length === detections.length) {
    return detections
  }
  return [
    ...matching,
    ...detections.filter((d) => d.book_number !== bookNumber),
  ]
}

export function buildRankingCandidates(
  detections: DetectionResult[],
  transcript = ""
): RankingCandidate[] {
  return preferSpokenBook(rankableSemantic(detections), transcript).map(
    (detection) => ({
      id: detectionCandidateId(detection),
      summary: `${detection.verse_ref} — ${detection.verse_text}`.slice(
        0,
        SUMMARY_TEXT_CHARS
      ),
      confidence: detection.confidence,
    })
  )
}

/** Record direct evidence before any asynchronous ranking work. Direct and
 * semantic detections arrive in separate batches, so this state bridges the
 * two worker emissions for a short, phrase-sized suppression window. */
export function noteBatchForGating(
  detections: DetectionResult[],
  gate: RankingGate
): void {
  const strongDirect = detections.some(
    (detection) =>
      detection.source === "direct" &&
      detection.confidence >= gate.confidenceThreshold
  )
  if (strongDirect) lastStrongDirectAt = Date.now()
}

/** True when local retrieval already has a clear semantic winner. */
export function retrievalIsDecisive(detections: DetectionResult[]): boolean {
  const ranked = rankableSemantic(detections)
  if (ranked.length === 0) return false
  const top = ranked[0].confidence
  if (top >= DECISIVE_SEMANTIC_CONFIDENCE) return true
  if (ranked.length === 1) return true
  return top - ranked[1].confidence >= MIN_AMBIGUITY_MARGIN
}

export function shouldRankDetections(
  detections: DetectionResult[],
  gate: RankingGate
): boolean {
  if (!gate.deepseekRankingEnabled || !gate.hasDeepseekApiKey) return false
  const strongDirect = detections.some(
    (detection) =>
      detection.source === "direct" &&
      detection.confidence >= gate.confidenceThreshold
  )
  if (strongDirect) return false
  const elapsedSinceDirect = Date.now() - lastStrongDirectAt
  if (
    lastStrongDirectAt > 0 &&
    elapsedSinceDirect >= 0 &&
    elapsedSinceDirect < DIRECT_SUPPRESSION_MS
  ) {
    return false
  }
  if (retrievalIsDecisive(detections)) return false
  return rankableSemantic(detections).length >= 2
}

export function pickRankingTranscript(detections: DetectionResult[]): string {
  let longest = ""
  for (const detection of detections) {
    if (
      detection.source === "semantic" &&
      detection.transcript_snippet.length > longest.length
    ) {
      longest = detection.transcript_snippet
    }
  }
  return longest.slice(0, MAX_TRANSCRIPT_CHARS)
}

function cacheKey(transcript: string, candidates: RankingCandidate[]): string {
  const ids = [...candidates.map((candidate) => candidate.id)].sort()
  return JSON.stringify([transcript, ids])
}

function rememberRanking(key: string, selectedId: string | null): void {
  if (rankingCache.has(key)) rankingCache.delete(key)
  if (rankingCache.size >= RANKING_CACHE_MAX) {
    const oldest = rankingCache.keys().next().value
    if (oldest !== undefined) rankingCache.delete(oldest)
  }
  rankingCache.set(key, selectedId)
}

/** Wait for a phrase to stop extending before asking the model. A newer batch
 * supersedes a pending one; if a previous cloud call is still running, the
 * newest batch waits and runs immediately after that call settles. */
export function scheduleRanking(
  detections: DetectionResult[],
  gate: RankingGate
): Promise<DetectionResult | null> {
  noteBatchForGating(detections, gate)
  supersedeScheduledWork()
  if (debounceTimer) clearTimeout(debounceTimer)
  const generation = ++scheduleGeneration

  if (!shouldRankDetections(detections, gate)) {
    return Promise.resolve(null)
  }

  return new Promise((resolve) => {
    pendingDebounced = { generation, detections, gate, resolve }
    debounceTimer = setTimeout(() => {
      debounceTimer = null
      const pending = pendingDebounced
      pendingDebounced = null
      if (pending) runScheduledRanking(pending)
    }, STABILITY_WINDOW_MS)
  })
}

export async function rankSemanticDetections(
  detections: DetectionResult[],
  gate: RankingGate
): Promise<DetectionResult | null> {
  noteBatchForGating(detections, gate)
  if (inFlight || consecutiveFailures >= CIRCUIT_BREAKER_FAILURES) return null
  if (!shouldRankDetections(detections, gate)) return null

  const transcript = pickRankingTranscript(detections)
  if (!transcript) return null

  const semantic = preferSpokenBook(rankableSemantic(detections), transcript)
  const candidates = semantic.map((detection) => ({
    id: detectionCandidateId(detection),
    summary: `${detection.verse_ref} — ${detection.verse_text}`.slice(
      0,
      SUMMARY_TEXT_CHARS
    ),
    confidence: detection.confidence,
  }))
  const key = cacheKey(transcript, candidates)
  const resolveFromId = (id: string | null) =>
    id ? (semantic.find((d) => detectionCandidateId(d) === id) ?? null) : null

  if (rankingCache.has(key)) {
    return resolveFromId(rankingCache.get(key) ?? null)
  }

  inFlight = true
  try {
    const selectedId = await invokeTauri<string | null>(
      "rank_detection_candidates",
      { transcript, candidates }
    )
    consecutiveFailures = 0
    rememberRanking(key, selectedId ?? null)
    return resolveFromId(selectedId ?? null)
  } catch {
    consecutiveFailures += 1
    return null
  } finally {
    inFlight = false
    drainAfterFlight()
  }
}
