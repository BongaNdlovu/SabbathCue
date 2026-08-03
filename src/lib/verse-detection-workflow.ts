import {
  previewVerseAndMaybeAutoLive,
  createScriptureQueueItem,
  previewEgwParagraph,
  presentEgwParagraph,
  createEgwQueueItem,
} from "@/lib/presentation-workflow"
import { bibleActions } from "@/hooks/use-bible"
import { useBibleStore } from "@/stores/bible-store"
import { useBroadcastLiveStore } from "@/stores/broadcast/live-store"
import { useBroadcastOutputIssueStore } from "@/stores/broadcast/output-issue-store"
import { useDetectionStore } from "@/stores/detection-store"
import { useQueueStore } from "@/stores/queue-store"
import { useSettingsStore } from "@/stores/settings-store"
import {
  recordWorkflowTrace,
  traceDetectionBatchDetails,
  traceDetectionDetails,
  traceReadingAdvanceDetails,
  traceVerseDetails,
} from "@/lib/workflow-trace"
import { recordDetectionFeedback } from "@/lib/detection-feedback"
import { recordAutoSelectionPerformance } from "@/lib/detection-profiler"
import { detectionCandidateId, scheduleRanking } from "@/lib/deepseek-ranker"
import type {
  DetectionResult,
  EgwParagraph,
  ReadingAdvance,
  Verse,
} from "@/types"

function detectionLikeToVerse({
  book_number,
  book_name,
  chapter,
  verse,
  verse_text,
}: {
  book_number: number
  book_name: string
  chapter: number
  verse: number
  verse_text: string
}): Verse {
  return {
    id: 0,
    translation_id: useBibleStore.getState().activeTranslationId,
    book_number,
    book_name,
    book_abbreviation: "",
    chapter,
    verse,
    text: verse_text,
  }
}

function readingAdvanceToDetection(advance: ReadingAdvance): DetectionResult {
  return {
    content_type: "bible",
    verse_ref: advance.reference,
    verse_text: advance.verse_text,
    book_name: advance.book_name,
    book_number: advance.book_number,
    chapter: advance.chapter,
    verse: advance.verse,
    confidence: advance.confidence,
    source: "direct",
    auto_queued: false,
    transcript_snippet: "",
    is_chapter_only: false,
    egw_paragraph: null,
  }
}

function findCurrentChapterVerse(detection: DetectionResult): Verse | null {
  const { activeTranslationId, currentChapter } = useBibleStore.getState()
  return (
    currentChapter.find(
      (verse) =>
        verse.translation_id === activeTranslationId &&
        verse.book_number === detection.book_number &&
        verse.chapter === detection.chapter &&
        verse.verse === detection.verse
    ) ?? null
  )
}

function isEgwDetection(
  detection: DetectionResult
): detection is DetectionResult & { egw_paragraph: EgwParagraph } {
  return detection.content_type === "egw" && Boolean(detection.egw_paragraph)
}

interface ResolvedDetectionVerse {
  verse: Verse
  usedFallback: boolean
  fallbackReason?: string
}

async function resolveDetectionVerse(
  detection: DetectionResult
): Promise<ResolvedDetectionVerse> {
  if (
    detection.book_number > 0 &&
    detection.chapter > 0 &&
    detection.verse > 0
  ) {
    try {
      const verse = await bibleActions.fetchVerse(
        detection.book_number,
        detection.chapter,
        detection.verse
      )
      if (verse) {
        return { verse, usedFallback: false }
      }
    } catch (error) {
      const currentVerse = findCurrentChapterVerse(detection)
      if (currentVerse) {
        useBroadcastOutputIssueStore.getState().reportOutputIssue({
          outputId: "global",
          kind: "verse-lookup",
          title: "Verse lookup failed",
          description: `Used loaded chapter text for ${detection.verse_ref}: ${String(error)}`,
        })
        return {
          verse: currentVerse,
          usedFallback: true,
          fallbackReason: "chapter-cache",
        }
      }

      useBroadcastOutputIssueStore.getState().reportOutputIssue({
        outputId: "global",
        kind: "verse-lookup",
        title: "Verse lookup failed",
        description: `Used detection text for ${detection.verse_ref}: ${String(error)}`,
      })
      return {
        verse: detectionLikeToVerse(detection),
        usedFallback: true,
        fallbackReason: "detection-text",
      }
    }

    const currentVerse = findCurrentChapterVerse(detection)
    if (currentVerse) {
      return { verse: currentVerse, usedFallback: false }
    }
  }
  return {
    verse: detectionLikeToVerse(detection),
    usedFallback: true,
    fallbackReason: "unresolved-detection",
  }
}

