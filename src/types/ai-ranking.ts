/** One verified local candidate offered to the AI ranker. The ranker may only
 *  choose among these — it never returns content of its own. */
export interface RankingCandidate {
  id: string
  summary: string
}
