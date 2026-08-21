import { useEffect } from "react"
import { invokeTauri, isTauriRuntime } from "@/lib/tauri-runtime"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useSettingsStore } from "@/stores/settings-store"

export function useDetectionSettingsSync() {
  useEffect(() => {
    if (!isTauriRuntime()) return

    let prev = {
      autoMode: useSettingsStore.getState().autoMode,
      bibleDetectionEnabled:
        useSettingsStore.getState().bibleDetectionEnabled,
      semanticDetectionEnabled:
        useSettingsStore.getState().semanticDetectionEnabled,
      confidenceThreshold: useSettingsStore.getState().confidenceThreshold,
      semanticConfidenceThreshold:
        useSettingsStore.getState().semanticConfidenceThreshold,
      cooldownMs: useSettingsStore.getState().cooldownMs,
      liveOutputEnabled: useBroadcastStore.getState().readingModeAutoLive,
    }

    const sync = (
      next = useSettingsStore.getState(),
      liveOutputEnabled = useBroadcastStore.getState().readingModeAutoLive
    ) => {
      void invokeTauri("update_detection_settings", {
        autoMode: next.autoMode,
        bibleDetectionEnabled: next.bibleDetectionEnabled,
        semanticDetectionEnabled: next.semanticDetectionEnabled,
        confidenceThreshold: next.confidenceThreshold,
        semanticConfidenceThreshold: next.semanticConfidenceThreshold,
        cooldownMs: next.cooldownMs,
        liveOutputEnabled,
      }).catch((e) => {
        console.warn("[detection-settings] sync failed", e)
        useBroadcastStore.getState().reportOutputIssue({
          outputId: "global",
          kind: "detection-settings",
          title: "Detection settings sync failed",
          description: `Could not sync detection settings to backend: ${String(e)}`,
        })
      })
    }

    sync()

    const checkAndSync = () => {
      const state = useSettingsStore.getState()
      const liveOutputEnabled =
        useBroadcastStore.getState().readingModeAutoLive

      if (
        state.autoMode === prev.autoMode &&
        state.bibleDetectionEnabled === prev.bibleDetectionEnabled &&
        state.semanticDetectionEnabled === prev.semanticDetectionEnabled &&
        state.confidenceThreshold === prev.confidenceThreshold &&
        state.semanticConfidenceThreshold ===
          prev.semanticConfidenceThreshold &&
        state.cooldownMs === prev.cooldownMs &&
        liveOutputEnabled === prev.liveOutputEnabled
      ) {
        return
      }

      prev = {
        autoMode: state.autoMode,
        bibleDetectionEnabled: state.bibleDetectionEnabled,
        semanticDetectionEnabled: state.semanticDetectionEnabled,
        confidenceThreshold: state.confidenceThreshold,
        semanticConfidenceThreshold: state.semanticConfidenceThreshold,
        cooldownMs: state.cooldownMs,
        liveOutputEnabled,
      }

      sync(state, liveOutputEnabled)
    }

    const unsubscribeSettings = useSettingsStore.subscribe(checkAndSync)
    const unsubscribeBroadcast = useBroadcastStore.subscribe(checkAndSync)

    return () => {
      unsubscribeSettings()
      unsubscribeBroadcast()
    }
  }, [])
}
