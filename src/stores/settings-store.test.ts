import { beforeEach, describe, expect, it, vi } from "vitest"

const reportOutputIssueMock = vi.fn()

vi.mock("@/stores/broadcast-store", () => ({
  useBroadcastStore: {
    getState: () => ({
      reportOutputIssue: reportOutputIssueMock,
    }),
  },
}))

const mockGet = vi.fn()
const mockSet = vi.fn()
const mockSave = vi.fn()
const mockLoad = vi.fn()

vi.mock("@tauri-apps/plugin-store", () => ({
  load: (...args: unknown[]) => mockLoad(...args),
}))

async function flushSave(): Promise<void> {
  // Advance past the debounce window, then let the chained
  // pendingSave promise resolve.
  await vi.advanceTimersByTimeAsync(300)
  await Promise.resolve()
  await Promise.resolve()
}

describe("settings store", () => {
  beforeEach(async () => {
    vi.useFakeTimers()
    const storage = new Map<string, string>()
    vi.stubGlobal("localStorage", {
      getItem: vi.fn((key: string) => storage.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => {
        storage.set(key, value)
      }),
      removeItem: vi.fn((key: string) => {
        storage.delete(key)
      }),
      clear: vi.fn(() => {
        storage.clear()
      }),
    })
    localStorage.clear()
    reportOutputIssueMock.mockReset()
    mockGet.mockReset()
    mockSet.mockReset()
    mockSave.mockReset()
    mockLoad.mockReset()
    mockLoad.mockResolvedValue({
      get: mockGet,
      set: mockSet,
      save: mockSave,
    })
    vi.resetModules()
  })

  it("hydrate merges persisted values over defaults", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "gain") return 2.5
      if (key === "sttProvider") return "deepgram"
      if (key === "autoPreviewDetections") return false
      if (key === "bibleDetectionEnabled") return false
      if (key === "semanticDetectionEnabled") return true
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    const state = useSettingsStore.getState()
    expect(state.gain).toBe(2.5)
    expect(state.sttProvider).toBe("deepgram")
    expect(state.autoPreviewDetections).toBe(false)
    expect(state.bibleDetectionEnabled).toBe(false)
    expect(state.semanticDetectionEnabled).toBe(true)
    // Defaults remain for keys with null
    expect(state.autoMode).toBe(false)
    expect(state.confidenceThreshold).toBe(0.9)
    expect(state.semanticConfidenceThreshold).toBe(0.7)
  })

  it("hydrate with no persisted values falls back to defaults", async () => {
    mockGet.mockResolvedValue(null)

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()
    const after = useSettingsStore.getState()

    expect(after.gain).toBe(1.0)
    expect(after.sttProvider).toBe("vosk")
    expect(after.autoMode).toBe(false)
    expect(after.autoPreviewDetections).toBe(true)
    expect(after.bibleDetectionEnabled).toBe(true)
    expect(after.semanticDetectionEnabled).toBe(true)
  })

  it("migrates the legacy default confidence threshold to the auto-live default", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "confidenceThreshold") return 0.8
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    expect(useSettingsStore.getState().confidenceThreshold).toBe(0.9)
  })

  it("clamps confidence threshold updates to a finite 0-1 range", async () => {
    const { useSettingsStore } = await import("./settings-store")

    useSettingsStore.getState().setConfidenceThreshold(2)
    expect(useSettingsStore.getState().confidenceThreshold).toBe(1)

    useSettingsStore.getState().setConfidenceThreshold(-0.5)
    expect(useSettingsStore.getState().confidenceThreshold).toBe(0)

    useSettingsStore.getState().setConfidenceThreshold(Number.NaN)
    expect(useSettingsStore.getState().confidenceThreshold).toBe(0.9)
  })

  it("does not trust persisted Deepgram key status when keychain status is unavailable", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "hasDeepgramApiKey") return true
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    expect(useSettingsStore.getState().hasDeepgramApiKey).toBe(false)
    expect(mockGet).not.toHaveBeenCalledWith("hasDeepgramApiKey")
  })

  it("migrates persisted Gladia provider to Vosk", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "sttProvider") return "gladia"
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    expect(useSettingsStore.getState().sttProvider).toBe("vosk")
  })

  it("migrates persisted Sherpa provider to Vosk", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "sttProvider") return "sherpa"
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    expect(useSettingsStore.getState().sttProvider).toBe("vosk")
  })

  it("hydrate maps persisted Whisper provider to local Vosk", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "sttProvider") return "whisper"
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    expect(useSettingsStore.getState().sttProvider).toBe("vosk")
  })

  it("hydrate maps removed faster-whisper provider to local Vosk", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "sttProvider") return "faster-whisper"
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    expect(useSettingsStore.getState().sttProvider).toBe("vosk")
  })

  it("hydrates persisted Vosk provider", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "sttProvider") return "vosk"
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    expect(useSettingsStore.getState().sttProvider).toBe("vosk")
  })

  it("a setter call after hydration writes the full snapshot to disk", async () => {
    mockGet.mockResolvedValue(null)

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    useSettingsStore.getState().setGain(1.75)

    // Debounced — nothing written yet.
    expect(mockSet).not.toHaveBeenCalled()
    expect(mockSave).not.toHaveBeenCalled()

    await flushSave()

    expect(mockSet).toHaveBeenCalledWith("gain", 1.75)
    expect(mockSave).toHaveBeenCalledTimes(1)
  })

  it("rapid setter calls coalesce into a single save", async () => {
    mockGet.mockResolvedValue(null)

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    const { setGain } = useSettingsStore.getState()
    setGain(1.1)
    setGain(1.2)
    setGain(1.3)

    await flushSave()

    expect(mockSave).toHaveBeenCalledTimes(1)
    expect(mockSet).toHaveBeenCalledWith("gain", 1.3)
  })

  it("concurrent hydrate calls attach only one subscription", async () => {
    mockGet.mockResolvedValue(null)

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    // Kick off two concurrent hydrations — a second caller must not
    // attach a duplicate subscription that would double every write.
    await Promise.all([hydrateSettings(), hydrateSettings()])

    useSettingsStore.getState().setGain(1.5)
    await flushSave()

    expect(mockSave).toHaveBeenCalledTimes(1)
  })

  it("repeated hydrate calls after completion still attach only one subscription", async () => {
    mockGet.mockResolvedValue(null)

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()
    await hydrateSettings()

    useSettingsStore.getState().setGain(1.5)
    await flushSave()

    expect(mockSave).toHaveBeenCalledTimes(1)
  })

  it("hydrate handles load rejection gracefully", async () => {
    mockLoad.mockRejectedValue(new Error("store not available"))
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {})

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await expect(hydrateSettings()).resolves.toBeUndefined()

    // Defaults preserved
    expect(useSettingsStore.getState().gain).toBe(1.0)
    expect(warnSpy).toHaveBeenCalledWith(
      "[settings] Failed to load persisted state, using defaults"
    )
    expect(reportOutputIssueMock).toHaveBeenCalledWith(
      expect.objectContaining({
        outputId: "global",
        kind: "persistence",
        title: "Settings load failed",
      })
    )
    warnSpy.mockRestore()
  })

  it("reports settings load failure on every failed hydration", async () => {
    mockLoad.mockRejectedValue(new Error("store not available"))
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {})

    let settingsModule = await import("./settings-store")
    await settingsModule.hydrateSettings()
    expect(reportOutputIssueMock).toHaveBeenCalledTimes(1)

    vi.resetModules()
    settingsModule = await import("./settings-store")
    await settingsModule.hydrateSettings()
    expect(reportOutputIssueMock).toHaveBeenCalledTimes(2)

    warnSpy.mockRestore()
  })

  it("persisted numeric zero values survive hydration", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "gain") return 0
      if (key === "confidenceThreshold") return 0
      if (key === "semanticConfidenceThreshold") return 0
      if (key === "cooldownMs") return 0
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    const state = useSettingsStore.getState()
    expect(state.gain).toBe(0)
    expect(state.confidenceThreshold).toBe(0)
    expect(state.semanticConfidenceThreshold).toBe(0)
    expect(state.cooldownMs).toBe(0)
    // Non-zero-keyed fields stay at defaults
    expect(state.sttProvider).toBe("vosk")
  })

  it("hydrates persisted Soniox provider and cloud language", async () => {
    mockGet.mockImplementation(async (key: string) => {
      if (key === "sttProvider") return "soniox"
      if (key === "sttLanguage") return "es"
      return null
    })

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    const state = useSettingsStore.getState()
    expect(state.sttProvider).toBe("soniox")
    expect(state.sttLanguage).toBe("es")
  })

  it("persist handles save rejection gracefully", async () => {
    mockGet.mockResolvedValue(null)
    mockSave.mockRejectedValue(new Error("disk error"))
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {})

    const { hydrateSettings, useSettingsStore } =
      await import("./settings-store")
    await hydrateSettings()

    useSettingsStore.getState().setAutoMode(true)
    await flushSave()

    expect(warnSpy).toHaveBeenCalledWith(
      "[settings] Failed to persist settings"
    )
    expect(reportOutputIssueMock).toHaveBeenCalledWith(
      expect.objectContaining({
        outputId: "global",
        kind: "persistence",
        title: "Settings save failed",
      })
    )
    warnSpy.mockRestore()
  })
})
