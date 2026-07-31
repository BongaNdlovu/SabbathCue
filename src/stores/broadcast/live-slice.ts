import { type StateCreator } from "zustand"
import { emitTo } from "@tauri-apps/api/event"
import type {
  BroadcastTheme,
  BroadcastTransition,
  BroadcastTransitionType,
  PresentationRenderData,
} from "@/types"
import {
  findThemeById,
  resolveOutputThemeId,
} from "@/stores/broadcast/theme-slice"
import {
  recordWorkflowTrace,
  tracePresentationDetails,
} from "@/lib/workflow-trace"
import { notifyAction } from "@/lib/action-notifications"
import type { BroadcastState } from "@/stores/broadcast-store"

export type BroadcastSyncOptions = { transitionType?: BroadcastTransitionType }

type BroadcastUpdatePayload = {
  theme: BroadcastTheme
  item: PresentationRenderData | null
  opacity: number
  transition?: BroadcastTransition
}

const DEFAULT_TRANSITION_DURATION_MS = 500

function transitionForTheme(
  theme: BroadcastTheme,
  type: BroadcastTransitionType
): BroadcastTransition {
  if (type === "none") {
    return { ...theme.transition, type, duration: 0 }
  }
  const themeDuration = theme.transition?.duration
  const duration =
    themeDuration && themeDuration > 0
      ? themeDuration
      : DEFAULT_TRANSITION_DURATION_MS
  return { ...theme.transition, type, duration }
}

function buildBroadcastPayload(
  state: BroadcastState,
  theme: BroadcastTheme,
  options?: BroadcastSyncOptions
): BroadcastUpdatePayload {
  const payload: BroadcastUpdatePayload = {
    theme,
    item: state.isLive ? state.liveItem : null,
    opacity: state.opacity,
  }
  if (options?.transitionType) {
    payload.transition = transitionForTheme(theme, options.transitionType)
  }
  return payload
}

export interface LiveSlice {
  isLive: boolean
  previewItem: PresentationRenderData | null
  liveItem: PresentationRenderData | null
  /** Queue item identity when the live output was presented from the queue. */
  liveQueueItemId: string | null
  readingModeAutoLive: boolean
  liveTransitionType: BroadcastTransitionType
  opacity: number
  setLive: (live: boolean, options?: BroadcastSyncOptions) => void
  setPreviewItem: (item: PresentationRenderData | null) => void
  setLiveItem: (item: PresentationRenderData | null) => void
  setLiveQueueItemId: (itemId: string | null) => void
  commitLiveItem: (
    item: PresentationRenderData,
    options?: { makeLive?: boolean; transitionType?: BroadcastTransitionType }
  ) => void
  setReadingModeAutoLive: (enabled: boolean) => void
  setLiveTransitionType: (type: BroadcastTransitionType) => void
  setOpacity: (opacity: number) => void
  syncBroadcastOutput: (options?: BroadcastSyncOptions) => void
  syncBroadcastOutputFor: (
    outputId: string,
    options?: BroadcastSyncOptions
  ) => void
}

export const createLiveSlice: StateCreator<
  BroadcastState,
  [],
  [],
  LiveSlice