function bestDetection(detections: DetectionResult[]): DetectionResult | null {
  if (detections.length === 0) return null

  let best = detections[0]
  for (let i = 1; i < detections.length; i += 1) {
    const candidate = detections[i]
    if (
      candidate.confidence > best.confidence ||
      (candidate.confidence === best.confidence &&
        (candidate.rank_score ?? candidate.confidence) >
          (best.rank_score ?? best.confidence))
    ) {
      best = candidate
    }
  }
  return best
}

interface DetectionSettingsSnapshot {
  semanticDetectionEnabled: boolean
  semanticConfidenceThreshold: number
}

function detectionAllowedBySettings(
  detection: DetectionResult,
  settings: DetectionSettingsSnapshot
): boolean {
  return (
    detection.source !== "semantic" ||
    (settings.semanticDetectionEnabled &&
      detection.confidence >= settings.semanticConfidenceThreshold)
  )
}

function selectPreviewHit(
  detections: DetectionResult[],
  minConfidence: number,
  semanticDetectionEnabled: boolean,
  semanticMinConfidence: number,
  aiWinner: DetectionResult | null
): DetectionResult | null {
  const directHits = detections.filter(
    (d) =>
      d.source === "direct" &&
      d.confidence >= minConfidence &&
      !d.is_chapter_only &&
      (isEgwDetection(d) || d.book_number > 0)
  )
  if (!semanticDetectionEnabled) return bestDetection(directHits)

  const semanticAutoLiveThreshold = Math.max(
    minConfidence,
    semanticMinConfidence
  )
  const semanticCandidates = detections.filter(
    (d) =>
      d.source === "semantic" &&
      d.confidence >= semanticMinConfidence &&
      !d.is_chapter_only &&
      (isEgwDetection(d) || d.book_number > 0)
  )
  semanticCandidates.sort(
    (a, b) =>
      b.confidence - a.confidence ||
      (b.rank_score ?? b.confidence) - (a.rank_score ?? a.confidence)
  )
  const strongest = semanticCandidates.find(
    (candidate) => candidate.confidence >= semanticAutoLiveThreshold
  )
  const aiConfirmed =
    aiWinner?.source === "semantic" &&
    aiWinner.confidence >= semanticMinConfidence
      ? aiWinner
      : null
  if (!strongest) {
    return bestDetection(
      aiConfirmed ? [...directHits, aiConfirmed] : directHits
    )
  }
  const runnerUp = semanticCandidates.find(
    (candidate) => candidate !== strongest
  )
  if (
    runnerUp &&
    detectionOrderingGap(strongest, runnerUp) < SEMANTIC_AUTO_LIVE_MIN_MARGIN
  ) {
    return bestDetection(
      aiConfirmed ? [...directHits, aiConfirmed] : directHits
    )
  }
  const finalists = [...directHits, strongest]
  if (aiConfirmed && aiConfirmed !== strongest) finalists.push(aiConfirmed)
  return bestDetection(finalists)
}

/**
 * Keep confidence as the primary decision signal. The internal rank score is
 * a reranking tie-breaker for equal-confidence semantic candidates (for
 * example, two event matches), not a reason to suppress a stronger quote.
 */
function detectionOrderingGap(
  strongest: DetectionResult,
  runnerUp: DetectionResult
): number {
  const confidenceGap = strongest.confidence - runnerUp.confidence
  if (Math.abs(confidenceGap) > Number.EPSILON) return confidenceGap
  return (
    (strongest.rank_score ?? strongest.confidence) -
    (runnerUp.rank_score ?? runnerUp.confidence)
  )
}

