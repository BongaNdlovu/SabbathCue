import { detectionKey } from "@/stores/detection-store"
import { EyeIcon, PlayIcon, PlusIcon, RadarIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { CollectDetectionButton } from "@/components/panels/collect-detection-button"
import { ConfidenceRing } from "@/components/ui/confidence-ring"
import { useDetection, detectionActions } from "@/hooks/use-detection"
import { useDashboardWorkspaceStore } from "@/stores/dashboard-workspace-store"
import {
  getDetectionActions,
  SourceBadge,
} from "@/components/panels/detections-panel"
import type { DetectionResult } from "@/types"

function LiveDetectionRow({ detection }: { detection: DetectionResult }) {
  const actions = getDetectionActions(detection)
  return (
    <div className="flex min-w-0 flex-col gap-2 border-b border-[var(--border-subtle)] px-3 py-2 [contain-intrinsic-size:0_64px] [content-visibility:auto] last:border-0 sm:flex-row sm:items-center sm:gap-3">
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex min-w-0 items-center gap-2">
          <ConfidenceRing confidence={detection.confidence} size="sm" />
          <span className="shrink-0 text-xs font-medium text-muted-foreground">
            {Math.round(detection.confidence * 100)}%
          </span>
          <SourceBadge source={detection.source} />
          <span className="min-w-0 truncate text-sm font-semibold text-foreground">
            {detection.verse_ref}
          </span>
        </div>
        {detection.verse_text ? (
          <p className="line-clamp-1 text-xs leading-relaxed text-muted-foreground">
            {detection.verse_text}
          </p>
        ) : null}
      </div>
      <div className="flex w-full shrink-0 flex-wrap items-center justify-end gap-1.5 sm:w-auto">
        <Button
          variant="outline"
          size="xs"
          className="gap-1.5"
          title={`Preview ${detection.verse_ref}`}
          aria-label={`Preview ${detection.verse_ref}`}
          onClick={actions.preview}
        >
          <EyeIcon className="size-3" />
          Preview
        </Button>
        <Button
          size="xs"
          className="gap-1.5"
          title={`Send ${detection.verse_ref} live`}
          aria-label={`Send ${detection.verse_ref} live`}
          onClick={actions.present}
        >
          <PlayIcon className="size-3" />
          Send Live
        </Button>
        <Button
          variant="ghost"
          size="icon-xs"
          title={`Add ${detection.verse_ref} to queue`}
          aria-label={`Add ${detection.verse_ref} to queue`}
          onClick={actions.queue}
        >
          <PlusIcon className="size-3" />
        </Button>
        <CollectDetectionButton detection={detection} compact />
      </div>
    </div>
  )
}

/**
 * Live Desk detection signal. Action intent is shared with the detection cards
 * via getDetectionActions, so all detection types behave identically here and
 * on the full detections page.
 */
export function LatestDetectionBar({ className }: { className?: string }) {
  const { detections } = useDetection()
  const recent = detections.slice(0, 5)

  return (
    <div
      data-slot="latest-detection-bar"
      className={cn(
        "glass-panel flex min-h-[190px] flex-col overflow-hidden",
        className
      )}
    >
      <div className="flex min-h-10 items-center justify-between gap-3 border-b border-[var(--border-subtle)] px-3 py-2">
        <div className="flex items-center gap-1.5 text-[0.625rem] font-medium text-muted-foreground uppercase">
          <RadarIcon className="size-3" />
          Live detections
          {recent.length > 0 ? (
            <span className="rounded border border-[var(--border-subtle)] px-1.5 py-0.5 text-[0.5625rem]">
              {recent.length}
            </span>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-1">
          {recent.length > 0 ? (
            <Button
              variant="ghost"
              size="xs"
              onClick={() => detectionActions.clearDetections()}
            >
              Clear
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="xs"
            onClick={() =>
              useDashboardWorkspaceStore.getState().setWorkspace("detections")
            }
          >
            Open Detections
          </Button>
        </div>
      </div>

      {recent.length > 0 ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {recent.map((detection) => (
            <LiveDetectionRow
              key={detectionKey(detection)}
              detection={detection}
            />
          ))}
        </div>
      ) : (
        <div className="flex flex-1 items-center px-3 text-xs text-muted-foreground">
          No detections yet
        </div>
      )}
    </div>
  )
}
