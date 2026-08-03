import {
  useMemo,
  useRef,
  useState,
  useEffect,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { CanvasPresentation } from "@/components/ui/canvas-verse"
import { VideoControlBar } from "@/components/broadcast/VideoControlBar"
import { VideoOverlayControls } from "@/components/broadcast/VideoOverlayControls"
import { EmergencyLiveButton } from "@/components/queue/EmergencyLiveButton"
import { PanelHeader } from "@/components/ui/panel-header"
import { PanelEmptyState } from "@/components/ui/panel-empty-state"
import { SegmentedControl } from "@/components/ui/segmented-control"
import { Switch } from "@/components/ui/switch"
import {
  applyPanelFullscreen,
  tauriWindowFullscreen,
} from "@/components/panels/live-output-panel-fullscreen"
import { commitPreviewToLive } from "@/lib/presentation-workflow"
import { cn } from "@/lib/utils"
import { convertTauriFileSrc } from "@/lib/tauri-runtime"
import { useBroadcastVideo } from "@/hooks/use-broadcast-video"
import { useYoutubeEmbedUrl } from "@/hooks/use-broadcast-youtube"
import {
  getBroadcastLiveStore,
  useBroadcastLiveStore,
  type BroadcastLiveItem,
} from "@/stores/broadcast/live-store"
import { selectActiveTheme, useBroadcastStore } from "@/stores/broadcast-store"
import { useBroadcastVideoStore } from "@/stores/broadcast/video-store"
import { PresentationDeckControls } from "@/components/panels/presentation-deck-controls"
import { PresentationArrowControls } from "@/components/panels/presentation-arrow-controls"
import { presentationDeckKind } from "@/lib/presentation-deck-navigation"
import { handlePresentationPanelArrowKey } from "@/lib/presentation-panel-navigation"
import {
  EyeIcon,
  EyeOffIcon,
  RadioIcon,
  SendIcon,
  Maximize2Icon,
  Minimize2Icon,
} from "lucide-react"
import { toast } from "sonner"
import type { BroadcastTheme, BroadcastTransitionType } from "@/types"

const LIVE_TRANSITION_OPTIONS: {
  value: BroadcastTransitionType
  label: string
}[] = [
  { value: "none", label: "Cut" },
  { value: "fade", label: "Fade" },
  { value: "slide", label: "Slide" },
  { value: "scale", label: "Scale" },
]

/// CSS animation class for the in-app live preview, matching the selected
/// transition. "none" (Cut) is instant, so no class.
export function liveTransitionClass(type: BroadcastTransitionType): string {
  switch (type) {
    case "fade":
      return "live-anim-fade"
    case "slide":
      return "live-anim-slide"
    case "scale":
      return "live-anim-scale"
    default:
      return ""
  }
}

function liveVideoSrc(item: BroadcastLiveItem): string | null {
  const video = item?.video
  if (!video) return null
  if (video.source === "local" && video.videoPath) {
    return convertTauriFileSrc(video.videoPath)
  }
  if (video.source === "url" && video.url) return video.url
  return null
}

// The live-screen video is a monitor of the live output: playback belongs to
// the VideoControlBar transport, so a remount (workspace switch) restores the
// last known transport state instead of autoplaying from the top.
function getLiveVideoResumeState() {
  return useBroadcastVideoStore.getState().videoTransport
}

function liveYoutubeId(item: BroadcastLiveItem): string | undefined {
  const video = item?.video
  return video?.source === "youtube" ? video.youtubeId : undefined
}

function LiveVideoMedia({
  item,
  youtubeSrc,
  videoSrc,
  setVideoElement,
}: {
  item: NonNullable<BroadcastLiveItem>
  youtubeSrc: string | null
  videoSrc: string | null
  setVideoElement: (element: HTMLVideoElement | null) => void
}) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const paused = useBroadcastVideoStore(
    (s) => s.videoTransport?.paused ?? false
  )
  if (item.video?.source === "youtube") {
    if (!youtubeSrc) return null
    return (
      <iframe
        title={item.reference}
        src={youtubeSrc}
        allow="autoplay; encrypted-media; picture-in-picture"
        allowFullScreen
        className="aspect-video max-h-full w-full rounded-md border border-[var(--border-subtle)] bg-black"
      />
    )
  }
  if (!videoSrc) return null
  return (
    <div
      ref={wrapRef}
      className="relative aspect-video max-h-full w-full rounded-md border border-[var(--border-subtle)] bg-black"
    >
      <video
        key={videoSrc}
        ref={setVideoElement}
        src={videoSrc}
        poster={item.video?.poster}
        className="h-full w-full object-contain"
      />
      <VideoOverlayControls
        paused={paused}
        onTogglePlay={() =>
          useBroadcastVideoStore.getState().sendVideoCommand({
            type: paused ? "play" : "pause",
          })
        }
        fullscreenTarget={() => wrapRef.current}
      />
    </div>
  )
}

