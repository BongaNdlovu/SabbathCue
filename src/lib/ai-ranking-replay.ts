export interface RankingReplayCandidate {
  id: string
  reference: string
  verseText: string
  confidence: number
}

export interface RankingReplayCase {
  id: string
  category: string
  transcript: string
  candidates: RankingReplayCandidate[]
  expectedId: string | null
}

export interface RankingReplayMetrics {
  total: number
  correctSelections: number
  falseSelections: number
  correctAbstentions: number
  missedSelections: number
  candidateSetHits: number
  positiveCases: number
  selectionPrecision: number
  abstentionRecall: number
  candidateSetRecall: number
  accuracy: number
}

export function validateRankingReplayCase(
  replayCase: RankingReplayCase
): string[] {
  const errors: string[] = []
  if (!replayCase.id.trim()) errors.push("case id is empty")
  if (!replayCase.transcript.trim()) errors.push("transcript is empty")
  if (replayCase.transcript.length > 500) {
    errors.push("transcript exceeds 500 characters")
  }
  if (replayCase.candidates.length === 0 || replayCase.candidates.length > 8) {
    errors.push("candidate count must be between 1 and 8")
  }
  const ids = new Set<string>()
  for (const candidate of replayCase.candidates) {
    if (!candidate.id.trim() || ids.has(candidate.id)) {
      errors.push(`duplicate or empty candidate id: ${candidate.id}`)
    }
    ids.add(candidate.id)
    if (!candidate.reference.trim() || !candidate.verseText.trim()) {
      errors.push(`candidate ${candidate.id} lacks reference or verse text`)
    }
    if (!Number.isFinite(candidate.confidence) || candidate.confidence < 0 || candidate.confidence > 1) {
      errors.push(`candidate ${candidate.id} has invalid confidence`)
    }
  }
  if (
    replayCase.expectedId !== null &&
    !ids.has(replayCase.expectedId) &&
    replayCase.category !== "retrieval-miss"
  ) {
    errors.push(`expected candidate is not present: ${replayCase.expectedId}`)
  }
  return errors
}

export function evaluateRankingReplay(
  cases: RankingReplayCase[],
  decisions: Readonly<Record<string, string | null>>
): RankingReplayMetrics {
  let correctSelections = 0
  let falseSelections = 0
  let correctAbstentions = 0
  let missedSelections = 0
  let candidateSetHits = 0
  let positiveCases = 0

  for (const replayCase of cases) {
    const decision = decisions[replayCase.id] ?? null
    const expected = replayCase.expectedId
    if (expected !== null) {
      positiveCases += 1
      if (replayCase.candidates.some((candidate) => candidate.id === expected)) {
        candidateSetHits += 1
      }
      const expectedWasOffered = replayCase.candidates.some(
        (candidate) => candidate.id === expected
      )
      if (decision === expected && expectedWasOffered) {
        correctSelections += 1
      } else {
        missedSelections += 1
        if (decision !== null) falseSelections += 1
      }
    } else if (decision === null) {
      correctAbstentions += 1
    } else {
      falseSelections += 1
    }
  }

  const selected = correctSelections + falseSelections
  return {
    total: cases.length,
    correctSelections,
    falseSelections,
    correctAbstentions,
    missedSelections,
    candidateSetHits,
    positiveCases,
    selectionPrecision: selected === 0 ? 1 : correctSelections / selected,
    abstentionRecall:
      cases.length - positiveCases === 0
        ? 1
        : correctAbstentions / (cases.length - positiveCases),
    candidateSetRecall: positiveCases === 0 ? 1 : candidateSetHits / positiveCases,
    accuracy:
      cases.length === 0
        ? 1
        : (correctSelections + correctAbstentions) / cases.length,
  }
}
