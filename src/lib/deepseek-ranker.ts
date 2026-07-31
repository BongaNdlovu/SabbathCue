import { invokeTauri } from "@/lib/tauri-runtime"
import type { DetectionResult } from "@/types"
import type { RankingCandidate } from "@/types/ai-ranking"

const MAX_CANDIDATES = 5
const MAX_TRANSCRIPT_CHARS = 500
const SUMMARY_TEXT_CHARS = 80
const CIRCUIT_BREAKER_FAILURES = 3

export interface RankingGate {
  deepseekRankingEnabled: boolean
  hasDeepseekApiKey: boolean
  confidenceThreshold: number
}

let inFlight = false
let consecutiveFailures = 0

export function resetRankerForTests(): void {
  inFlight = false
  consecutiveFailures = 0
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

export function buildRankingCandidates(
  detections: DetectionResult[]
): RankingCandidate[] {
  return rankableSemantic(detections).map((detection) => ({
    id: detectionCandidateId(detection),
    summary: `${detection.verse_ref} — ${detection.verse_text}`.slice(
      0,
      SUMMARY_TEXT_CHARS
    ),
  }))
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

export async function rankSemanticDetections(
  detections: DetectionResult[],
  gate: RankingGate
): Promise<DetectionResult | null> {
  if (inFlight || consecutiveFailures >= CIRCUIT_BREAKER_FAILURES) return null
  if (!shouldRankDetections(detections, gate)) return null

  const semantic = rankableSemantic(detections)
  const candidates = buildRankingCandidates(detections)
  const transcript = pickRankingTranscript(detections)
  if (!transcript) return null

  inFlight = true
  try {
    // Rust resolves the streamed letter back to a real candidate id, or null
    // on abstention. Timeouts and HTTP errors reject and feed the breaker.
    const selectedId = await invokeTauri<string | null>(
      "rank_detection_candidates",
      { transcript, candidates }
    )
    consecutiveFailures = 0
    if (!selectedId) return null
    return (
      semantic.find(
        (detection) => detectionCandidateId(detection) === selectedId
      ) ?? null
    )
  } catch {
    consecutiveFailures += 1
    return null
  } finally {
    inFlight = false
  }
}
