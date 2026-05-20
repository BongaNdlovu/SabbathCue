import { useMemo, useRef, useState, useEffect } from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { CanvasVerse } from "@/components/ui/canvas-verse"
import { PanelHeader } from "@/components/ui/panel-header"
import { Switch } from "@/components/ui/switch"
import { commitPreviewToLive } from "@/lib/presentation-workflow"
import { cn } from "@/lib/utils"
import { useBibleStore } from "@/stores/bible-store"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { EyeIcon, EyeOffIcon, RadioIcon, SendIcon, Maximize2Icon, Minimize2Icon } from "lucide-react"
import { toast } from "sonner"

export function LiveOutputPanel() {
  const isLive = useBroadcastStore((s) => s.isLive)
  const liveVerse = useBroadcastStore((s) => s.liveVerse)
  const readingModeAutoLive = useBroadcastStore((s) => s.readingModeAutoLive)
  const themes = useBroadcastStore((s) => s.themes)
  const activeThemeId = useBroadcastStore((s) => s.activeThemeId)
  const selectedVerse = useBibleStore((s) => s.selectedVerse)
  const [isFullscreen, setIsFullscreen] = useState(false)
  const panelRef = useRef<HTMLDivElement>(null)

  const activeTheme = useMemo(
    () => themes.find((t) => t.id === activeThemeId) ?? themes[0],
    [themes, activeThemeId],
  )

  const visibleVerse = useMemo(
    () => (isLive ? liveVerse : null),
    [isLive, liveVerse],
  )
  const canCommitPreview = Boolean(selectedVerse)

  const toggleFullscreen = async () => {
    const panel = panelRef.current
    if (!panel) return

    try {
      if (document.fullscreenElement === panel) {
        await document.exitFullscreen()
      } else {
        await panel.requestFullscreen()
      }
    } catch (error) {
      toast.error("Fullscreen failed", {
        description: String(error),
      })
    }
  }

  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(document.fullscreenElement === panelRef.current)
    }

    document.addEventListener("fullscreenchange", handleFullscreenChange)
    return () => {
      document.removeEventListener("fullscreenchange", handleFullscreenChange)
    }
  }, [])

  return (
    <div
      ref={panelRef}
      data-slot="live-output-panel"
      className={cn(
        "flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-card",
        isLive && "shadow-[inset_0_2px_0_0_rgba(16,185,129,0.35)]",
        isFullscreen && "!rounded-none !border-0 !h-screen !w-screen",
      )}
    >
      <PanelHeader title="Live output" icon={<RadioIcon className="size-3" />}>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="xs"
            className="h-6 gap-1 px-2"
            onClick={toggleFullscreen}
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
              isLive && "bg-emerald-500 text-white hover:bg-emerald-500",
            )}
          >
            {isLive ? "On air" : "Hidden"}
          </Badge>
        </div>
      </PanelHeader>

      <div className="flex min-h-10 items-center justify-between gap-2 border-b border-border px-3 py-1.5">
        <Button
          size="sm"
          disabled={!canCommitPreview}
          className="gap-1.5"
          onClick={() => commitPreviewToLive()}
          title={
            canCommitPreview
              ? "Send the Program Preview verse to Live Output"
              : "Select a verse before sending live"
          }
        >
          <SendIcon className="size-3.5" />
          Send Preview Live
        </Button>

        <label className="flex items-center gap-2">
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
            onCheckedChange={(checked) =>
              useBroadcastStore.getState().setLive(checked)
            }
            className="data-[state=checked]:bg-emerald-500"
          />
        </label>
      </div>

      <div className="flex min-h-9 items-center justify-between gap-2 border-b border-border px-3 py-1.5">
        <span className="truncate text-xs text-muted-foreground">
          Auto-live reading mode
        </span>
        <Switch
          checked={readingModeAutoLive}
          onCheckedChange={(checked) =>
            useBroadcastStore.getState().setReadingModeAutoLive(checked)
          }
          className="data-[state=checked]:bg-emerald-500"
        />
      </div>

      <div
        className={cn(
          "flex min-h-0 flex-1 items-center justify-center p-3 transition-opacity",
          !isLive && "opacity-45",
        )}
      >
        <CanvasVerse theme={activeTheme} verse={visibleVerse} />
      </div>

      <div className="truncate border-t border-border px-3 py-1.5 text-xs text-muted-foreground">
        {liveVerse
          ? liveVerse.reference
          : "Nothing has been sent to the live output yet."}
      </div>
    </div>
  )
}
