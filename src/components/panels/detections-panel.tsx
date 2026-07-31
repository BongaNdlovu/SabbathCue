import { useEffect, useState } from "react"
import { cn } from "@/lib/utils"
import { recordDetectionFeedback } from "@/lib/detection-feedback"
import { PanelHeader } from "@/components/ui/panel-header"
import { PanelEmptyState } from "@/components/ui/panel-empty-state"
import { ConfidenceRing } from "@/components/ui/confidence-ring"
import { Button } from "@/components/ui/button"
import { CollectDetectionButton } from "@/components/panels/collect-detection-button"
import {
  BrainCircuitIcon,
  EyeIcon,
  PlayIcon,
  PlusIcon,
  RadarIcon,
  SparklesIcon,
} from "lucide-react"
import { useDetection, detectionActions } from "@/hooks/use-detection"
import { useSettingsStore } from "@/stores/settings-store"
import { useCollectedDetectionsStore } from "@/stores/collected-detections-store"
import { useQueueStore } from "@/stores/queue-store"
import {
  buildDetectionContextStack,
  buildHeldReferenceCandidates,
  useDetectionStore,
  type DetectionContextEntry,
  type HeldReferenceCandidate,
} from "@/stores/detection-store"
import { detectionCandidateId } from "@/lib/deepseek-ranker"
import {
  detectionToVerse,
  presentVerse,
  selectPreviewVerse,
  createScriptureQueueItem,
  previewEgwParagraph,
  presentEgwParagraph,
  createEgwQueueItem,
} from "@/lib/presentation-workflow"
import { loadHymnVoiceControl } from "@/services/hymnal/hymn-voice-control-loader"
import type { DetectionResult, EgwParagraph, HymnDetection } from "@/types"

const SOURCE_COLORS: Record<
  string,
  { bg: string; text: string; label: string }
> = {
  direct: { bg: "bg-green-500/15", text: "text-green-600", label: "Direct" },
  semantic: {
    bg: "bg-indigo-500/15",
    text: "text-indigo-700 dark:text-indigo-300",
    label: "Semantic",
  },
  hymn: {
    bg: "bg-amber-500/15",
    text: "text-amber-700 dark:text-amber-300",
    label: "Hymn",
  },
}

export function SourceBadge({ source }: { source: string }) {
  const style = SOURCE_COLORS[source] ?? {
    bg: "bg-[var(--shell-bg-sunken)]",
    text: "text-muted-foreground",
    label: source,
  }
  return (
    <span
      className={`rounded px-1.5 py-0.5 text-[0.5625rem] font-medium tracking-wider uppercase ${style.bg} ${style.text}`}
    >
      {style.label}
    </span>
  )
}

function isEgwDetection(
  detection: DetectionResult
): detection is DetectionResult & { egw_paragraph: EgwParagraph } {
  return detection.content_type === "egw" && Boolean(detection.egw_paragraph)
}

function isHymnDetection(
  detection: DetectionResult
): detection is DetectionResult & { hymn: HymnDetection } {
  return detection.content_type === "hymn" && Boolean(detection.hymn)
}

/** Emphasize the spoken EGW sentence when an anchor offset is present. */
function DetectionQuoteText({ detection }: { detection: DetectionResult }) {
  const text = detection.verse_text
  const start = detection.match_char_start
  if (
    start == null ||
    start < 0 ||
    start >= text.length ||
    detection.content_type !== "egw"
  ) {
    return <>{text}</>
  }
  // Offset is a UTF-8 byte index from Rust; for BMP/Latin text (EGW) byte===code unit.
  const before = text.slice(0, start)
  const rest = text.slice(start)
  const sentenceEnd = rest.search(/[.!?](?:\s|$)/)
  const emphasisLen = sentenceEnd >= 0 ? sentenceEnd + 1 : Math.min(rest.length, 160)
  const emphasis = rest.slice(0, emphasisLen)
  const after = rest.slice(emphasisLen)
  return (
    <>
      {before}
      <span className="font-medium text-foreground">{emphasis}</span>
      {after}
    </>
  )
}

