/** One verified local candidate offered to the AI ranker. The ranker may only
 *  choose among these — it never returns content of its own. */
export interface RankingCandidate {
  id: string
  reference: string
  verseText: string
  /** Local retrieval confidence 0–1 so the model can abstain on a weak pool. */
  confidence: number
}
