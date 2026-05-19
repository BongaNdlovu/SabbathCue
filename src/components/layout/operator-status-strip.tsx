import { Badge } from "@/components/ui/badge"
import { LevelMeter } from "@/components/ui/level-meter"
import { cn } from "@/lib/utils"
import {
  useAudioStore,
  useBroadcastStore,
  useQueueStore,
  useTranscriptStore,
} from "@/stores"
import { MicIcon, RadioIcon, Rows3Icon, SwatchBookIcon } from "lucide-react"

export function OperatorStatusStrip() {
  const audioLevel = useAudioStore((s) => s.level)
  const isTranscribing = useTranscriptStore((s) => s.isTranscribing)
  const isLive = useBroadcastStore((s) => s.isLive)
  const liveVerse = useBroadcastStore((s) => s.liveVerse)
  const queueLength = useQueueStore((s) => s.items.length)
  const themes = useBroadcastStore((s) => s.themes)
  const activeThemeId = useBroadcastStore((s) => s.activeThemeId)
  const activeTheme = themes.find((t) => t.id === activeThemeId)

  return (
    <div className="flex h-8 items-center gap-3 border-b border-border bg-card/80 px-3 text-xs text-muted-foreground">
      <div className="flex items-center gap-2">
        <MicIcon className="size-3.5" />
        <LevelMeter level={audioLevel.rms} bars={5} />
        <span>{isTranscribing ? "Listening" : "Idle"}</span>
      </div>

      <div className="flex items-center gap-2">
        <RadioIcon
          className={cn("size-3.5", isLive && "text-emerald-500")}
        />
        <Badge
          variant={isLive ? "default" : "outline"}
          className={cn(
            "h-5 text-[0.5625rem] uppercase",
            isLive && "bg-emerald-500 text-white hover:bg-emerald-500",
          )}
        >
          {isLive ? "On air" : "Hidden"}
        </Badge>
        <span className="max-w-[280px] truncate">
          {liveVerse?.reference ?? "No live verse"}
        </span>
      </div>

      <div className="flex items-center gap-1.5">
        <Rows3Icon className="size-3.5" />
        <span>{queueLength} queued</span>
      </div>

      <div className="ml-auto flex min-w-0 items-center gap-1.5">
        <SwatchBookIcon className="size-3.5" />
        <span className="truncate">{activeTheme?.name ?? "No theme"}</span>
      </div>
    </div>
  )
}