function HymnDetectionCard({
  detection,
}: {
  detection: DetectionResult & { hymn: HymnDetection }
}) {
  const { title } = detection.hymn
  const actions = getDetectionActions(detection)

  return (
    <div className="queue-item p-3 last:border-0">
      <div className="flex items-center gap-2">
        <ConfidenceRing confidence={detection.confidence} />
        <span className="text-xs font-medium text-muted-foreground">
          {Math.round(detection.confidence * 100)}%
        </span>
        <SourceBadge source="hymn" />
        <span className="text-sm font-semibold text-foreground">
          {detection.verse_ref}
        </span>
      </div>

      {title && (
        <p className="mt-1 line-clamp-2 text-sm leading-relaxed text-muted-foreground">
          {title}
        </p>
      )}

      <div className="mt-2 flex flex-wrap gap-2">
        <Button
          size="sm"
          variant="outline"
          className="gap-1"
          onClick={actions.preview}
        >
          <EyeIcon className="size-3" />
          Preview
        </Button>
        <Button size="sm" className="gap-1" onClick={actions.present}>
          <PlayIcon className="size-3" />
          Present
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="gap-1"
          onClick={actions.queue}
        >
          <PlusIcon className="size-3" />
          Queue
        </Button>
        <CollectDetectionButton detection={detection} />
      </div>
    </div>
  )
}

/**
 * Resolve the preview / live / queue intent for a detection (verse, EGW paragraph,
 * or hymn). Single source of truth shared by the detection cards and the Live Desk
 * latest-detection bar.
 */
export function getDetectionActions(detection: DetectionResult): {
  preview: () => void
  present: () => void
  queue: () => void
} {
  const recordCollected = () =>
    useCollectedDetectionsStore.getState().record(detection)
  const recordFeedback = (
    action: "previewed" | "presented" | "queued"
  ) => recordDetectionFeedback(detection, action)

  if (isHymnDetection(detection)) {
    const { number } = detection.hymn
    return {
      preview: () => {
        recordFeedback("previewed")
        void loadHymnVoiceControl().then((mod) =>
          mod.previewHymnByNumber(number)
        )
      },
      present: () => {
        recordFeedback("presented")
        recordCollected()
        void loadHymnVoiceControl().then((mod) =>
          mod.presentHymnByNumber(number)
        )
      },
      queue: () => {
        recordFeedback("queued")
        recordCollected()
        void loadHymnVoiceControl().then((mod) => mod.queueHymnByNumber(number))
      },
    }
  }

  if (isEgwDetection(detection)) {
    const egwParagraph = detection.egw_paragraph
    return {
      preview: () => {
        recordFeedback("previewed")
        previewEgwParagraph(egwParagraph)
      },
      present: () => {
        recordFeedback("presented")
        recordCollected()
        presentEgwParagraph(egwParagraph)
      },
      queue: () => {
        recordFeedback("queued")
        recordCollected()
        useQueueStore.getState().addOrFlashItem(
          createEgwQueueItem(egwParagraph, {
            confidence: detection.confidence,
            source: "ai-direct",
          })
        )
      },
    }
  }

  const verse = detectionToVerse(detection)
  return {
    preview: () => {
      recordFeedback("previewed")
      selectPreviewVerse(verse)
    },
    present: () => {
      recordFeedback("presented")
      recordCollected()
      presentVerse(verse)
    },
    queue: () => {
      recordFeedback("queued")
      recordCollected()
      useQueueStore.getState().addOrFlashItem(
        createScriptureQueueItem(verse, {
          reference: detection.verse_ref,
          confidence: detection.confidence,
          source: detection.source === "direct" ? "ai-direct" : "ai-semantic",
        })
      )
    },
  }
}

/** Marks the candidate the AI ranker chose. Suggestion only — the operator
 *  still decides what goes to preview or live. */
