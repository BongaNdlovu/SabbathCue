import { useEffect, useMemo } from "react"
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
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"
import { ChevronLeftIcon, ChevronRightIcon, MonitorIcon, SendIcon, XIcon } from "lucide-react"

export function PreviewPanel() {
  const activeTranslationId = useBibleStore((s) => s.activeTranslationId)
  const previewItem = useBroadcastStore((s) => s.previewItem)
  const themes = useBroadcastStore((s) => s.themes)
  const activeThemeId = useBroadcastStore((s) => s.activeThemeId)
  const isLive = useBroadcastStore((s) => s.isLive)
  const readingModeAutoLive = useBroadcastStore((s) => s.readingModeAutoLive)

  useEffect(() => {
    if (useBroadcastStore.getState().previewItem?.kind !== "scripture") return
    const verse = useBibleStore.getState().selectedVerse
    if (verse && verse.book_number > 0 && verse.chapter > 0 && verse.verse > 0) {
      bibleActions
        .fetchVerse(verse.book_number, verse.chapter, verse.verse)
        .then((v) => {
          if (v) selectPreviewVerse(v)
        })
        .catch(() => {})
    }
  }, [activeTranslationId])

  const activeTheme = useMemo(
    () => themes.find((t) => t.id === activeThemeId) ?? themes[0],
    [themes, activeThemeId],
  )

  const clearPreviewBlocked = isLive && readingModeAutoLive

  const clearPreview = () => {
    if (clearPreviewBlocked) return
    useBroadcastStore.getState().setPreviewItem(null)
    useBibleStore.getState().selectVerse(null)
  }

  const previewDeckIndex = previewItem?.kind === "hymn"
    ? useHymnSlideStore
        .getState()
        .deck.findIndex((item) => item.screenId === previewItem.hymnSlide?.screenId)
    : -1
  const canNavigateHymn = previewDeckIndex >= 0
  const navigateHymnPreview = (delta: number) => {
    const hymnSlides = useHymnSlideStore.getState()
    const currentIndex =
      hymnSlides.deck.findIndex((item) => item.screenId === previewItem?.hymnSlide?.screenId)
    if (currentIndex < 0) return
    const nextIndex = Math.max(0, Math.min(hymnSlides.deck.length - 1, currentIndex + delta))
    const next = hymnSlides.deck[nextIndex]
    if (!next || nextIndex === currentIndex) return
    hymnSlides.setDeck(hymnSlides.deck, nextIndex)
    selectPreviewItem(next)
  }

  return (
    <div
      data-slot="preview-panel"
      className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-card"
    >
      <PanelHeader title="Program preview" icon={<MonitorIcon className="size-3" />} step={2}>
        <Badge variant="outline" className="h-5 text-[0.5625rem] uppercase">
          Staged
        </Badge>
      </PanelHeader>

      <div className="flex min-h-10 items-center justify-between gap-2 border-b border-border px-3 py-1.5">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">
            {previewItem?.reference ?? "No item selected"}
          </p>
          <p className="text-xs text-muted-foreground">
            Preview only. Verses, hymns, and songs change audience output when sent live.
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {previewItem?.kind === "hymn" && (
            <div className="flex items-center gap-1">
              <Button
                size="icon-xs"
                variant="outline"
                disabled={!canNavigateHymn || previewDeckIndex <= 0}
                onClick={() => navigateHymnPreview(-1)}
                title="Previous hymn or song slide"
              >
                <ChevronLeftIcon className="size-3" />
              </Button>
              <span className="min-w-12 text-center text-[0.625rem] tabular-nums text-muted-foreground">
                {(previewItem.hymnSlide?.slideIndex ?? 0) + 1}/{previewItem.hymnSlide?.slideCount ?? 1}
              </span>
              <Button
                size="icon-xs"
                variant="outline"
                disabled={!canNavigateHymn || previewDeckIndex >= useHymnSlideStore.getState().deck.length - 1}
                onClick={() => navigateHymnPreview(1)}
                title="Next hymn or song slide"
              >
                <ChevronRightIcon className="size-3" />
              </Button>
            </div>
          )}
          <Button
            size="sm"
            variant="outline"
            disabled={!previewItem || clearPreviewBlocked}
            className="gap-1.5"
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
            className="gap-1.5"
            onClick={() => commitPreviewToLive()}
          >
            <SendIcon className="size-3.5" />
            Send Live
          </Button>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 items-center justify-center p-3">
        {previewItem ? (
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