function LiveHeaderActions({
  isLive,
  isFullscreen,
  onToggleFullscreen,
}: {
  isLive: boolean
  isFullscreen: boolean
  onToggleFullscreen: () => void
}) {
  return (
    <div className="flex items-center gap-2">
      <Button
        variant="ghost"
        size="xs"
        className="h-6 gap-1 px-2"
        aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
        onClick={onToggleFullscreen}
      >
        {isFullscreen ? (
          <Minimize2Icon className="size-3.5" />
        ) : (
          <Maximize2Icon className="size-3.5" />
        )}
      </Button>
      <Badge
        variant={isLive ? "default" : "outline"}
        className={cn(
          "h-5 text-[0.5625rem] uppercase",
          isLive && "bg-emerald-500 text-foreground hover:bg-emerald-500"
        )}
      >
        {isLive ? "On air" : "Hidden"}
      </Badge>
    </div>
  )
}

function LiveSendControls({
  isLive,
  liveItem,
  canCommitPreview,
  liveTransitionType,
}: {
  isLive: boolean
  liveItem: BroadcastLiveItem
  canCommitPreview: boolean
  liveTransitionType: BroadcastTransitionType
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <SegmentedControl
        aria-label="Live transition"
        value={liveTransitionType}
        options={LIVE_TRANSITION_OPTIONS}
        onChange={(type) => getBroadcastLiveStore().setLiveTransitionType(type)}
        className="[&_button]:px-2"
      />
      <EmergencyLiveButton />
      <Button
        size="sm"
        disabled={!canCommitPreview}
        className="gap-2"
        onClick={() => commitPreviewToLive()}
        title={
          canCommitPreview
            ? "Send the Program Preview item to Live Output"
            : "Select a verse, hymn, or song before sending live"
        }
      >
        <SendIcon className="size-3.5" />
        Send Preview Live
      </Button>
      {isLive && presentationDeckKind(liveItem) ? (
        <PresentationDeckControls item={liveItem} isLive crossQueueBoundaries />
      ) : isLive && liveItem?.kind === "scripture" ? (
        <PresentationArrowControls item={liveItem} isLive />
      ) : null}
    </div>
  )
}

function LiveVisibilitySwitch({ isLive }: { isLive: boolean }) {
  return (
    <label className="flex items-center gap-2.5">
      {isLive ? (
        <EyeIcon className="size-3.5 text-emerald-500" />
      ) : (
        <EyeOffIcon className="size-3.5 text-muted-foreground" />
      )}
      <span className="text-xs text-muted-foreground">
        {isLive ? "Visible" : "Hidden"}
      </span>
      <Switch
        checked={isLive}
        onCheckedChange={(checked) => getBroadcastLiveStore().setLive(checked)}
        className="data-[state=checked]:bg-emerald-500"
      />
    </label>
  )
}

