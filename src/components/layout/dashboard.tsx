import {
  useCallback,
  useEffect,
  lazy,
  useRef,
  useState,
  Suspense,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { Button } from "@/components/ui/button"
import { TransportBar } from "@/components/controls/transport-bar"
import { OperatorStatusStrip } from "@/components/layout/operator-status-strip"
import { TranscriptPanel } from "@/components/panels/transcript-panel"
import { PreviewPanel } from "@/components/panels/preview-panel"
import { LiveOutputPanel } from "@/components/panels/live-output-panel"
import { QueuePanel } from "@/components/panels/queue-panel"
import { DetectionsPanel } from "@/components/panels/detections-panel"
import { SearchPanel } from "@/components/panels/search-panel"
import { useDashboardKeyboardControls } from "@/hooks/use-dashboard-keyboard-controls"
import {
  clampNumber,
  layoutStateFromPreset,
  loadDashboardLayoutState,
  saveDashboardLayoutState,
  type DashboardViewMode,
} from "@/lib/dashboard-layout"
import { useDashboardWorkspaceStore } from "@/stores/dashboard-workspace-store"
import { useServicePlanStore } from "@/stores/service-plan-store"

const LazyHymnWorkspace = lazy(() =>
  import("@/components/hymnal/HymnWorkspace").then((mod) => ({
    default: mod.HymnWorkspace,
  })),
)

const LazyServicePlanWorkspace = lazy(() =>
  import("@/components/service-plan/ServicePlanPage").then((mod) => ({
    default: mod.ServicePlanWorkspace,
  })),
)

function ResizeHandle({
  axis,
  label,
  onPointerDown,
}: {
  axis: "x" | "y"
  label: string
  onPointerDown: (event: ReactPointerEvent<HTMLDivElement>) => void
}) {
  return (
    <div
      role="separator"
      aria-label={label}
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      title={label}
      onPointerDown={onPointerDown}
      className={
        axis === "x"
          ? "relative cursor-col-resize rounded-sm bg-border/40 transition-colors hover:bg-primary/50 after:absolute after:inset-y-1 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-muted-foreground/40"
          : "relative cursor-row-resize rounded-sm bg-border/40 transition-colors hover:bg-primary/50 after:absolute after:left-1 after:right-1 after:top-1/2 after:h-px after:-translate-y-1/2 after:bg-muted-foreground/40"
      }
    />
  )
}

export function Dashboard() {
  const contentRef = useRef<HTMLDivElement>(null)
  const [windowWidth, setWindowWidth] = useState(() =>
    typeof window === "undefined" ? 1920 : window.innerWidth,
  )
  const workspace = useDashboardWorkspaceStore((s) => s.workspace)
  const setWorkspace = useDashboardWorkspaceStore((s) => s.setWorkspace)
  const plannerOpen = useServicePlanStore((s) => s.plannerOpen)
  const openPlanner = useServicePlanStore((s) => s.openPlanner)
  const closePlanner = useServicePlanStore((s) => s.closePlanner)
  const [layout, setLayout] = useState(loadDashboardLayoutState)
  const isCompact = windowWidth < 1400
  const viewMode = layout.viewMode
  const topHeightPercent = layout.topHeightPercent
  const transcriptWidth = layout.transcriptWidth
  const queueWidth = layout.queueWidth
  const detectionsWidth = layout.detectionsWidth

  useDashboardKeyboardControls()

  useEffect(() => {
    const onResize = () => setWindowWidth(window.innerWidth)
    window.addEventListener("resize", onResize)
    return () => window.removeEventListener("resize", onResize)
  }, [])

  useEffect(() => {
    saveDashboardLayoutState(layout)
  }, [layout])

  useEffect(() => {
    if (plannerOpen && workspace !== "service-plans") {
      setWorkspace("service-plans")
    }
  }, [plannerOpen, setWorkspace, workspace])

  const applyViewMode = (mode: DashboardViewMode) => {
    setLayout(layoutStateFromPreset(mode))
  }

  const resetLayout = () => {
    setLayout(layoutStateFromPreset("balanced"))
  }

  const startTopResize = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    const content = contentRef.current
    if (!content) return

    const rect = content.getBoundingClientRect()
    const onMove = (moveEvent: PointerEvent) => {
      const next = ((moveEvent.clientY - rect.top) / rect.height) * 100
      setLayout((current) => ({
        ...current,
        topHeightPercent: clampNumber(next, 34, 68),
      }))
    }
    const onUp = () => {
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }, [])

  const startTranscriptResize = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = transcriptWidth
    const onMove = (moveEvent: PointerEvent) => {
      setLayout((current) => ({
        ...current,
        transcriptWidth: clampNumber(startWidth + moveEvent.clientX - startX, 240, 520),
      }))
    }
    const onUp = () => {
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }, [transcriptWidth])

  const startQueueResize = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = queueWidth
    const onMove = (moveEvent: PointerEvent) => {
      setLayout((current) => ({
        ...current,
        queueWidth: clampNumber(startWidth - (moveEvent.clientX - startX), 240, 520),
      }))
    }
    const onUp = () => {
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }, [queueWidth])

  const startDetectionsResize = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = detectionsWidth
    const onMove = (moveEvent: PointerEvent) => {
      setLayout((current) => ({
        ...current,
        detectionsWidth: clampNumber(startWidth - (moveEvent.clientX - startX), 360, 760),
      }))
    }
    const onUp = () => {
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }, [detectionsWidth])

  return (
    <div className="fixed inset-0 flex flex-col overflow-hidden bg-background">
      <TransportBar />
      <OperatorStatusStrip />

      <div className="flex items-center gap-1 border-b border-border bg-card/60 px-3 py-1.5">
        <Button
          size="xs"
          variant={workspace === "live" ? "default" : "outline"}
          aria-pressed={workspace === "live"}
          onClick={() => {
            closePlanner()
            setWorkspace("live")
          }}
        >
          Live
        </Button>
        <Button
          size="xs"
          variant={workspace === "service-plans" ? "default" : "outline"}
          aria-pressed={workspace === "service-plans"}
          onClick={() => {
            setWorkspace("service-plans")
            openPlanner()
          }}
        >
          Service Plans
        </Button>
        <Button
          size="xs"
          variant={workspace === "hymns" ? "default" : "outline"}
          aria-pressed={workspace === "hymns"}
          onClick={() => {
            closePlanner()
            setWorkspace("hymns")
          }}
        >
          Hymns
        </Button>

        {workspace === "live" && (
          <>
            <div className="mx-2 h-4 w-px bg-border" />
            {(["balanced", "broadcast", "study"] as const).map((mode) => (
              <Button
                key={mode}
                size="xs"
                variant={viewMode === mode ? "default" : "outline"}
                onClick={() => applyViewMode(mode)}
                className="capitalize"
              >
                {mode}
              </Button>
            ))}
            <span className="ml-2 text-xs text-muted-foreground">
              Drag labeled dividers to resize panels
            </span>
            <Button size="xs" variant="ghost" onClick={resetLayout} className="ml-auto">
              Reset layout
            </Button>
          </>
        )}
      </div>

      {workspace === "service-plans" ? (
        <div className="min-h-0 flex-1 overflow-hidden">
          <Suspense fallback={<div className="h-full rounded-lg border border-border bg-card" />}>
            <LazyServicePlanWorkspace />
          </Suspense>
        </div>
      ) : workspace === "hymns" ? (
        <div className="min-h-0 flex-1 overflow-hidden">
          <Suspense fallback={<div className="h-full rounded-lg border border-border bg-card" />}>
            <LazyHymnWorkspace />
          </Suspense>
        </div>
      ) : (
        <div ref={contentRef} className="flex min-h-0 flex-1 flex-col gap-1.5 p-3">
          <div
            className="grid min-h-0 gap-1.5 *:min-h-0"
            style={{
              height: `${topHeightPercent}%`,
              gridTemplateColumns: isCompact
                ? `minmax(0, 1fr) minmax(0, 1fr)`
                : `${transcriptWidth}px 6px minmax(280px, 1fr) minmax(280px, 1fr) 6px ${queueWidth}px`,
              gridTemplateRows: isCompact ? `minmax(0, 1fr) minmax(0, 0.8fr)` : undefined,
            }}
          >
            <div className={isCompact ? "min-h-0" : "contents"}>
              <TranscriptPanel />
            </div>
            {!isCompact && (
              <ResizeHandle
                axis="x"
                label="Resize transcript panel"
                onPointerDown={startTranscriptResize}
              />
            )}
            <PreviewPanel />
            <LiveOutputPanel />
            {!isCompact && (
              <ResizeHandle
                axis="x"
                label="Resize queue panel"
                onPointerDown={startQueueResize}
              />
            )}
            <div className={isCompact ? "min-h-0" : "contents"}>
              <QueuePanel />
            </div>
          </div>

          <ResizeHandle
            axis="y"
            label="Resize top and bottom dashboard sections"
            onPointerDown={startTopResize}
          />

          <div
            className="grid min-h-0 flex-1 gap-1.5"
            style={{
              gridTemplateColumns: isCompact
                ? "minmax(0, 1fr)"
                : `minmax(0, 1fr) 6px ${detectionsWidth}px`,
              gridTemplateRows: isCompact ? "minmax(0, 1fr) minmax(220px, 0.55fr)" : undefined,
            }}
          >
            <SearchPanel />
            {!isCompact && (
              <ResizeHandle
                axis="x"
                label="Resize recent detections panel"
                onPointerDown={startDetectionsResize}
              />
            )}
            <DetectionsPanel />
          </div>
        </div>
      )}
    </div>
  )
}
