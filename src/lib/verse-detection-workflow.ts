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
      (candidate.rank_score ?? candidate.confidence) >
      (best.rank_score ?? best.confidence)
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
  semanticMinConfidence: number
): DetectionResult | null {
  const directHits = detections.filter(
    (d) =>
      d.source === "direct" &&
      d.confidence >= minConfidence &&
      !d.is_chapter_only &&
      (isEgwDetection(d) || d.book_number > 0)
  )
  const directHit = bestDetection(directHits)
  if (directHit) return directHit

  if (!semanticDetectionEnabled) return null

  const semanticAutoLiveThreshold = Math.max(
    minConfidence,
    semanticMinConfidence
  )
  const semanticHits = detections.filter(
    (d) =>
      d.source === "semantic" &&
      d.confidence >= semanticAutoLiveThreshold &&
      !d.is_chapter_only &&
      d.book_number > 0
  )
  semanticHits.sort(
    (a, b) => (b.rank_score ?? b.confidence) - (a.rank_score ?? a.confidence)
  )
  const strongest = semanticHits[0]
  const runnerUp = semanticHits[1]
  if (
    strongest &&
    runnerUp &&
    (strongest.rank_score ?? strongest.confidence) -
      (runnerUp.rank_score ?? runnerUp.confidence) <
      SEMANTIC_AUTO_LIVE_MIN_MARGIN
  ) {
    return null
  }
  return strongest ?? null
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
const SEMANTIC_SINGLE_PASS_MATCH_STRENGTH = 0.95
const SEMANTIC_AUTO_LIVE_MIN_MARGIN = 0.02
const SEMANTIC_CONFIRMATION_WINDOW_MS = 8_000
const pendingSemanticConfirmations = new Map<string, number>()

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

let pendingAiSuggestion: Promise<void> = Promise.resolve()
let aiSuggestionEpoch = 0

/** Resolves once the in-flight AI suggestion for the last batch has settled.
 *  Tests await this instead of racing the fire-and-forget call. */
export function aiSuggestionSettledForTests(): Promise<void> {
  return pendingAiSuggestion
}

/** Ask the AI ranker which semantic candidate best matches the speech and
 *  record it as a display-only badge. Deliberately kept off the preview path:
 *  a suggestion never decides what goes to preview or live. */
async function maybeMarkAiSuggestion(
  detections: DetectionResult[]
): Promise<void> {
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
    if (epoch !== aiSuggestionEpoch) return
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
  } catch (error) {
    // Drop any earlier badge rather than leaving a stale suggestion on screen.
    if (epoch === aiSuggestionEpoch) {
      useDetectionStore.getState().markAiSuggested(null)
    }
    console.warn("[ai-ranking] Suggestion pass failed", error)
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

async function handleVerseDetectionsInternal(detections: DetectionResult[]) {
  const settings = useSettingsStore.getState()
  const acceptedDetections = detections.filter((detection) =>
    detectionAllowedBySettings(detection, settings)
  )
  useDetectionStore.getState().addDetections(acceptedDetections)

  // Fire-and-forget: AI ranking is a display-only suggestion and must never
  // block or influence the preview/auto-live path below. maybeMarkAiSuggestion
  // handles its own failures, so this promise never rejects.
  pendingAiSuggestion = maybeMarkAiSuggestion(acceptedDetections)

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
  const previewHit = autoPreview
    ? confirmedSemanticHit(
        selectPreviewHit(
          acceptedDetections,
          settings.confidenceThreshold,
          settings.semanticDetectionEnabled,
          settings.semanticConfidenceThreshold
        )
      )
    : null
  const resolvedDetections = new WeakMap<
    DetectionResult,
    ResolvedDetectionVerse
  >()
  if (previewHit) {
    recordAutoSelectionPerformance(previewHit)
    recordDetectionFeedback(previewHit, "auto-selected")
    if (isEgwDetection(previewHit)) {
      recordWorkflowTrace(
        "detection.preview.selected",
        "EGW direct hit selected",
        {
          detection: traceDetectionDetails(previewHit),
          autoQueued: previewHit.auto_queued,
        }
      )
      if (previewHit.auto_queued) {
        presentEgwParagraph(previewHit.egw_paragraph)
      } else {
        previewEgwParagraph(previewHit.egw_paragraph)
      }
    } else {
      const resolved = await resolveDetectionVerse(previewHit)
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

export async function handleVerseDetections(detections: DetectionResult[]) {
  detectionHandlingChain = detectionHandlingChain
    .catch((error) => {
      reportDetectionBatchError(error)
    })
    .then(() => handleVerseDetectionsInternal(detections))
    .catch((error) => {
      reportDetectionBatchError(error)
    })
  return detectionHandlingChain
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
