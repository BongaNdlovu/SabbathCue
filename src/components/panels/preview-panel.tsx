import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { CanvasPresentation } from "@/components/ui/canvas-verse"
import { PanelHeader } from "@/components/ui/panel-header"
import { PanelEmptyState } from "@/components/ui/panel-empty-state"
import { bibleActions } from "@/hooks/use-bible"
import {
  commitPreviewToLive,
  selectPreviewItem,
  selectPreviewVerse,
} from "@/lib/presentation-workflow"
import { useBibleStore } from "@/stores/bible-store"
import {
  getBroadcastLiveStore,
  useBroadcastLiveStore,
} from "@/stores/broadcast/live-store"
import { selectActiveTheme, useBroadcastStore } from "@/stores/broadcast-store"
import { useEgwSlideStore } from "@/stores/egw-slide-store"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import { useSermonSlideStore } from "@/stores/sermon-slide-store"
import { PresentationDeckControls } from "@/components/panels/presentation-deck-controls"
import { PresentationArrowControls } from "@/components/panels/presentation-arrow-controls"
import { PreviewQuickSearch } from "@/components/panels/preview-quick-search"
import { VideoOverlayControls } from "@/components/broadcast/VideoOverlayControls"
import {
  presentationDeckKind,
  type PresentationDeckKind,
} from "@/lib/presentation-deck-navigation"
import { handlePresentationPanelArrowKey } from "@/lib/presentation-panel-navigation"
import { MonitorIcon, SendIcon, XIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import { convertTauriFileSrc } from "@/lib/tauri-runtime"
import type { PresentationRenderData } from "@/types"

function previewVideoSrc(item: PresentationRenderData): string | null {
  const video = item.video
  if (!video) return null
  if (video.source === "local" && video.videoPath) return convertTauriFileSrc(video.videoPath)
  if (video.source === "url" && video.url) return video.url
  if (video.source === "youtube" && video.youtubeId) {
    return `https://www.youtube-nocookie.com/embed/${video.youtubeId}?controls=1`
  }
  return null
}

export function PreviewPanel({ className }: { className?: string }) {
  const activeTranslationId = useBibleStore((s) => s.activeTranslationId)
  const previewItem = useBroadcastLiveStore((s) => s.previewItem)
  const activeTheme = useBroadcastStore(selectActiveTheme)
  const isLive = useBroadcastLiveStore((s) => s.isLive)
  const readingModeAutoLive = useBroadcastLiveStore(
    (s) => s.readingModeAutoLive
  )
  // The preview video is a local staging copy: its overlay controls act on
  // the element directly and never touch the live transport.
  const previewVideoWrapRef = useRef<HTMLDivElement>(null)
  const previewVideoRef = useRef<HTMLVideoElement | null>(null)
  const previewVideoCleanupRef = useRef<(() => void) | null>(null)
  const [previewVideoPaused, setPreviewVideoPaused] = useState(true)
  const attachPreviewVideo = useCallback((el: HTMLVideoElement | null) => {
    previewVideoCleanupRef.current?.()
    previewVideoCleanupRef.current = null
    previewVideoRef.current = el
    setPreviewVideoPaused(el ? el.paused : true)
    if (!el) return
    const update = () => setPreviewVideoPaused(el.paused)
    el.addEventListener("play", update)
    el.addEventListener("pause", update)
    previewVideoCleanupRef.current = () => {
      el.removeEventListener("play", update)
      el.removeEventListener("pause", update)
    }
  }, [])

  const togglePreviewVideo = () => {
    const el = previewVideoRef.current
    if (!el) return
    if (el.paused) {
      if (el.ended) el.currentTime = 0
      void el
        .play()
        .catch((error) => console.debug("[preview] video play failed", error))
    } else {
      el.pause()
    }
  }

  // Refetch the staged verse only when the translation actually changed —
  // the effect previously ran on every mount, so switching workspaces back
  // to Live Desk refired the fetch and overwrote a preview staged by other
  // flows with the (possibly drifted) selectedVerse.
  const lastTranslationIdRef = useRef(activeTranslationId)
  useEffect(() => {
    if (lastTranslationIdRef.current === activeTranslationId) return
    lastTranslationIdRef.current = activeTranslationId
    let cancelled = false
    if (getBroadcastLiveStore().previewItem?.kind !== "scripture") return
    const verse = useBibleStore.getState().selectedVerse
    if (verse && verse.book_number > 0 && verse.chapter > 0 && verse.verse > 0) {
      bibleActions
        .fetchVerse(verse.book_number, verse.chapter, verse.verse)
        .then((v) => {
          if (!cancelled && v) selectPreviewVerse(v)
        })
        .catch((e) => console.error("[preview] verse refetch on translation change failed", e))
    }
    return () => {
      cancelled = true
    }
  }, [activeTranslationId])

  const clearPreviewBlocked = isLive && readingModeAutoLive

  const clearPreview = () => {
    if (clearPreviewBlocked) return
    getBroadcastLiveStore().setPreviewItem(null)
    useBibleStore.getState().selectVerse(null)
  }

  const navigatePreviewDeck = (kind: PresentationDeckKind, index: number) => {
    if (kind === "hymn") {
      const hymnSlides = useHymnSlideStore.getState()
      const next = hymnSlides.deck[index]
      if (!next) return
      hymnSlides.setDeck(hymnSlides.deck, index)
      selectPreviewItem(next)
      return
    }
    if (kind === "egw") {
      const egwSlides = useEgwSlideStore.getState()
      const next = egwSlides.deck[index]
      if (!next) return
      egwSlides.setDeck(egwSlides.deck, index)
      selectPreviewItem(next)
      return
    }
    const sermonSlides = useSermonSlideStore.getState()
    const next = sermonSlides.deck[index]
    if (!next) return
    sermonSlides.setDeck(sermonSlides.deck, index, sermonSlides.activeItemId)
    selectPreviewItem(next)
  }

  const handlePanelKeyDown = (event: KeyboardEvent<HTMLDivElement>) =>
    handlePresentationPanelArrowKey(event, () => ({
      item: getBroadcastLiveStore().previewItem,
      isLive: false,
    }))

  return (
    <div
      data-slot="preview-panel"
      data-preview-ready={previewItem ? "true" : "false"}
      tabIndex={0}
      onKeyDown={handlePanelKeyDown}
      className={cn(
        "glass-panel relative flex min-h-0 flex-col overflow-hidden outline-none",
        className,
      )}
    >
      <PanelHeader title="Program preview" icon={<MonitorIcon className="size-3" />} step={2}>
        <Badge variant="outline" className="h-5 text-[0.5625rem] uppercase">
          Staged
        </Badge>
      </PanelHeader>

      <div className="flex min-h-12 flex-wrap items-center justify-between gap-3 border-b border-[var(--border-subtle)] px-4 py-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">
            {previewItem?.reference ?? "No item selected"}
          </p>
          <p className="text-xs text-muted-foreground">
            Preview only. Verses, hymns, and songs change audience output when sent live.
          </p>
        </div>

        <PreviewQuickSearch />

        <div className="flex shrink-0 flex-wrap items-center gap-2">
          {presentationDeckKind(previewItem) ? (
            <PresentationDeckControls
              item={previewItem}
              onNavigate={navigatePreviewDeck}
              isLive={false}
              crossQueueBoundaries
            />
          ) : previewItem?.kind === "scripture" ? (
            <PresentationArrowControls item={previewItem} isLive={false} />
          ) : null}
          <Button
            size="sm"
            variant="outline"
            disabled={!previewItem || clearPreviewBlocked}
            className="gap-2"
            onClick={clearPreview}
            title={
              clearPreviewBlocked
                ? "Turn off Auto-live reading mode or hide Live Output before clearing preview"
                : "Clear preview"
            }
          >
            <XIcon className="size-3.5" />
            Clear
          </Button>
          <Button
            size="sm"
            disabled={!previewItem}
            className="gap-2"
            onClick={() => commitPreviewToLive()}
          >
            <SendIcon className="size-3.5" />
            Send Live
          </Button>
        </div>
      </div>

      <div
        data-slot="preview-stage"
        className="flex min-h-0 flex-1 items-center justify-center p-4"
      >
        {previewItem?.kind === "video" && previewVideoSrc(previewItem) ? (
          previewItem.video?.source === "youtube" ? (
            <iframe
              title={previewItem.reference}
              src={previewVideoSrc(previewItem) ?? undefined}
              allow="autoplay; encrypted-media; picture-in-picture"
              allowFullScreen
              className="aspect-video max-h-full w-full rounded-md border border-[var(--border-subtle)] bg-black"
            />
          ) : (
            <div
              ref={previewVideoWrapRef}
              className="relative aspect-video max-h-full w-full rounded-md border border-[var(--border-subtle)] bg-black"
            >
              <video
                ref={attachPreviewVideo}
                src={previewVideoSrc(previewItem) ?? undefined}
                poster={previewItem.video?.poster}
                className="h-full w-full object-contain"
              />
              <VideoOverlayControls
                paused={previewVideoPaused}
                onTogglePlay={togglePreviewVideo}
                fullscreenTarget={() => previewVideoWrapRef.current}
              />
            </div>
          )
        ) : previewItem && activeTheme ? (
          <CanvasPresentation theme={activeTheme} item={previewItem} />
        ) : (
          <PanelEmptyState
            icon={<MonitorIcon className="size-8" />}
            title="No item selected"
            description="Detected verses, searched passages, hymns, and song slides appear here before going live."
          />
        )}
      </div>
    </div>
  )
}