async function queueDetectedVerse(
  detection: DetectionResult,
  resolvedDetection?: ResolvedDetectionVerse
): Promise<void> {
  if (isEgwDetection(detection)) {
    if (!detection.auto_queued) {
      recordWorkflowTrace(
        "detection.queue.skipped",
        "EGW detection not queued",
        {
          reason: "auto_queued_false",
          detection: traceDetectionDetails(detection),
        }
      )
      return
    }

    useQueueStore.getState().addOrFlashItem(
      createEgwQueueItem(detection.egw_paragraph, {
        confidence: detection.confidence,
        source: "ai-direct",
      })
    )
    recordWorkflowTrace("detection.queue.added", "EGW detection queued", {
      detection: traceDetectionDetails(detection),
    })
    return
  }

  const { verse } =
    resolvedDetection ?? (await resolveDetectionVerse(detection))
  if (
    !detection.is_chapter_only &&
    detection.source === "direct" &&
    useQueueStore
      .getState()
      .updateEarlyRef(
        verse.book_number,
        verse.chapter,
        verse.verse,
        detection.verse_ref,
        verse.text
      )
  ) {
    recordWorkflowTrace(
      "detection.queue.added",
      "Existing early reference updated",
      {
        action: "update_existing_early_ref",
        detection: traceDetectionDetails(detection),
        verse: traceVerseDetails(verse),
      }
    )
    return
  }

  if (!detection.auto_queued) {
    recordWorkflowTrace("detection.queue.skipped", "Detection not queued", {
      reason: "auto_queued_false",
      detection: traceDetectionDetails(detection),
      verse: traceVerseDetails(verse),
    })
    return
  }

  useQueueStore.getState().addOrFlashDetectionItem(
    createScriptureQueueItem(verse, {
      reference: detection.verse_ref,
      confidence: detection.confidence,
      source: detection.source === "direct" ? "ai-direct" : "ai-semantic",
      is_chapter_only: detection.is_chapter_only,
    })
  )
  recordWorkflowTrace("detection.queue.added", "Detection queued", {
    detection: traceDetectionDetails(detection),
    verse: traceVerseDetails(verse),
  })
}

let detectionHandlingChain: Promise<void> = Promise.resolve()
let detectionHandlingGeneration = 0
/** Resolvers woken when `detectionHandlingGeneration` advances past a batch. */
const staleBatchWaiters: Array<{
  batchGeneration: number
  resolve: () => void
}> = []
const DETECTION_ARBITRATION_WINDOW_MS = 400
let pendingDetectionBatch: DetectionResult[] = []
let detectionArbitrationTimer: ReturnType<typeof setTimeout> | null = null
let detectionArbitrationWaiters: Array<() => void> = []
const SEMANTIC_SINGLE_PASS_MATCH_STRENGTH = 0.95
const SEMANTIC_AUTO_LIVE_MIN_MARGIN = 0.02
const SEMANTIC_CONFIRMATION_WINDOW_MS = 8_000
const pendingSemanticConfirmations = new Map<string, number>()

function notifyStaleBatchWaiters(): void {
  const current = detectionHandlingGeneration
  for (let i = staleBatchWaiters.length - 1; i >= 0; i -= 1) {
    if (staleBatchWaiters[i].batchGeneration !== current) {
      const [waiter] = staleBatchWaiters.splice(i, 1)
      waiter.resolve()
    }
  }
}

/** Resolves as soon as `batchGeneration` is no longer the latest handling generation. */
function whenBatchBecomesStale(batchGeneration: number): Promise<void> {
  return new Promise((resolve) => {
    if (batchGeneration !== detectionHandlingGeneration) {
      resolve()
      return
    }
    staleBatchWaiters.push({ batchGeneration, resolve })
    // If generation advanced between the check and the push, notify now so
    // this waiter cannot sit forever (single-threaded, but keeps the invariant
    // obvious and safe under future re-entry).
    if (batchGeneration !== detectionHandlingGeneration) {
      notifyStaleBatchWaiters()
    }
  })
}

function discardStaleDetectionBatch(
  generation: number,
  acceptedCount: number
): boolean {
  if (generation === detectionHandlingGeneration) return false
  recordWorkflowTrace(
    "detection.preview.skipped",
    "Stale detection batch discarded after newer speech",
    {
      generation,
      latestGeneration: detectionHandlingGeneration,
      count: acceptedCount,
    }
  )
  return true
}