function AiSuggestedBadge({ detection }: { detection: DetectionResult }) {
  const aiSuggestedKey = useDetectionStore((s) => s.aiSuggestedKey)
  if (!aiSuggestedKey || aiSuggestedKey !== detectionCandidateId(detection)) {
    return null
  }
  return (
    <span
      className="flex items-center gap-1 rounded bg-violet-500/15 px-1.5 py-0.5 text-[0.5625rem] font-medium tracking-wider text-violet-700 uppercase dark:text-violet-300"
      title="Suggested by AI ranking from locally detected candidates"
    >
      <SparklesIcon className="size-2.5" />
      AI suggested
    </span>
  )
}

function DetectionCard({ detection }: { detection: DetectionResult }) {
  if (isHymnDetection(detection)) {
    return <HymnDetectionCard detection={detection} />
  }

  const actions = getDetectionActions(detection)

  return (
    <div className="queue-item p-3 last:border-0">
      <div className="flex items-center gap-2">
        <ConfidenceRing confidence={detection.confidence} />
        <span className="text-xs font-medium text-muted-foreground">
          {Math.round(detection.confidence * 100)}%
        </span>
        <SourceBadge source={detection.source} />
        <AiSuggestedBadge detection={detection} />
        <span className="text-sm font-semibold text-foreground">
          {detection.verse_ref}
        </span>
      </div>

      {detection.verse_text && (
        <p className="mt-1 line-clamp-2 text-sm leading-relaxed text-muted-foreground">
          <DetectionQuoteText detection={detection} />
        </p>
      )}

      <div className="mt-2 flex flex-wrap gap-2">
        <Button
          size="sm"
          variant="outline"
          className="gap-1"
          onClick={actions.preview}
        >
          <EyeIcon className="size-3" />
          Preview
        </Button>
        <Button size="sm" className="gap-1" onClick={actions.present}>
          <PlayIcon className="size-3" />
          Present
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="gap-1"
          onClick={actions.queue}
        >
          <PlusIcon className="size-3" />
          Queue
        </Button>
        <CollectDetectionButton detection={detection} />
      </div>
    </div>
  )
}