function ReadingModeRow({
  hidden,
  readingModeAutoLive,
}: {
  hidden: boolean
  readingModeAutoLive: boolean
}) {
  return (
    <div
      className={cn(
        "flex min-h-10 items-center justify-between gap-3 border-b border-[var(--border-subtle)] px-4 py-2",
        hidden && "hidden"
      )}
    >
      <span className="truncate text-xs text-muted-foreground">
        Auto Live output
      </span>
      <Switch
        checked={readingModeAutoLive}
        onCheckedChange={(checked) =>
          getBroadcastLiveStore().setReadingModeAutoLive(checked)
        }
        className="data-[state=checked]:bg-emerald-500"
      />
    </div>
  )
}

function LiveStage({
  isLive,
  activeTheme,
  visibleItem,
  isFullscreenLayout,
}: {
  isLive: boolean
  activeTheme: BroadcastTheme | null
  visibleItem: BroadcastLiveItem
  isFullscreenLayout: boolean
}) {
  const transitionType = useBroadcastLiveStore((s) => s.liveTransitionType)
  const [videoElement, setVideoElement] = useState<HTMLVideoElement | null>(
    null
  )
  // Re-key on content change so the transition animation replays each time a
  // new verse/slide goes live, matching the selected transition.
  const contentKey = visibleItem
    ? `${visibleItem.reference}#${visibleItem.hymnSlide?.slideIndex ?? 0}`
    : "empty"
  // Same chrome-less embed as the audience output, so the in-app live box
  // mirrors what the projector shows instead of offering YouTube controls
  // that only affect the local copy.
  const youtubeSrc = useYoutubeEmbedUrl(liveYoutubeId(visibleItem))
  const videoSrc = liveVideoSrc(visibleItem)
  useBroadcastVideo({
    video: videoElement,
    item: visibleItem?.kind === "video" ? visibleItem : null,
    outputId: "operator",
    getResumeState: getLiveVideoResumeState,
  })
  return (
    <div
      data-slot="live-output-stage"
      className={cn(
        "flex min-h-0 flex-1 bg-[var(--shell-bg-sunken)] p-2",
        isFullscreenLayout && "bg-black p-0",
        !isFullscreenLayout && !isLive && "opacity-45 transition-opacity"
      )}
    >
      <div
        data-slot="live-output-frame"
        className={cn(
          "flex h-full w-full items-center justify-center rounded-md border border-[var(--border-subtle)] p-2 text-center",
          isLive && !isFullscreenLayout && "live-glowing-active",
          isFullscreenLayout && "rounded-none border-0 p-0"
        )}
      >
        {visibleItem && activeTheme ? (
          <div
            key={contentKey}
            data-slot="live-output-anim"
            className={cn(
              "flex h-full w-full items-center justify-center",
              liveTransitionClass(transitionType)
            )}
          >
            {visibleItem.kind === "video" && (youtubeSrc || videoSrc) ? (
              <LiveVideoMedia
                item={visibleItem}
                youtubeSrc={youtubeSrc}
                videoSrc={videoSrc}
                setVideoElement={setVideoElement}
              />
            ) : (
              <CanvasPresentation
                theme={activeTheme}
                item={visibleItem}
                className={
                  isFullscreenLayout ? "[&_canvas]:rounded-none" : undefined
                }
              />
            )}
          </div>
        ) : (
          <PanelEmptyState
            icon={<EyeOffIcon className="size-8" />}
            title="Nothing live"
            description="Send a verse, hymn, or song slide to show audience output."
          />
        )}
      </div>
    </div>
  )
}