export function resetSemanticConfirmationForTests() {
  pendingSemanticConfirmations.clear()
}

export function pendingSemanticConfirmationCountForTests() {
  return pendingSemanticConfirmations.size
}

function confirmedSemanticHit(
  detection: DetectionResult | null
): DetectionResult | null {
  if (!detection || detection.source !== "semantic") {
    pendingSemanticConfirmations.clear()
    return detection
  }

  if (detection.confidence >= SEMANTIC_SINGLE_PASS_MATCH_STRENGTH) {
    pendingSemanticConfirmations.clear()
    return detection
  }

  const key = `${detection.book_number}:${detection.chapter}:${detection.verse}`
  const now = Date.now()

  // Evict confirmations that have aged out of the window so the map cannot
  // grow unbounded over a long session.
  for (const [pendingKey, seenAt] of pendingSemanticConfirmations) {
    if (now - seenAt > SEMANTIC_CONFIRMATION_WINDOW_MS) {
      pendingSemanticConfirmations.delete(pendingKey)
    }
  }

  if (pendingSemanticConfirmations.has(key)) {
    pendingSemanticConfirmations.delete(key)
    return detection
  }

  pendingSemanticConfirmations.set(key, now)
  return null
}

let pendingAiSuggestion: Promise<DetectionResult | null> = Promise.resolve(null)
let aiSuggestionEpoch = 0

/** Resolves once the in-flight AI suggestion for the last batch has settled.
 *  Tests await this instead of racing the fire-and-forget call. */
export function aiSuggestionSettledForTests(): Promise<DetectionResult | null> {
  return pendingAiSuggestion
}

/** Ask the AI ranker which semantic candidate best matches the speech and
 *  record it as a suggestion. In Auto Preview it is an advisory arbiter only
 *  when local retrieval is ambiguous; direct and stronger local candidates
 *  remain authoritative. */
async function maybeMarkAiSuggestion(
  detections: DetectionResult[]
): Promise<DetectionResult | null> {
  aiSuggestionEpoch += 1
  const epoch = aiSuggestionEpoch
  try {
    const settings = useSettingsStore.getState()
    const winner = await scheduleRanking(detections, {
      deepseekRankingEnabled: settings.deepseekRankingEnabled,
      hasDeepseekApiKey: settings.hasDeepseekApiKey,
      confidenceThreshold: settings.confidenceThreshold,
    })
    // A newer batch owns the badge now; a stale flight must not overwrite it
    // (e.g. after a strong direct hit made the newer batch clear the badge).
    if (epoch !== aiSuggestionEpoch) return null
    useDetectionStore
      .getState()
      .markAiSuggested(winner ? detectionCandidateId(winner) : null)
    if (winner) {
      recordWorkflowTrace(
        "detection.ai.suggested",
        "AI ranker selected a candidate",
        { detection: traceDetectionDetails(winner) }
      )
    }
    return winner
  } catch (error) {
    // Drop any earlier badge rather than leaving a stale suggestion on screen.
    if (epoch === aiSuggestionEpoch) {
      useDetectionStore.getState().markAiSuggested(null)
    }
    console.warn("[ai-ranking] Suggestion pass failed", error)
    return null
  }
}

function reportDetectionBatchError(error: unknown): void {
  useBroadcastOutputIssueStore.getState().reportOutputIssue({
    outputId: "global",
    kind: "auto-detection",
    title: "Detection batch failed",
    description: `An unexpected detection batch error occurred: ${String(error)}`,
  })
}