> = (set, get) => ({
  isLive: false,
  previewItem: null,
  liveItem: null,
  liveQueueItemId: null,
  readingModeAutoLive: true,
  liveTransitionType: "fade",
  opacity: 1,

  syncBroadcastOutputFor: (outputId: string, options) => {
    const s = get()
    const themeId = resolveOutputThemeId(s, outputId)
    const label = outputId === "alt" ? "broadcast-alt" : "broadcast"
    const theme = findThemeById(s.themes, themeId)
    if (!theme) return

    void emitTo(
      label,
      "broadcast:verse-update",
      buildBroadcastPayload(s, theme, options)
    ).then(
      () => {
        get().clearOutputIssueFor(
          outputId === "alt" ? "alt" : "main",
          "broadcast-sync"
        )
      },
      (error) => {
        console.warn(`[broadcast-store] sync emit to '${label}' failed`, error)
        get().reportOutputIssue({
          outputId: outputId === "alt" ? "alt" : "main",
          kind: "broadcast-sync",
          title: "Broadcast sync failed",
          description: `Could not sync live output to ${label}: ${String(error)}`,
        })
      }
    )
  },
  syncBroadcastOutput: (options) => {
    get().syncBroadcastOutputFor("main", options)
    get().syncBroadcastOutputFor("alt", options)
  },
  setLive: (isLive, options) => {
    const shouldStopVideo = !isLive && get().liveItem?.kind === "video"
    set({ isLive, liveQueueItemId: isLive ? get().liveQueueItemId : null })
    recordWorkflowTrace(
      "live.state",
      isLive ? "Live screen shown" : "Live screen hidden",
      {
        isLive,
        live: tracePresentationDetails(get().liveItem),
      }
    )
    get().syncBroadcastOutput(isLive ? options : undefined)
    if (shouldStopVideo) get().sendVideoCommand({ type: "stop" })
    notifyAction(isLive ? "Live screen shown" : "Live screen cleared")
  },
  setPreviewItem: (previewItem) => {
    set({ previewItem })
    recordWorkflowTrace("preview.state", "Preview state updated", {
      preview: tracePresentationDetails(previewItem),
    })
  },
  setLiveItem: (liveItem) => {
    set({ liveItem, liveQueueItemId: null })
    recordWorkflowTrace("live.state", "Live item state updated", {
      isLive: get().isLive,
      live: tracePresentationDetails(liveItem),
    })
    get().syncBroadcastOutput()
  },
  setLiveQueueItemId: (liveQueueItemId) => set({ liveQueueItemId }),
  commitLiveItem: (liveItem, options) => {
    const makeLive = options?.makeLive ?? true
    const previousWasVideo = get().liveItem?.kind === "video"
    const sinkId =
      liveItem.kind === "video" ? get().preferredAudioOutputDeviceId : ""
    if (sinkId) get().sendVideoCommand({ type: "setSinkId", sinkId })
    if (liveItem.kind === "video") {
      set(
        makeLive
          ? {
              liveItem,
              isLive: true,
              videoTransport: null,
              liveQueueItemId: null,
            }
          : { liveItem, videoTransport: null, liveQueueItemId: null }
      )
    } else if (previousWasVideo) {
      set(
        makeLive
          ? {
              liveItem,
              isLive: true,
              videoTransport: null,
              liveQueueItemId: null,
            }
          : { liveItem, videoTransport: null, liveQueueItemId: null }
      )
    } else {
      set(
        makeLive
          ? { liveItem, isLive: true, liveQueueItemId: null }
          : { liveItem, liveQueueItemId: null }
      )
    }
    recordWorkflowTrace("live.state", "Live commit state applied", {
      makeLive,
      isLive: get().isLive,
      live: tracePresentationDetails(get().liveItem),
    })
    if (previousWasVideo && liveItem.kind !== "video") {
      get().sendVideoCommand({ type: "stop" })
    }
    get().syncBroadcastOutput({
      transitionType: options?.transitionType ?? get().liveTransitionType,
    })
    if (liveItem.kind === "video") {
      get().sendVideoCommand({ type: "load", item: liveItem })
    }
    if (makeLive) notifyAction("Sent to live", liveItem.reference)
  },
  setReadingModeAutoLive: (readingModeAutoLive) => {
    set({ readingModeAutoLive })
  },
  setLiveTransitionType: (liveTransitionType) => {
    set({ liveTransitionType })
  },
  setOpacity: (opacity) => {
    const nextOpacity = Number.isFinite(opacity)
      ? Math.max(0, Math.min(1, opacity))
      : 1
    set({ opacity: nextOpacity })
    get().syncBroadcastOutput()
  },
})
