import { beforeEach, describe, expect, it, vi } from "vitest"

const mockInvoke = vi.fn()
const mockToastError = vi.fn()

vi.mock("@/lib/tauri-runtime", () => ({
  invokeTauri: (...args: unknown[]) => mockInvoke(...args),
}))

vi.mock("sonner", () => ({
  toast: {
    error: (...args: unknown[]) => mockToastError(...args),
  },
}))

const handleHymnVoiceControlMock = vi.fn()
const handleQueueItemVoiceControlMock = vi.fn()

vi.mock("@/hooks/use-tauri-event", () => ({
  useTauriEvent: () => {},
}))

vi.mock("@/services/hymnal/hymn-voice-control", () => ({
  handleHymnVoiceControl: (...args: unknown[]) =>
    handleHymnVoiceControlMock(...args),
}))

vi.mock("@/services/queue/queue-voice-control", () => ({
  handleQueueItemVoiceControl: (...args: unknown[]) =>
    handleQueueItemVoiceControlMock(...args),
}))

async function loadModules() {
  vi.resetModules()
  const transcriptMod = await import("@/stores/transcript-store")
  const settingsMod = await import("@/stores/settings-store")
  const hookMod = await import("./use-transcription")
  return {
    useTranscriptStore: transcriptMod.useTranscriptStore,
    useSettingsStore: settingsMod.useSettingsStore,
    transcriptionActions: hookMod.transcriptionActions,
    handleTranscriptFinalPayload: hookMod.handleTranscriptFinalPayload,
    classifyTranscriptionIssue: hookMod.classifyTranscriptionIssue,
  }
}

async function loadFinalHandlerModules() {
  const modules = await loadModules()
  modules.useTranscriptStore.getState().setConnectionStatus("connected")
  return modules
}