async function handleVerseDetectionsInternal(
  detections: DetectionResult[],
  generation: number
) {
  const settings = useSettingsStore.getState()
  const acceptedDetections = detections.filter((detection) =>
    detectionAllowedBySettings(detection, settings)
  )
  useDetectionStore.getState().addDetections(acceptedDetections)

  // Start ranking immediately so Auto Preview can use the bounded arbiter for
  // ambiguous semantic batches. Strong direct hits bypass the await below;
  // newer Auto Preview batches run concurrently and carry their own generation.
  const aiSuggestion = maybeMarkAiSuggestion(acceptedDetections)
  pendingAiSuggestion = aiSuggestion

  const autoPreview = settings.autoMode
  recordWorkflowTrace("detection.batch", "Detection batch entered workflow", {
    ...traceDetectionBatchDetails(acceptedDetections),
    incomingCount: detections.length,
    suppressedBySettings: detections.length - acceptedDetections.length,
    autoMode: settings.autoMode,
    confidenceThreshold: settings.confidenceThreshold,
    semanticDetectionEnabled: settings.semanticDetectionEnabled,
    semanticConfidenceThreshold: settings.semanticConfidenceThreshold,
  })
  const hasStrongDirectHit = acceptedDetections.some(
    (detection) =>
      detection.source === "direct" &&
      detection.confidence >= settings.confidenceThreshold &&
      !detection.is_chapter_only &&
      (isEgwDetection(detection) || detection.book_number > 0)
  )
  // Strong direct hits skip the ranking await entirely. Ambiguous Auto Preview
  // batches wait for ranking only while they remain the latest generation —
  // a newer batch must not leave this handler blocked on a stale flight.
  // Manual mode serializes batches and still processes older generations, so
  // staleness discard applies only when autoPreview is on.
  //
  // Note: `scheduleRanking` also supersedes prior flights with null when a
  // newer batch schedules. The generation race is defense-in-depth so this
  // handler cannot hang if ranking never settles (tests, circuit changes).
  let aiWinner: DetectionResult | null = null
  if (autoPreview && !hasStrongDirectHit) {
    if (discardStaleDetectionBatch(generation, acceptedDetections.length)) {
      return
    }
    aiWinner = await Promise.race([
      aiSuggestion,
      whenBatchBecomesStale(generation).then(() => null),
    ])
  }
  if (
    autoPreview &&
    discardStaleDetectionBatch(generation, acceptedDetections.length)
  ) {
    return
  }
  const selectedHit = autoPreview
    ? selectPreviewHit(
        acceptedDetections,
        settings.confidenceThreshold,
        settings.semanticDetectionEnabled,
        settings.semanticConfidenceThreshold,
        aiWinner
      )
    : null
  const previewHit =
    selectedHit && selectedHit === aiWinner
      ? selectedHit
      : confirmedSemanticHit(selectedHit)
  const resolvedDetections = new WeakMap<
    DetectionResult,
    ResolvedDetectionVerse
  >()
  if (previewHit) {
    recordAutoSelectionPerformance(previewHit)
    recordDetectionFeedback(previewHit, "auto-selected")
    if (isEgwDetection(previewHit)) {
      const autoLive = useBroadcastLiveStore.getState().readingModeAutoLive
      recordWorkflowTrace("detection.preview.selected", "EGW hit selected", {
        detection: traceDetectionDetails(previewHit),
        autoQueued: previewHit.auto_queued,
        autoLive,
      })
      if (autoLive) {
        presentEgwParagraph(previewHit.egw_paragraph)
      } else {
        previewEgwParagraph(previewHit.egw_paragraph)
      }
    } else {
      const resolved = await resolveDetectionVerse(previewHit)
      // Verse lookup can outlive a newer batch the same way ranking can;
      // never stage preview/live from a superseded generation.
      if (
        autoPreview &&
        discardStaleDetectionBatch(generation, acceptedDetections.length)
      ) {
        return
      }
      resolvedDetections.set(previewHit, resolved)
      recordWorkflowTrace(
        "detection.preview.selected",
        "Detection selected for preview",
        {
          detection: traceDetectionDetails(previewHit),
          verse: traceVerseDetails(resolved.verse),
          usedFallback: resolved.usedFallback,
          fallbackReason: resolved.fallbackReason,
        }
      )
      previewVerseAndMaybeAutoLive(resolved.verse, { autoLive: true })
    }
  } else if (autoPreview) {
    recordWorkflowTrace(
      "detection.preview.skipped",
      "No trusted hit met preview criteria",
      {
        count: acceptedDetections.length,
        confidenceThreshold: settings.confidenceThreshold,
        semanticDetectionEnabled: settings.semanticDetectionEnabled,
        semanticConfidenceThreshold: settings.semanticConfidenceThreshold,
      }
    )
  }

  // In Auto mode, detections only stage to preview; the queue stays
  // operator-driven.
  if (autoPreview) {
    recordWorkflowTrace(
      "detection.queue.skipped",
      "Auto mode keeps detection queue operator-driven",
      {
        reason: "auto_mode_preview_only",
        count: acceptedDetections.length,
      }
    )
    return
  }

  for (const detection of acceptedDetections) {
    await queueDetectedVerse(detection, resolvedDetections.get(detection))
  }
}