function DetectionContextStack({
  entries,
}: {
  entries: DetectionContextEntry[]
}) {
  if (entries.length === 0) return null

  return (
    <div className="border-b border-[var(--border-subtle)] px-3 py-2">
      <div className="mb-2 flex items-center gap-1.5 text-[0.625rem] font-medium text-muted-foreground uppercase">
        <RadarIcon className="size-3" />
        Context stack
      </div>
      <div className="flex flex-wrap gap-2">
        {entries.map((entry, index) => (
          <div
            key={entry.key}
            className="rounded border border-[var(--border-subtle)] bg-[var(--shell-bg-sunken)] px-2 py-1"
          >
            <div className="flex items-center gap-1.5">
              <span className="text-[0.5625rem] font-medium text-muted-foreground uppercase">
                {index === 0 ? "Current" : "Recent"}
              </span>
              <SourceBadge source={entry.source} />
            </div>
            <div className="mt-0.5 text-xs font-semibold text-foreground">
              {entry.reference}
            </div>
            <div className="text-[0.625rem] text-muted-foreground">
              {entry.detail} / {Math.round(entry.confidence * 100)}%
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

function HeldReferencesPanel({
  candidates,
}: {
  candidates: HeldReferenceCandidate[]
}) {
  if (candidates.length === 0) return null

  return (
    <div className="border-b border-[var(--border-subtle)]">
      <div className="flex items-center justify-between px-3 py-2">
        <div className="flex items-center gap-1.5 text-[0.625rem] font-medium text-muted-foreground uppercase">
          <EyeIcon className="size-3" />
          Held references
        </div>
        <span className="rounded border border-[var(--border-subtle)] px-1.5 py-0.5 text-[0.5625rem] text-muted-foreground">
          {candidates.length}
        </span>
      </div>
      {candidates.map(({ detection, reason }, index) => (
        <div key={`${detection.verse_ref}-${index}`}>
          <div className="mx-3 rounded-t border border-b-0 border-amber-500/30 bg-amber-500/10 px-2 py-1 text-[0.5625rem] font-medium text-amber-700 uppercase dark:text-amber-300">
            {reason}
          </div>
          <DetectionCard detection={detection} />
        </div>
      ))}
    </div>
  )
}

export function DetectionsPanel({ className }: { className?: string }) {
  const { detections } = useDetection()
  const confidenceThreshold = useSettingsStore((s) => s.confidenceThreshold)
  const semanticDetectionEnabled = useSettingsStore(
    (s) => s.semanticDetectionEnabled
  )
  const semanticConfidenceThreshold = useSettingsStore(
    (s) => s.semanticConfidenceThreshold
  )
  const [semanticStatus, setSemanticStatus] = useState<{
    has_semantic: boolean
    paraphrase_enabled: boolean
    semantic_detection_enabled: boolean
  } | null>(null)

  useEffect(() => {
    let cancelled = false
    detectionActions
      .getDetectionStatus()
      .then((status) => {
        if (!cancelled) {
          setSemanticStatus({
            has_semantic: status.has_semantic,
            paraphrase_enabled: status.paraphrase_enabled,
            semantic_detection_enabled: status.semantic_detection_enabled,
          })
        }
      })
      .catch((e) => console.error("[detections] status fetch failed", e))
    return () => {
      cancelled = true
    }
  }, [])

  const contextStack = buildDetectionContextStack(detections)
  const heldReferences = buildHeldReferenceCandidates(
    detections,
    confidenceThreshold,
    semanticConfidenceThreshold
  )
  const heldSet = new Set(
    heldReferences.map((candidate) => candidate.detection)
  )
  const trustedDetections = detections.filter(
    (detection) => !heldSet.has(detection)
  )

  return (
    <div
      data-slot="detections-panel"
      className={cn(
        "glass-panel relative flex min-h-0 flex-col overflow-hidden",
        className
      )}
    >
      <PanelHeader
        title="Recent detections"
        icon={<RadarIcon className="size-3" />}
        step={6}
      >
        <div className="flex flex-wrap items-center justify-end gap-2">
          <span
            className="inline-flex items-center gap-1 rounded border border-[var(--border-subtle)] px-1.5 py-0.5 text-[0.5625rem] text-muted-foreground uppercase"
            title="Semantic detection controls visible suggestions; Auto-live controls automatic output."
          >
            <BrainCircuitIcon className="size-2.5" />
            {semanticDetectionEnabled && semanticStatus?.has_semantic
              ? "Semantic"
              : "Direct"}
            {semanticDetectionEnabled && semanticStatus?.paraphrase_enabled
              ? " + paraphrase"
              : ""}
            {semanticDetectionEnabled
              ? ` semantic ${Math.round(semanticConfidenceThreshold * 100)}%`
              : " semantic off"}
            {" / "}auto {Math.round(confidenceThreshold * 100)}%
          </span>
          <button
            onClick={() => detectionActions.clearDetections()}
            className="text-[0.625rem] text-muted-foreground transition-colors hover:text-foreground"
          >
            Clear all
          </button>
        </div>
      </PanelHeader>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <DetectionContextStack entries={contextStack} />
        <HeldReferencesPanel candidates={heldReferences} />
        <div className="flex flex-col gap-0">
          {detections.length === 0 && (
            <div className="flex min-h-0 flex-1 items-center justify-center">
              <PanelEmptyState
                icon={<RadarIcon className="size-8" />}
                title="No detections yet"
                description="Verse detections will appear here during live transcription."
              />
            </div>
          )}
          {heldReferences.length > 0 && trustedDetections.length > 0 && (
            <div className="border-b border-[var(--border-subtle)] px-3 py-2 text-[0.625rem] font-medium text-muted-foreground uppercase">
              Recent trusted detections
            </div>
          )}
          {trustedDetections.map((detection, i) => (
            <DetectionCard
              key={`${detection.verse_ref}-${i}`}
              detection={detection}
            />
          ))}
        </div>
      </div>
    </div>
  )
}
