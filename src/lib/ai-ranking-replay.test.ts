import { describe, expect, it } from "vitest"
import baseline from "../../data/detection-fixtures/ai-ranking-baseline.json"
import cases from "../../data/detection-fixtures/ai-ranking-cases.json"
import {
  evaluateRankingReplay,
  validateRankingReplayCase,
  type RankingReplayCase,
} from "./ai-ranking-replay"

const replayCases = cases as RankingReplayCase[]

describe("AI ranking replay corpus", () => {
  it("contains only bounded, self-consistent cases", () => {
    expect(replayCases.length).toBeGreaterThanOrEqual(5)
    for (const replayCase of replayCases) {
      expect(validateRankingReplayCase(replayCase)).toEqual([])
    }
  })

  it("reports selection, abstention, and candidate-set metrics", () => {
    const decisions = Object.fromEntries(
      replayCases.map((replayCase) => [
        replayCase.id,
        replayCase.expectedId !== null &&
          replayCase.candidates.some(
            (candidate) => candidate.id === replayCase.expectedId
          )
          ? replayCase.expectedId
          : null,
      ])
    )
    const metrics = evaluateRankingReplay(replayCases, decisions)

    expect(metrics.total).toBe(replayCases.length)
    expect(metrics.selectionPrecision).toBe(1)
    expect(metrics.abstentionRecall).toBe(1)
    expect(metrics.candidateSetRecall).toBeLessThan(1)
    expect(metrics.accuracy).toBe(6 / 7)
    expect(metrics).toMatchObject(baseline.metrics)
  })

  it("counts wrong selections and missed selections separately", () => {
    const decisions = Object.fromEntries(
      replayCases.map((replayCase, index) => [
        replayCase.id,
        index === 0 ? "not-offered" : null,
      ])
    )
    const metrics = evaluateRankingReplay(replayCases, decisions)

    expect(metrics.falseSelections).toBe(1)
    expect(metrics.missedSelections).toBeGreaterThan(0)
    expect(metrics.selectionPrecision).toBe(0)
  })
})
