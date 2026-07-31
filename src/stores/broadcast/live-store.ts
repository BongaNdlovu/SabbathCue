import {
  useBroadcastStore,
  type BroadcastState,
} from "@/stores/broadcast-store"

export type BroadcastLiveState = Pick<
  BroadcastState,
  | "isLive"
  | "previewItem"
  | "liveItem"
  | "liveQueueItemId"
  | "readingModeAutoLive"
  | "liveTransitionType"
  | "opacity"
  | "setLive"
  | "setPreviewItem"
  | "setLiveItem"
  | "setLiveQueueItemId"
  | "commitLiveItem"
  | "setReadingModeAutoLive"
  | "setLiveTransitionType"
  | "setOpacity"
  | "syncBroadcastOutput"
  | "syncBroadcastOutputFor"
>

export type BroadcastLiveItem = BroadcastLiveState["liveItem"]

type BroadcastLiveHook = {
  <T>(selector: (state: BroadcastLiveState) => T): T
  getState: () => BroadcastLiveState
}

// Reference useBroadcastStore lazily (at call time) rather than capturing it at
// module-init, so this view can't freeze to `undefined` if it is ever evaluated
// while broadcast-store is mid-initialization inside an import cycle. See
// output-issue-store.ts for the full rationale.
export const useBroadcastLiveStore = Object.assign(
  <T>(selector: (state: BroadcastLiveState) => T): T =>
    (useBroadcastStore as unknown as BroadcastLiveHook)(selector),
  {
    getState: (): BroadcastLiveState => useBroadcastStore.getState(),
  }
) as unknown as BroadcastLiveHook

export function getBroadcastLiveStore(): BroadcastLiveState {
  return useBroadcastLiveStore.getState()
}