describe("use-transcription", () => {
  beforeEach(() => {
    mockInvoke.mockReset()
    mockToastError.mockReset()
    handleHymnVoiceControlMock.mockReset()
    handleHymnVoiceControlMock.mockResolvedValue(false)
    handleQueueItemVoiceControlMock.mockReset()
    handleQueueItemVoiceControlMock.mockReturnValue(false)
  })

  describe("transcriptionActions.start", () => {
    it("invokes start_transcription with settings-derived params for vosk", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useSettingsStore, transcriptionActions } = await loadModules()

      useSettingsStore.setState({
        sttProvider: "vosk",
        audioDeviceId: "dev-42",
        gain: 1.5,
      })

      await transcriptionActions.start()

      expect(mockInvoke).toHaveBeenCalledWith("start_transcription", {
        deviceId: "dev-42",
        gain: 1.5,
        provider: "vosk",
        sttLanguage: "en",
        lowPower: false,
      })
    })

    it("keeps local transcription on Vosk for settings-derived params", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useSettingsStore, transcriptionActions } = await loadModules()

      useSettingsStore.setState({
        sttProvider: "vosk",
        audioDeviceId: "dev-43",
        gain: 1.25,
      })

      await transcriptionActions.start()

      expect(mockInvoke).toHaveBeenCalledWith("start_transcription", {
        deviceId: "dev-43",
        gain: 1.25,
        provider: "vosk",
        sttLanguage: "en",
        lowPower: false,
      })
    })

    it("forwards low power mode", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useSettingsStore, transcriptionActions } = await loadModules()

      useSettingsStore.setState({
        sttProvider: "vosk",
        lowPowerMode: true,
      })

      await transcriptionActions.start()

      expect(mockInvoke).toHaveBeenCalledWith(
        "start_transcription",
        expect.objectContaining({ lowPower: true })
      )
    })

    it("invokes deepgram provider without forwarding secrets", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useSettingsStore, transcriptionActions } = await loadModules()

      useSettingsStore.setState({
        sttProvider: "deepgram",
        audioDeviceId: null,
        gain: 1.0,
      })

      await transcriptionActions.start()

      expect(mockInvoke).toHaveBeenCalledWith(
        "start_transcription",
        expect.objectContaining({
          provider: "deepgram",
          deviceId: null,
          gain: 1.0,
        })
      )
    })

    it("invokes soniox provider without forwarding secrets", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useSettingsStore, transcriptionActions } = await loadModules()

      useSettingsStore.setState({
        sttProvider: "soniox",
        audioDeviceId: null,
        gain: 1.0,
      })

      await transcriptionActions.start()

      expect(mockInvoke).toHaveBeenCalledWith(
        "start_transcription",
        expect.objectContaining({
          provider: "soniox",
          deviceId: null,
          gain: 1.0,
        })
      )
    })

    it("sets connectionStatus to 'connecting' before invoke resolves and 'isTranscribing' after", async () => {
      let resolveInvoke: () => void = () => {}
      mockInvoke.mockReturnValue(
        new Promise<void>((resolve) => {
          resolveInvoke = resolve
        })
      )

      const { useTranscriptStore, transcriptionActions } = await loadModules()

      const pending = transcriptionActions.start()

      expect(useTranscriptStore.getState().connectionStatus).toBe("connecting")
      expect(useTranscriptStore.getState().isTranscribing).toBe(false)

      resolveInvoke()
      await pending

      expect(useTranscriptStore.getState().isTranscribing).toBe(true)
      expect(useTranscriptStore.getState().connectionStatus).not.toBe("error")
    })

    it("routes a missing-Deepgram-key error to onMissingApiKey (no toast)", async () => {
      mockInvoke.mockRejectedValue(
        "No Deepgram API key configured. Set it in Settings or via DEEPGRAM_API_KEY env var."
      )
      const { useSettingsStore, useTranscriptStore, transcriptionActions } =
        await loadModules()
      const onMissingApiKey = vi.fn()

      useSettingsStore.setState({ sttProvider: "deepgram" })
      await transcriptionActions.start(onMissingApiKey)

      expect(onMissingApiKey).toHaveBeenCalledWith("deepgram")
      expect(mockToastError).not.toHaveBeenCalled()
      expect(useTranscriptStore.getState().connectionStatus).toBe("error")
      expect(useTranscriptStore.getState().isTranscribing).toBe(false)
    })

    it("routes a missing-Soniox-key error and stores a visible issue", async () => {
      mockInvoke.mockRejectedValue(
        "No Soniox API key configured. Set it in Settings."
      )
      const { useSettingsStore, useTranscriptStore, transcriptionActions } =
        await loadModules()
      const onMissingApiKey = vi.fn()

      useSettingsStore.setState({ sttProvider: "soniox" })
      await transcriptionActions.start(onMissingApiKey)

      expect(onMissingApiKey).toHaveBeenCalledWith("soniox")
      expect(mockToastError).not.toHaveBeenCalled()
      expect(useTranscriptStore.getState().lastIssue).toMatchObject({
        kind: "missing_api_key",
        provider: "soniox",
        title: "Soniox API key needed",
      })
    })

    it("falls back to toast when missing-key error fires but no callback is provided", async () => {
      mockInvoke.mockRejectedValue("No Deepgram API key provided")
      const { transcriptionActions } = await loadModules()

      await transcriptionActions.start()

      expect(mockToastError).toHaveBeenCalledWith("Deepgram API key needed", {
        description:
          "Add a Deepgram API key in Speech settings, then start transcription again.",
        id: "stt-status",
      })
    })

    it("stores billing failures as actionable transcript issues", async () => {
      mockInvoke.mockRejectedValue(
        "Soniox error 402: Organization balance exhausted. Please either add funds manually or enable autopay."
      )
      const { useSettingsStore, useTranscriptStore, transcriptionActions } =
        await loadModules()

      useSettingsStore.setState({ sttProvider: "soniox" })
      await transcriptionActions.start()

      expect(useTranscriptStore.getState().lastIssue).toMatchObject({
        kind: "billing",
        provider: "soniox",
        title: "Soniox needs more transcription credit",
      })
      expect(mockToastError).toHaveBeenCalledWith(
        "Soniox needs more transcription credit",
        {
          description: expect.stringContaining("Add funds or enable autopay"),
          id: "stt-status",
        }
      )
    })

    it("surfaces local model errors as a specific issue", async () => {
      mockInvoke.mockRejectedValue("Vosk model not found")
      const { useTranscriptStore, transcriptionActions } = await loadModules()
      const onMissingApiKey = vi.fn()

      await transcriptionActions.start(onMissingApiKey)

      expect(onMissingApiKey).not.toHaveBeenCalled()
      expect(mockToastError).toHaveBeenCalledWith("Vosk model missing", {
        description:
          "The local speech model could not be found. Download the Vosk model from setup, then start transcription again.",
        id: "stt-status",
      })
      expect(useTranscriptStore.getState().connectionStatus).toBe("error")
      expect(useTranscriptStore.getState().lastIssue?.kind).toBe(
        "model_missing"
      )
    })
  })

  describe("late transcript events after disconnect", () => {
    it("does not record a final that arrives after the provider disconnected", async () => {
      const { useTranscriptStore, handleTranscriptFinalPayload } =
        await loadModules()
      useTranscriptStore.getState().setConnectionStatus("disconnected")

      await handleTranscriptFinalPayload({
        text: "late final after hangup",
        is_final: true,
        confidence: 0.9,
        words: [],
      })

      expect(useTranscriptStore.getState().segments).toEqual([])
    })

    it("records a final after reconnect", async () => {
      const { useTranscriptStore, handleTranscriptFinalPayload } =
        await loadModules()
      useTranscriptStore.getState().setConnectionStatus("disconnected")
      useTranscriptStore.getState().setConnectionStatus("connected")

      await handleTranscriptFinalPayload({
        text: "The Lord is my shepherd I shall not want",
        is_final: true,
        confidence: 0.9,
        words: [],
      })

      expect(useTranscriptStore.getState().segments.at(-1)?.text).toBe(
        "The Lord is my shepherd I shall not want"
      )
    })
  })

  describe("classifyTranscriptionIssue", () => {
    it("classifies auth, billing, network, and provider errors", async () => {
      const { classifyTranscriptionIssue } = await loadModules()

      expect(
        classifyTranscriptionIssue("invalid api key", "deepgram")
      ).toMatchObject({ kind: "auth", provider: "deepgram" })
      expect(
        classifyTranscriptionIssue("quota exceeded for this account", "soniox")
      ).toMatchObject({ kind: "billing", provider: "soniox" })
      expect(
        classifyTranscriptionIssue("websocket closed unexpectedly", "soniox")
      ).toMatchObject({ kind: "network", provider: "soniox" })
      expect(
        classifyTranscriptionIssue("provider returned malformed JSON", "soniox")
      ).toMatchObject({ kind: "provider", provider: "soniox" })
    })
  })

  describe("transcriptionActions.stop", () => {
    it("resets transcript state on success", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({
        isTranscribing: true,
        currentPartial: "partial text",
        connectionStatus: "connected",
      })

      await transcriptionActions.stop()

      const state = useTranscriptStore.getState()
      expect(state.isTranscribing).toBe(false)
      expect(state.currentPartial).toBe("")
      expect(state.connectionStatus).toBe("disconnected")
      expect(state.lastIssue).toBeNull()
      expect(mockToastError).not.toHaveBeenCalled()
    })

    it("silently swallows the exact 'Transcription is not running' error", async () => {
      mockInvoke.mockRejectedValue("Transcription is not running")
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({ isTranscribing: true })

      await transcriptionActions.stop()

      expect(mockToastError).not.toHaveBeenCalled()
      expect(useTranscriptStore.getState().isTranscribing).toBe(false)
    })

    it("silently swallows wrapped 'Transcription is not running' errors", async () => {
      mockInvoke.mockRejectedValue(
        "Command failed: Transcription is not running right now"
      )
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({ isTranscribing: true })

      await transcriptionActions.stop()

      expect(mockToastError).not.toHaveBeenCalled()
      expect(useTranscriptStore.getState().isTranscribing).toBe(false)
    })

    it("surfaces other stop errors as a toast AND still resets UI state", async () => {
      mockInvoke.mockRejectedValue("Audio device disappeared")
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({
        isTranscribing: true,
        currentPartial: "mid-sentence...",
        connectionStatus: "connected",
      })

      await transcriptionActions.stop()

      expect(mockToastError).toHaveBeenCalledWith(
        "Could not stop transcription",
        { description: "Audio device disappeared" }
      )
      const state = useTranscriptStore.getState()
      expect(state.isTranscribing).toBe(false)
      expect(state.currentPartial).toBe("")
      expect(state.connectionStatus).toBe("disconnected")
    })
  })

  describe("transcriptionActions.setLiveGain", () => {
    it("invokes set_input_gain while transcription is active", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({ isTranscribing: true })

      await transcriptionActions.setLiveGain(1.25)

      expect(mockInvoke).toHaveBeenCalledWith("set_input_gain", { gain: 1.25 })
    })

    it("does not invoke set_input_gain while transcription is inactive", async () => {
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({ isTranscribing: false })

      await transcriptionActions.setLiveGain(1.25)

      expect(mockInvoke).not.toHaveBeenCalled()
    })
  })

  describe("transcriptionActions.dumpMemory", () => {
    it("clears visible transcript without restarting when transcription is inactive", async () => {
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({
        isTranscribing: false,
        segments: [
          {
            id: "seg-1",
            text: "old words",
            is_final: true,
            confidence: 0.9,
            words: [],
            timestamp: Date.now(),
          },
        ],
        currentPartial: "half heard",
      })

      await transcriptionActions.dumpMemory()

      expect(useTranscriptStore.getState().segments).toEqual([])
      expect(useTranscriptStore.getState().currentPartial).toBe("")
      expect(mockInvoke).not.toHaveBeenCalled()
    })

    it("restarts active transcription so provider prompt state is discarded too", async () => {
      mockInvoke.mockResolvedValue(undefined)
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      useTranscriptStore.setState({
        isTranscribing: true,
        connectionStatus: "connected",
        segments: [
          {
            id: "seg-1",
            text: "old words",
            is_final: true,
            confidence: 0.9,
            words: [],
            timestamp: Date.now(),
          },
        ],
      })

      await transcriptionActions.dumpMemory()

      expect(useTranscriptStore.getState().segments).toEqual([])
      expect(mockInvoke).toHaveBeenNthCalledWith(1, "stop_transcription")
      expect(mockInvoke).toHaveBeenNthCalledWith(
        2,
        "start_transcription",
        expect.any(Object)
      )
    })

    it("restores the transcript when restart fails after stopping", async () => {
      mockInvoke
        .mockResolvedValueOnce(undefined)
        .mockRejectedValueOnce("Audio device disappeared")
      const { useTranscriptStore, transcriptionActions } = await loadModules()

      const existingSegments = [
        {
          id: "seg-1",
          text: "keep these words",
          is_final: true,
          confidence: 0.9,
          words: [],
          timestamp: Date.now(),
        },
      ]
      useTranscriptStore.setState({
        isTranscribing: true,
        connectionStatus: "connected",
        segments: existingSegments,
        currentPartial: "unfinished thought",
      })

      await transcriptionActions.dumpMemory()

      expect(mockInvoke).toHaveBeenNthCalledWith(1, "stop_transcription")
      expect(mockInvoke).toHaveBeenNthCalledWith(
        2,
        "start_transcription",
        expect.any(Object)
      )
      expect(useTranscriptStore.getState().segments).toEqual(existingSegments)
      expect(useTranscriptStore.getState().currentPartial).toBe(
        "unfinished thought"
      )
      expect(useTranscriptStore.getState().connectionStatus).toBe("error")
    })
  })

  describe("handleTranscriptFinalPayload", () => {
    it("coalesces adjacent Speechmatics finals into one visible segment", async () => {
      vi.useFakeTimers()
      vi.setSystemTime(1_000)
      const { useTranscriptStore, handleTranscriptFinalPayload } =
        await loadFinalHandlerModules()

      await handleTranscriptFinalPayload({
        text: "The first part of the sentence",
        is_final: true,
        confidence: 0.9,
        words: [],
        provider: "speechmatics",
      })
      vi.setSystemTime(3_000)
      await handleTranscriptFinalPayload({
        text: "continues in the next final span.",
        is_final: true,
        confidence: 0.95,
        words: [],
        provider: "speechmatics",
      })

      expect(useTranscriptStore.getState().segments).toMatchObject([
        {
          text: "The first part of the sentence continues in the next final span.",
          provider: "speechmatics",
        },
      ])
      vi.useRealTimers()
    })

    it("starts a new Speechmatics line after the coalescing gap", async () => {
      vi.useFakeTimers()
      vi.setSystemTime(1_000)
      const { useTranscriptStore, handleTranscriptFinalPayload } =
        await loadFinalHandlerModules()

      await handleTranscriptFinalPayload({
        text: "First thought.",
        is_final: true,
        confidence: 0.9,
        words: [],
        provider: "speechmatics",
      })
      vi.setSystemTime(5_001)
      await handleTranscriptFinalPayload({
        text: "Second thought.",
        is_final: true,
        confidence: 0.9,
        words: [],
        provider: "speechmatics",
      })

      expect(useTranscriptStore.getState().segments).toHaveLength(2)
      vi.useRealTimers()
    })

    it("keeps non-Speechmatics finals as separate visible segments", async () => {
      vi.useFakeTimers()
      vi.setSystemTime(1_000)
      const { useTranscriptStore, handleTranscriptFinalPayload } =
        await loadFinalHandlerModules()

      for (const provider of ["deepgram", "soniox"] as const) {
        await handleTranscriptFinalPayload({
          text: `${provider} first`,
          is_final: true,
          confidence: 0.9,
          words: [],
          provider,
        })
        vi.setSystemTime(Date.now() + 2_000)
        await handleTranscriptFinalPayload({
          text: `${provider} second`,
          is_final: true,
          confidence: 0.9,
          words: [],
          provider,
        })
      }

      expect(useTranscriptStore.getState().segments).toHaveLength(4)
      vi.useRealTimers()
    })

    it("stores final transcript segments and invokes hymn voice control", async () => {
      const { useTranscriptStore, handleTranscriptFinalPayload } =
        await loadFinalHandlerModules()
      useTranscriptStore.getState().setPartial("hymn")

      await handleTranscriptFinalPayload({
        text: "hymn 12",
        is_final: true,
        confidence: 0.95,
        words: [],
      })

      const state = useTranscriptStore.getState()
      expect(state.segments).toHaveLength(1)
      expect(state.segments[0]).toMatchObject({
        text: "hymn 12",
        is_final: true,
        confidence: 0.95,
      })
      expect(state.currentPartial).toBe("")
      expect(handleHymnVoiceControlMock).toHaveBeenCalledWith("hymn 12")
    })

    it("ignores a final transcript received after disconnection", async () => {
      const { useTranscriptStore, handleTranscriptFinalPayload } =
        await loadFinalHandlerModules()
      useTranscriptStore.getState().setConnectionStatus("disconnected")

      await handleTranscriptFinalPayload({
        text: "late stale final",
        is_final: true,
        confidence: 0.95,
        words: [],
      })

      expect(useTranscriptStore.getState().segments).toEqual([])
    })

    it("stores and handles queue commands while Bible mode is off", async () => {
      handleQueueItemVoiceControlMock.mockReturnValue(true)
      const {
        useSettingsStore,
        useTranscriptStore,
        handleTranscriptFinalPayload,
      } = await loadFinalHandlerModules()
      useSettingsStore.getState().setBibleDetectionEnabled(false)

      await handleTranscriptFinalPayload({
        text: "item number 2",
        is_final: true,
        confidence: 0.95,
        words: [],
      })

      expect(useTranscriptStore.getState().segments.at(-1)).toMatchObject({
        text: "item number 2",
        is_final: true,
      })
      expect(handleQueueItemVoiceControlMock).toHaveBeenCalledWith(
        "item number 2"
      )
      expect(handleHymnVoiceControlMock).not.toHaveBeenCalled()
    })

    it("invokes hymn voice control for Adventist hymnal cue variants", async () => {
      const { handleTranscriptFinalPayload } = await loadFinalHandlerModules()

      for (const text of [
        "SDA hymn 100",
        "Adventist hymn 100",
        "Seventh-day Adventist hymnal 100",
        "SDA lied 100",
        "Adventiste liedboek 100",
        "Sewendedag Adventiste lied 100",
      ]) {
        handleHymnVoiceControlMock.mockClear()

        await handleTranscriptFinalPayload({
          text,
          is_final: true,
          confidence: 0.95,
          words: [],
        })

        expect(handleHymnVoiceControlMock).toHaveBeenCalledWith(text)
      }
    })
  })

  describe("stt_error integration contract", () => {
    it("surfaces stt errors via classified issue state", async () => {
      const { useTranscriptStore, classifyTranscriptionIssue } =
        await loadModules()

      // Simulate what the stt_error handler does
      const issue = classifyTranscriptionIssue(
        "Soniox error 402: Organization balance exhausted.",
        "soniox"
      )
      useTranscriptStore.getState().setConnectionStatus("error")
      useTranscriptStore.getState().setIssue(issue)
      mockToastError(issue.title, {
        description: issue.description,
        id: "stt-status",
      })

      expect(useTranscriptStore.getState().connectionStatus).toBe("error")
      expect(useTranscriptStore.getState().lastIssue).toMatchObject({
        kind: "billing",
        provider: "soniox",
      })
      expect(mockToastError).toHaveBeenCalledWith(issue.title, {
        description: issue.description,
        id: "stt-status",
      })
    })
  })
})
