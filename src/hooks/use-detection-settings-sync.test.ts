// @vitest-environment jsdom
import { act } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { createRoot } from "react-dom/client"
import React from "react"

const invokeMock = vi.fn()
const reportOutputIssueMock = vi.fn()

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => true,
  invokeTauri: (...args: unknown[]) => invokeMock(...args),
}))

vi.mock("@/stores/broadcast-store", () => ({
  useBroadcastStore: {
    getState: () => ({
      reportOutputIssue: reportOutputIssueMock,
    }),
  },
}))

describe("useDetectionSettingsSync", () => {
  let container: HTMLDivElement
  let root: ReturnType<typeof createRoot>

  beforeEach(() => {
    invokeMock.mockReset()
    reportOutputIssueMock.mockReset()
    invokeMock.mockResolvedValue(undefined)
    container = document.createElement("div")
    document.body.appendChild(container)
    root = createRoot(container)
  })

  afterEach(() => {
    act(() => {
      root.unmount()
    })
    container.remove()
    vi.resetModules()
  })

  it("reports a detection-settings issue when backend sync fails", async () => {
    invokeMock.mockRejectedValueOnce(new Error("backend offline"))

    const { useSettingsStore } = await import("@/stores/settings-store")
    const { useDetectionSettingsSync } =
      await import("./use-detection-settings-sync")
    useSettingsStore.getState().setSemanticDetectionEnabled(false)

    function Probe() {
      useDetectionSettingsSync()
      return null
    }

    await act(async () => {
      root.render(React.createElement(Probe))
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(invokeMock).toHaveBeenCalledWith("update_detection_settings", {
      autoMode: useSettingsStore.getState().autoMode,
      bibleDetectionEnabled:
        useSettingsStore.getState().bibleDetectionEnabled,
      semanticDetectionEnabled:
        useSettingsStore.getState().semanticDetectionEnabled,
      confidenceThreshold: useSettingsStore.getState().confidenceThreshold,
      semanticConfidenceThreshold:
        useSettingsStore.getState().semanticConfidenceThreshold,
      cooldownMs: useSettingsStore.getState().cooldownMs,
    })
    expect(reportOutputIssueMock).toHaveBeenCalledWith(
      expect.objectContaining({
        outputId: "global",
        kind: "detection-settings",
      })
    )
  })

  it("syncs semantic threshold changes to the backend", async () => {
    const { useSettingsStore } = await import("@/stores/settings-store")
    const { useDetectionSettingsSync } =
      await import("./use-detection-settings-sync")

    function Probe() {
      useDetectionSettingsSync()
      return null
    }

    await act(async () => {
      root.render(React.createElement(Probe))
      await Promise.resolve()
    })

    invokeMock.mockClear()

    await act(async () => {
      useSettingsStore.getState().setSemanticConfidenceThreshold(0.72)
      await Promise.resolve()
    })

    expect(invokeMock).toHaveBeenCalledWith("update_detection_settings", {
      autoMode: useSettingsStore.getState().autoMode,
      bibleDetectionEnabled:
        useSettingsStore.getState().bibleDetectionEnabled,
      semanticDetectionEnabled:
        useSettingsStore.getState().semanticDetectionEnabled,
      confidenceThreshold: useSettingsStore.getState().confidenceThreshold,
      semanticConfidenceThreshold: 0.72,
      cooldownMs: useSettingsStore.getState().cooldownMs,
    })
  })

  it("syncs semantic detection enablement changes to the backend", async () => {
    const { useSettingsStore } = await import("@/stores/settings-store")
    const { useDetectionSettingsSync } =
      await import("./use-detection-settings-sync")
    useSettingsStore.getState().setSemanticDetectionEnabled(false)

    function Probe() {
      useDetectionSettingsSync()
      return null
    }

    await act(async () => {
      root.render(React.createElement(Probe))
      await Promise.resolve()
    })

    invokeMock.mockClear()

    await act(async () => {
      useSettingsStore.getState().setSemanticDetectionEnabled(true)
      await Promise.resolve()
    })

    expect(invokeMock).toHaveBeenCalledWith("update_detection_settings", {
      autoMode: useSettingsStore.getState().autoMode,
      bibleDetectionEnabled:
        useSettingsStore.getState().bibleDetectionEnabled,
      semanticDetectionEnabled: true,
      confidenceThreshold: useSettingsStore.getState().confidenceThreshold,
      semanticConfidenceThreshold:
        useSettingsStore.getState().semanticConfidenceThreshold,
      cooldownMs: useSettingsStore.getState().cooldownMs,
    })
  })

  it("syncs Bible mode changes without changing transcription settings", async () => {
    const { useSettingsStore } = await import("@/stores/settings-store")
    const { useDetectionSettingsSync } =
      await import("./use-detection-settings-sync")

    function Probe() {
      useDetectionSettingsSync()
      return null
    }

    await act(async () => {
      root.render(React.createElement(Probe))
      await Promise.resolve()
    })

    invokeMock.mockClear()

    await act(async () => {
      useSettingsStore.getState().setBibleDetectionEnabled(false)
      await Promise.resolve()
    })

    expect(invokeMock).toHaveBeenCalledWith("update_detection_settings", {
      autoMode: useSettingsStore.getState().autoMode,
      bibleDetectionEnabled: false,
      semanticDetectionEnabled:
        useSettingsStore.getState().semanticDetectionEnabled,
      confidenceThreshold: useSettingsStore.getState().confidenceThreshold,
      semanticConfidenceThreshold:
        useSettingsStore.getState().semanticConfidenceThreshold,
      cooldownMs: useSettingsStore.getState().cooldownMs,
    })
  })
})