export function handleVerseDetections(detections: DetectionResult[]): Promise<void> {
  const generation = ++detectionHandlingGeneration
  notifyStaleBatchWaiters()
  const autoMode = useSettingsStore.getState().autoMode
  const task = autoMode
    ? handleVerseDetectionsInternal(detections, generation)
    : detectionHandlingChain
        .catch((error) => {
          reportDetectionBatchError(error)
        })
        .then(() => handleVerseDetectionsInternal(detections, generation))

  const handled = task.catch((error) => {
    reportDetectionBatchError(error)
  })
  if (!autoMode) detectionHandlingChain = handled
  return handled
}

export function scheduleVerseDetections(
  detections: DetectionResult[]
): Promise<void> {
  pendingDetectionBatch.push(...detections)

  const settled = new Promise<void>((resolve) => {
    detectionArbitrationWaiters.push(resolve)
  })
  if (detectionArbitrationTimer) return settled

  detectionArbitrationTimer = setTimeout(() => {
    const batch = pendingDetectionBatch
    const waiters = detectionArbitrationWaiters
    pendingDetectionBatch = []
    detectionArbitrationWaiters = []
    detectionArbitrationTimer = null

    void handleVerseDetections(batch).finally(() => {
      for (const resolve of waiters) resolve()
    })
  }, DETECTION_ARBITRATION_WINDOW_MS)

  return settled
}

export function resetDetectionArbitrationForTests(): void {
  if (detectionArbitrationTimer) clearTimeout(detectionArbitrationTimer)
  detectionArbitrationTimer = null
  pendingDetectionBatch = []
  const waiters = detectionArbitrationWaiters
  detectionArbitrationWaiters = []
  for (const resolve of waiters) resolve()
  // Invalidate any Auto Preview task that was still awaiting a lookup or
  // ranking promise when the test/session arbitration state was reset.
  detectionHandlingGeneration += 1
  notifyStaleBatchWaiters()
  detectionHandlingChain = Promise.resolve()
}

export function handleReadingAdvance(advance: ReadingAdvance) {
  if (advance.book_number <= 0) {
    recordWorkflowTrace("reading.ignored", "Reading advance ignored", {
      reason: "invalid_book",
      ...traceReadingAdvanceDetails(advance),
    })
    return
  }

  // Reading mode streams high-confidence advances while a passage is read.
  // Only auto-stage them in Auto broadcast mode; in Manual mode the operator
  // drives preview/live manually.
  const settings = useSettingsStore.getState()
  if (!settings.autoMode) {
    recordWorkflowTrace("reading.ignored", "Reading advance ignored", {
      reason: "manual_mode",
      ...traceReadingAdvanceDetails(advance),
    })
    return
  }

  const verse = detectionLikeToVerse({
    book_number: advance.book_number,
    book_name: advance.book_name,
    chapter: advance.chapter,
    verse: advance.verse,
    verse_text: advance.verse_text,
  })

  const broadcast = useBroadcastLiveStore.getState()
  recordWorkflowTrace("reading.accepted", "Reading advance accepted", {
    ...traceReadingAdvanceDetails(advance),
    liveWasOn: broadcast.isLive,
    readingModeAutoLive: broadcast.readingModeAutoLive,
    verse: traceVerseDetails(verse),
  })

  // Surface the advancing verse in the detections panel — reading mode otherwise
  // stages straight to preview, leaving the operator with no detection card for
  // the verse currently being read.
  useDetectionStore.getState().addDetection(readingAdvanceToDetection(advance))

  previewVerseAndMaybeAutoLive(verse, {
    autoLive: true,
  })
}
