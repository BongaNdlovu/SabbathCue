/** One verified local candidate offered to the AI ranker. The ranker may only
 *  choose among these — it never returns content of its own. */
export interface RankingEvidence {
  /** Internal local ordering evidence; never a model-generated score. */
  rankScore: number
  /** True only when the spoken batch explicitly names this candidate's book. */
  namedBookMatch: boolean
  /** True when the transcript contains a meaningful contiguous verse phrase. */
  exactPhraseMatch: boolean
  /** Capped meaningful terms shared by the transcript and verse text. */
  overlapTerms: string[]
}

export interface RankingCandidate {
  id: string
  reference: string
  verseText: string
  /** Local retrieval confidence 0–1 so the model can abstain on a weak pool. */
  confidence: number
  evidence: RankingEvidence
}
