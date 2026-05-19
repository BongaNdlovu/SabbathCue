import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { CanvasVerse } from "@/components/ui/canvas-verse"
import { PanelHeader } from "@/components/ui/panel-header"
import { Switch } from "@/components/ui/switch"
import { commitPreviewToLive } from "@/lib/presentation-workflow"
import { cn } from "@/lib/utils"
import { useBibleStore, useBroadcastStore } from "@/stores"
import { EyeIcon, EyeOffIcon, RadioIcon, SendIcon } from "lucide-react"

export function LiveOutputPanel() {
  const isLive = useBroadcastStore((s) => s.isLive)
  const liveVerse = useBroadcastStore((s) => s.liveVerse)
  const themes = useBroadcastStore((s) => s.themes)
  const activeThemeId = useBroadcastStore((s) => s.activeThemeId)
  const selectedVerse = useBibleStore((s) => s.selectedVerse)

  const activeTheme = themes.find((t) => t.id === activeThemeId) ?? themes[0]
  const visibleVerse = isLive ? liveVerse : null
  const canCommitPreview = Boolean(selectedVerse)

  return (
    <div
      data-slot="live-output-panel"
      className={cn(
        "flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-card",
        isLive && "shadow-[inset_0_2px_0_0_rgba(16,185,129,0.35)]",
      )}
    >
      <PanelHeader title="Live output" icon={<RadioIcon className="size-3" />}>
        <Badge
          variant={isLive ? "default" : "outline"}
          className={cn(
            "h-5 text-[0.5625rem] uppercase",
            isLive && "bg-emerald-500 text-white hover:bg-emerald-500",
          )}
        >
          {isLive ? "On air" : "Hidden"}
        </Badge>
      </PanelHeader>

      <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
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

      <div
        className={cn(
          "flex min-h-0 flex-1 items-center justify-center p-3 transition-opacity",
          !isLive && "opacity-45",
        )}
      >
        <CanvasVerse theme={activeTheme} verse={visibleVerse} />
      </div>

      <div className="border-t border-border px-3 py-2 text-xs text-muted-foreground">
        {liveVerse
          ? liveVerse.reference
          : "Nothing has been sent to the live output yet."}
      </div>
    </div>
  )
}