export function LiveOutputPanel({ className }: { className?: string }) {
  const isLive = useBroadcastLiveStore((s) => s.isLive)
  const liveItem = useBroadcastLiveStore((s) => s.liveItem)
  const readingModeAutoLive = useBroadcastLiveStore(
    (s) => s.readingModeAutoLive
  )
  const liveTransitionType = useBroadcastLiveStore(
    (s) => s.liveTransitionType
  )
  const previewItem = useBroadcastLiveStore((s) => s.previewItem)
  const [isFullscreen, setIsFullscreen] = useState(false)
  const [isFullscreenLayout, setIsFullscreenLayout] = useState(false)
  const panelRef = useRef<HTMLDivElement>(null)

  const visibleItem = useMemo(
    () => (isLive ? liveItem : null),
    [isLive, liveItem]
  )
  const activeTheme = useBroadcastStore(selectActiveTheme)
  const canCommitPreview = Boolean(previewItem)
  const setPanelFullscreen = async (fullscreen: boolean) => {
    try {
      await applyPanelFullscreen(
        fullscreen,
        { setFullscreen: tauriWindowFullscreen },
        (next) => {
          setIsFullscreen(next)
          setIsFullscreenLayout(next)
        }
      )
    } catch (error) {
      toast.error("Fullscreen failed", {
        description: String(error),
      })
    }
  }

  const toggleFullscreen = () => setPanelFullscreen(!isFullscreen)

  const handlePanelKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) =>
    handlePresentationPanelArrowKey(event, () => {
      const broadcast = getBroadcastLiveStore()
      return { item: broadcast.liveItem, isLive: broadcast.isLive }
    })

  // Window fullscreen has no built-in Escape handling (unlike the HTML5
  // Fullscreen API), so restore it here.
  useEffect(() => {
    if (!isFullscreenLayout) return

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return
      event.preventDefault()
      void setPanelFullscreen(false)
    }

    window.addEventListener("keydown", handleKeyDown)
    return () => {
      window.removeEventListener("keydown", handleKeyDown)
    }
  }, [isFullscreenLayout])

  useEffect(() => {
    if (isFullscreenLayout) {
      document.body.dataset.liveOutputFullscreen = "true"
      return () => {
        delete document.body.dataset.liveOutputFullscreen
      }
    }

    delete document.body.dataset.liveOutputFullscreen
  }, [isFullscreenLayout])

  return (
    <div
      ref={panelRef}
      data-slot="live-output-panel"
      data-live={isLive ? "true" : "false"}
      data-fullscreen-layout={isFullscreenLayout ? "true" : undefined}
      tabIndex={0}
      onKeyDown={handlePanelKeyDown}
      className={cn(
        "glass-panel relative flex min-h-0 flex-col overflow-hidden outline-none",
        isFullscreenLayout &&
          "!fixed !inset-0 !z-[80] !h-screen !w-screen !rounded-none !border-0",
        className
      )}
    >
      <PanelHeader
        title="Live output"
        icon={<RadioIcon className="size-3" />}
        step={3}
        className={cn(isFullscreenLayout && "hidden")}
      >
        <LiveHeaderActions
          isLive={isLive}
          isFullscreen={isFullscreen}
          onToggleFullscreen={toggleFullscreen}
        />
      </PanelHeader>

      <div
        className={cn(
          "flex min-h-12 flex-wrap items-center justify-between gap-3 border-b border-[var(--border-subtle)] px-4 py-2",
          isFullscreenLayout && "hidden"
        )}
      >
        <LiveSendControls
          isLive={isLive}
          liveItem={liveItem}
          canCommitPreview={canCommitPreview}
          liveTransitionType={liveTransitionType}
        />

        <LiveVisibilitySwitch isLive={isLive} />
      </div>

      <ReadingModeRow
        hidden={isFullscreenLayout}
        readingModeAutoLive={readingModeAutoLive}
      />

      {isLive && liveItem?.kind === "video" ? (
        <VideoControlBar item={liveItem} />
      ) : null}

      <LiveStage
        isLive={isLive}
        activeTheme={activeTheme}
        visibleItem={visibleItem}
        isFullscreenLayout={isFullscreenLayout}
      />

      <div
        className={cn(
          "truncate border-t border-[var(--border-subtle)] px-4 py-2 text-xs text-muted-foreground",
          isFullscreenLayout && "hidden"
        )}
      >
        {liveItem
          ? liveItem.reference
          : "Nothing has been sent to the live output yet."}
      </div>
    </div>
  )
}
