import { useEffect, useState } from "react"
import { PanelHeader } from "@/components/ui/panel-header"
import { PanelEmptyState } from "@/components/ui/panel-empty-state"
import { ConfidenceDot } from "@/components/ui/confidence-dot"
import { Button } from "@/components/ui/button"
import { BrainCircuitIcon, EyeIcon, PlayIcon, PlusIcon, RadarIcon } from "lucide-react"
import { useDetection, detectionActions } from "@/hooks/use-detection"
import { useSettingsStore } from "@/stores/settings-store"
import { useQueueStore } from "@/stores/queue-store"
import {
  detectionToVerse,
  presentVerse,
  selectPreviewVerse,
  createScriptureQueueItem,
} from "@/lib/presentation-workflow"
import type { DetectionResult } from "@/types"

const SOURCE_COLORS: Record<string, { bg: string; text: string; label: string }> = {
  direct: { bg: "bg-green-500/15", text: "text-green-600", label: "Direct" },
  semantic: { bg: "bg-indigo-500/15", text: "text-indigo-300", label: "Semantic" },
}

function SourceBadge({ source }: { source: string }) {
  const style = SOURCE_COLORS[source] ?? { bg: "bg-muted", text: "text-muted-foreground", label: source }
  return (
    <span className={`rounded px-1.5 py-0.5 text-[0.5625rem] font-medium uppercase tracking-wider ${style.bg} ${style.text}`}>
      {style.label}
    </span>
  )
}

function DetectionCard({ detection }: { detection: DetectionResult }) {
  const verse = detectionToVerse(detection)

  const handlePreview = () => {
    selectPreviewVerse(verse, { navigate: true })
  }

  const handlePresent = () => {
    presentVerse(verse, { navigate: true })
  }

  return (
    <div className="border-b border-border p-3 last:border-0">
      <div className="flex items-center gap-2">
        <ConfidenceDot confidence={detection.confidence} />
        <span className="text-xs font-medium text-muted-foreground">
          {Math.round(detection.confidence * 100)}%
        </span>
        <SourceBadge source={detection.source} />
        <span className="text-sm font-semibold text-foreground">
          {detection.verse_ref}
        </span>
      </div>

      {detection.verse_text && (
        <p className="mt-1 line-clamp-2 text-sm leading-relaxed text-muted-foreground">
          {detection.verse_text}
        </p>
      )}

      <div className="mt-2 flex gap-2">
        <Button size="sm" variant="outline" className="gap-1" onClick={handlePreview}>
          <EyeIcon className="size-3" />
          Preview
        </Button>
        <Button size="sm" className="gap-1" onClick={handlePresent}>
          <PlayIcon className="size-3" />
          Present
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="gap-1"
          onClick={() => {
            useQueueStore.getState().addOrFlashItem(
              createScriptureQueueItem(verse, {
                reference: detection.verse_ref,
                confidence: detection.confidence,
                source: detection.source === "direct" ? "ai-direct" : "ai-semantic",
              })
            )
          }}
        >
          <PlusIcon className="size-3" />
          Queue
        </Button>
      </div>
    </div>
  )
}

export function DetectionsPanel() {
  const { detections } = useDetection()
  const confidenceThreshold = useSettingsStore((s) => s.confidenceThreshold)
  const [semanticStatus, setSemanticStatus] = useState<{
    has_semantic: boolean
    paraphrase_enabled: boolean
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
          })
        }
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <div
      data-slot="detections-panel"
      className="flex h-full flex-col overflow-hidden rounded-lg border border-border bg-card"
    >
      <PanelHeader title="Recent detections" icon={<RadarIcon className="size-3" />} step={6}>
        <div className="flex items-center gap-2">
          <span
            className="inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-[0.5625rem] uppercase text-muted-foreground"
            title="Semantic detections remain visible from 42%; the threshold controls automatic output only."
          >
            <BrainCircuitIcon className="size-2.5" />
            {semanticStatus?.has_semantic ? "Semantic" : "Keyword"}
            {semanticStatus?.paraphrase_enabled ? " + paraphrase" : ""}
            {" "}
            auto {Math.round(confidenceThreshold * 100)}%
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
          {detections.map((detection, i) => (
            <DetectionCard key={`${detection.verse_ref}-${i}`} detection={detection} />
          ))}
        </div>
      </div>
    </div>
  )
}
