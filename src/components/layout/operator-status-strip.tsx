import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Badge } from "@/components/ui/badge"
import { LevelMeter } from "@/components/ui/level-meter"
import { cn } from "@/lib/utils"
import { useAudioStore } from "@/stores/audio-store"
import { useBibleStore } from "@/stores/bible-store"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useQueueStore } from "@/stores/queue-store"
import { useTranscriptStore } from "@/stores/transcript-store"
import { detectionActions } from "@/hooks/use-detection"
import { transcriptionActions } from "@/hooks/use-transcription"
import {
  MicIcon,
  RadioIcon,
  Rows3Icon,
  SwatchBookIcon,
  EyeOffIcon,
  StopCircleIcon,
  Trash2Icon,
  XIcon,
  PauseCircleIcon,
  BellOffIcon,
  BellRingIcon,
} from "lucide-react"

export function OperatorStatusStrip() {
  const audioLevel = useAudioStore((s) => s.level)
  const isTranscribing = useTranscriptStore((s) => s.isTranscribing)
  const isLive = useBroadcastStore((s) => s.isLive)
  const liveVerse = useBroadcastStore((s) => s.liveVerse)
  const readingModeAutoLive = useBroadcastStore((s) => s.readingModeAutoLive)
  const queueLength = useQueueStore((s) => s.items.length)
  const themes = useBroadcastStore((s) => s.themes)
  const activeThemeId = useBroadcastStore((s) => s.activeThemeId)
  const activeTheme = themes.find((t) => t.id === activeThemeId)
  const selectedVerse = useBibleStore((s) => s.selectedVerse)

  const [detectionPaused, setDetectionPaused] = useState(false)

  useEffect(() => {
    detectionActions.getDetectionControlStatus().then(
      (status) => setDetectionPaused(status.detection_paused)
    ).catch(() => {})
  }, [])

  const clearLiveOutput = () => {
    useBroadcastStore.getState().setLiveVerse(null)
    useBroadcastStore.getState().setLive(false)
  }

  const clearPreview = () => {
    useBibleStore.getState().selectVerse(null)
  }

  const pauseAutoLive = () => {
    useBroadcastStore.getState().setReadingModeAutoLive(false)
    invoke("stop_reading_mode").catch(() => {})
  }

  const toggleDetectionPaused = () => {
    const next = !detectionPaused
    detectionActions.setDetectionPaused(next).then(() => {
      setDetectionPaused(next)
    }).catch(() => {})
  }

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

      <div className="flex items-center gap-1">
        <button
          onClick={clearLiveOutput}
          disabled={!liveVerse}
          title="Clear Live Output"
          className={cn(
            "flex items-center gap-1 rounded px-1.5 py-0.5 text-[0.6rem] uppercase tracking-wider transition-colors",
            liveVerse
              ? "text-amber-500 hover:bg-amber-500/15 hover:text-amber-400"
              : "cursor-not-allowed text-muted-foreground/30"
          )}
        >
          <Trash2Icon className="size-3" />
          Clear Live
        </button>
        <button
          onClick={clearPreview}
          disabled={!selectedVerse}
          title="Clear Preview"
          className={cn(
            "flex items-center gap-1 rounded px-1.5 py-0.5 text-[0.6rem] uppercase tracking-wider transition-colors",
            selectedVerse
              ? "text-amber-500 hover:bg-amber-500/15 hover:text-amber-400"
              : "cursor-not-allowed text-muted-foreground/30"
          )}
        >
          <XIcon className="size-3" />
          Clear Preview
        </button>
        <button
          onClick={pauseAutoLive}
          disabled={!readingModeAutoLive}
          title="Pause Auto-Live"
          className={cn(
            "flex items-center gap-1 rounded px-1.5 py-0.5 text-[0.6rem] uppercase tracking-wider transition-colors",
            readingModeAutoLive
              ? "text-amber-500 hover:bg-amber-500/15 hover:text-amber-400"
              : "cursor-not-allowed text-muted-foreground/30"
          )}
        >
          <PauseCircleIcon className="size-3" />
          Pause Auto-Live
        </button>
        <button
          onClick={toggleDetectionPaused}
          title={detectionPaused ? "Resume Suggestions" : "Pause Suggestions"}
          className={cn(
            "flex items-center gap-1 rounded px-1.5 py-0.5 text-[0.6rem] uppercase tracking-wider transition-colors",
            detectionPaused
              ? "text-emerald-500 hover:bg-emerald-500/15 hover:text-emerald-400"
              : "text-amber-500 hover:bg-amber-500/15 hover:text-amber-400"
          )}
        >
          {detectionPaused ? (
            <BellRingIcon className="size-3" />
          ) : (
            <BellOffIcon className="size-3" />
          )}
          {detectionPaused ? "Resume Suggestions" : "Pause Suggestions"}
        </button>
      </div>

      <div className="ml-auto flex min-w-0 items-center gap-1.5">
        <button
          onClick={() => useBroadcastStore.getState().setLive(false)}
          disabled={!isLive}
          title="Hide Live Output"
          className={cn(
            "flex items-center gap-1 rounded px-1.5 py-0.5 text-[0.6rem] uppercase tracking-wider transition-colors",
            isLive
              ? "text-red-500 hover:bg-red-500/15 hover:text-red-400"
              : "cursor-not-allowed text-muted-foreground/30"
          )}
        >
          <EyeOffIcon className="size-3" />
          Hide Live Output
        </button>
        <button
          onClick={() => { void transcriptionActions.stop() }}
          disabled={!isTranscribing}
          title="Stop Transcription"
          className={cn(
            "flex items-center gap-1 rounded px-1.5 py-0.5 text-[0.6rem] uppercase tracking-wider transition-colors",
            isTranscribing
              ? "text-red-500 hover:bg-red-500/15 hover:text-red-400"
              : "cursor-not-allowed text-muted-foreground/30"
          )}
        >
          <StopCircleIcon className="size-3" />
          Stop Transcription
        </button>
        <SwatchBookIcon className="size-3.5" />
        <span className="truncate">{activeTheme?.name ?? "No theme"}</span>
      </div>
    </div>
  )
}
