import { useCallback, useEffect } from "react"
import { invokeTauri } from "@/lib/tauri-runtime"
import { toast } from "sonner"
import {
  observeDetectionCandidates,
  profileDetectionEvent,
} from "@/lib/detection-profiler"
import {
  handleReadingAdvance,
  scheduleVerseDetections,
} from "@/lib/verse-detection-workflow"
import { refreshLiveTranslation } from "@/lib/presentation-workflow"
import { useAudioStore } from "@/stores/audio-store"
import { useBibleStore } from "@/stores/bible-store"
import { useDetectionStore } from "@/stores/detection-store"
import { useSettingsStore, type SttProvider } from "@/stores/settings-store"
import {
  useTranscriptStore,
  type TranscriptionIssue,
} from "@/stores/transcript-store"
import { handleSermonSlideVoiceControl } from "@/services/slides/sermon-slide-voice-control"
import { handleQueueItemVoiceControl } from "@/services/queue/queue-voice-control"
import { loadHymnVoiceControl } from "@/services/hymnal/hymn-voice-control-loader"
import {
  recordWorkflowTrace,
  traceDetectionBatchDetails,
  traceReadingAdvanceDetails,
  traceTranscriptDetails,
} from "@/lib/workflow-trace"
import type { DetectionResult, ReadingAdvance } from "@/types"
import { useTauriEvent } from "./use-tauri-event"

interface TranscriptPartialPayload {
  text: string
  is_final: boolean
  provider?: SttProvider
  confidence: number
  words: Array<{
    text: string
    start: number
    end: number
    confidence: number
    punctuated: string
  }>
}

interface UseTranscriptionOptions {
  /**
   * Called when `start_transcription` fails because the user picked a cloud
   * provider but hasn't set an API key. Panels typically react by
   * opening a key-prompt dialog instead of showing the default toast.
   */
  onMissingApiKey?: (provider: SttProvider) => void
}

const MISSING_DEEPGRAM_KEY_MARKER = "No Deepgram API key"
const MISSING_SONIOX_KEY_MARKER = "No Soniox API key"
const MISSING_SPEECHMATICS_KEY_MARKER = "No Speechmatics API key"
const NOT_RUNNING_ERROR = "Transcription is not running"
const MAYBE_HYMN_CUE_PATTERN =
  /\b(?:(?:sda|adventist|adventiste|seventh(?:\s|-)?day\s+adventist|sewende(?:\s|-)?dag\s+adventiste)\s+(?:hymn|hymns|hymnal|hymnals|song|songs|lied|liedere|liedboek|liedboeke)|(?:hymn|hymns|hymnal|hymnals|song|songs|lied|liedere|liedboek|liedboeke))(?:\s+(?:number|nommer))?\s+[a-z0-9]/i
const BILLING_ERROR_PATTERN =
  /\b(?:402|balance exhausted|insufficient balance|insufficient[_ ]funds|quota|credits?|billing|payment|tokens?|funds?|autopay)\b/i
const AUTH_ERROR_PATTERN =
  /\b(?:401|403|unauthorized|not[_ ]authorised|forbidden|invalid api key|authentication|permission denied)\b/i
const NETWORK_ERROR_PATTERN =
  /\b(?:connection failed|failed to connect|timeout|timed out|dns|network|websocket|socket closed|closed unexpectedly)\b/i
const MODEL_MISSING_PATTERN =
  /\b(?:model not found|model missing|worker not found|download.*model|missing.*model)\b/i
const STT_STATUS_TOAST_ID = "stt-status"

const PROVIDER_LABELS: Record<SttProvider, string> = {
  deepgram: "Deepgram",
  soniox: "Soniox",
  speechmatics: "Speechmatics",
  vosk: "Vosk",
}

function isMissingApiKeyMessage(message: string): boolean {
  return (
    message.includes(MISSING_DEEPGRAM_KEY_MARKER) ||
    message.includes(MISSING_SONIOX_KEY_MARKER) ||
    message.includes(MISSING_SPEECHMATICS_KEY_MARKER)
  )
}

function providerFromMessage(message: string): SttProvider | null {
  if (message.includes(MISSING_DEEPGRAM_KEY_MARKER)) return "deepgram"
  if (message.includes(MISSING_SONIOX_KEY_MARKER)) return "soniox"
  if (message.includes(MISSING_SPEECHMATICS_KEY_MARKER)) return "speechmatics"
  return null
}

export function classifyTranscriptionIssue(
  message: string,
  provider: SttProvider
): TranscriptionIssue {
  const issueProvider = providerFromMessage(message) ?? provider
  const providerLabel = PROVIDER_LABELS[issueProvider]

  if (isMissingApiKeyMessage(message)) {
    return {
      kind: "missing_api_key",
      provider: issueProvider,
      title: `${providerLabel} API key needed`,
      description: `Add a ${providerLabel} API key in Speech settings, then start transcription again.`,
      actionLabel: "Open settings",
    }
  }

  if (BILLING_ERROR_PATTERN.test(message)) {
    return {
      kind: "billing",
      provider: issueProvider,
      title: `${providerLabel} needs more transcription credit`,
      description: `${providerLabel} rejected the session because the account needs more credit, tokens, or billing setup. Add funds or enable autopay in the provider console, then start transcription again.`,
    }
  }

  if (AUTH_ERROR_PATTERN.test(message)) {
    return {
      kind: "auth",
      provider: issueProvider,
      title: `${providerLabel} key was rejected`,
      description: `Check that the ${providerLabel} API key is active and has permission to use speech transcription.`,
      actionLabel: "Open settings",
    }
  }

  if (NETWORK_ERROR_PATTERN.test(message)) {
    return {
      kind: "network",
      provider: issueProvider,
      title: `${providerLabel} connection failed`,
      description:
        "The transcription service could not be reached. Check the internet connection and try starting transcription again.",
    }
  }

  if (provider === "vosk" && MODEL_MISSING_PATTERN.test(message)) {
    return {
      kind: "model_missing",
      provider,
      title: "Vosk model missing",
      description:
        "The local speech model could not be found. Download the Vosk model from setup, then start transcription again.",
    }
  }

  return {
    kind: issueProvider === "vosk" ? "unknown" : "provider",
    provider: issueProvider,
    title: `${providerLabel} transcription stopped`,
    description: message,
  }
}

export const transcriptionActions = {
  async start(
    onMissingApiKey?: (provider: SttProvider) => void
  ): Promise<boolean> {
    const transcript = useTranscriptStore.getState()
    transcript.setConnectionStatus("connecting")
    transcript.clearIssue()

    const settings = useSettingsStore.getState()
    try {
      await invokeTauri("start_transcription", {
        deviceId: settings.audioDeviceId,
        gain: settings.gain,
        provider: settings.sttProvider,
        sttLanguage: settings.sttLanguage,
        lowPower: settings.lowPowerMode,
      })
      transcript.setTranscribing(true)
      transcript.clearIssue()
      return true
    } catch (e) {
      const msg = String(e)
      const issue = classifyTranscriptionIssue(msg, settings.sttProvider)
      transcript.setConnectionStatus("error")
      transcript.setIssue(issue)
      if (issue.kind === "missing_api_key" && onMissingApiKey) {
        onMissingApiKey(issue.provider)
      } else {
        toast.error(issue.title, {
          description: issue.description,
          id: STT_STATUS_TOAST_ID,
        })
      }
      return false
    }
  },

  async stop(): Promise<void> {
    const transcript = useTranscriptStore.getState()
    try {
      await invokeTauri("stop_transcription")
    } catch (e) {
      if (!String(e).includes(NOT_RUNNING_ERROR)) {
        toast.error("Could not stop transcription", { description: String(e) })
      }
    }
    transcript.setTranscribing(false)
    transcript.setPartial("")
    transcript.setConnectionStatus("disconnected")
    transcript.clearIssue()
  },

  async setLiveGain(gain: number): Promise<void> {
    if (!useTranscriptStore.getState().isTranscribing) return

    try {
      await invokeTauri("set_input_gain", { gain })
    } catch (error) {
      console.warn("[transcription] Could not update live input gain", error)
    }
  },

  async dumpMemory(
    onMissingApiKey?: (provider: SttProvider) => void
  ): Promise<void> {
    const transcript = useTranscriptStore.getState()
    const wasTranscribing = transcript.isTranscribing
    const previousSegments = transcript.segments
    const previousPartial = transcript.currentPartial

    if (!wasTranscribing) {
      transcript.clearTranscript()
      transcript.clearIssue()
      return
    }

    await transcriptionActions.stop()
    const restarted = await transcriptionActions.start(onMissingApiKey)
    if (restarted) {
      useTranscriptStore.getState().clearTranscript()
      return
    }

    // Restore the pre-restart transcript without clobbering anything newer
    // that arrived while the restart was in flight — a blind setState
    // previously overwrote finals and partials that landed during the
    // restart. Each field is restored only when its current value is empty.
    useTranscriptStore.setState((state) => ({
      segments: state.segments.length === 0 ? previousSegments : state.segments,
      currentPartial:
        state.currentPartial === "" ? previousPartial : state.currentPartial,
    }))
  },
}

export async function handleTranscriptFinalPayload(
  payload: TranscriptPartialPayload
): Promise<void> {
  recordWorkflowTrace("transcription.final", "Final transcript received", {
    ...traceTranscriptDetails({
      text: payload.text,
      confidence: payload.confidence,
      isFinal: true,
      wordCount: payload.words.length,
    }),
  })

  const transcriptStore = useTranscriptStore.getState()
  transcriptStore.setPartial("")
  transcriptStore.addSegment({
    id: crypto.randomUUID(),
    text: payload.text,
    is_final: true,
    confidence: payload.confidence,
    words: payload.words,
    timestamp: Date.now(),
    provider: payload.provider,
  })
  if (handleSermonSlideVoiceControl(payload.text)) return
  if (handleQueueItemVoiceControl(payload.text)) return
  if (!MAYBE_HYMN_CUE_PATTERN.test(payload.text)) return

  const { handleHymnVoiceControl } = await loadHymnVoiceControl()
  await handleHymnVoiceControl(payload.text)
}

export function useTranscriptionEventBridge() {
  // STT lifecycle events
  useTauriEvent("stt_connected", () => {
    recordWorkflowTrace("transcription.connected", "STT connected")
    const transcript = useTranscriptStore.getState()
    transcript.setConnectionStatus("connected")
    transcript.clearIssue()
  })
  useTauriEvent("stt_disconnected", () => {
    recordWorkflowTrace("transcription.disconnected", "STT disconnected")
    const transcript = useTranscriptStore.getState()
    transcript.setTranscribing(false)
    if (transcript.connectionStatus !== "error") {
      transcript.setConnectionStatus("disconnected")
    }
  })
  useTauriEvent<string>("stt_voice_control", (command) => {
    if (command === "stop") {
      const transcript = useTranscriptStore.getState()
      transcript.setTranscribing(false)
      transcript.setPartial("")
      transcript.setConnectionStatus("disconnected")
    }
  })
  useTauriEvent<string>("stt_error", (msg) => {
    recordWorkflowTrace("transcription.error", "STT error", { message: msg })
    const provider = useSettingsStore.getState().sttProvider
    const issue = classifyTranscriptionIssue(msg, provider)
    const transcript = useTranscriptStore.getState()
    transcript.setTranscribing(false)
    transcript.setPartial("")
    transcript.setConnectionStatus("error")
    transcript.setIssue(issue)
    toast.error(issue.title, {
      description: issue.description,
      id: STT_STATUS_TOAST_ID,
    })
  })
  useTauriEvent("stt_speech_started", () => {
    // Only show the synthetic hint while actually transcribing, and clear it
    // after a short window: VAD starts without recognized words (coughs,
    // noise) otherwise left a fake "Speech detected..." partial pulsing next
    // to an idle state indefinitely.
    if (!useTranscriptStore.getState().isTranscribing) return
    useTranscriptStore.getState().setPartial("Speech detected...")
    window.setTimeout(() => {
      const transcript = useTranscriptStore.getState()
      if (transcript.currentPartial === "Speech detected...") {
        transcript.setPartial("")
      }
    }, 3_000)
  })

  // Audio source lifecycle: when the OS device disappears (mic unplugged,
  // headset disconnects, app loses access) the watchdog in the Rust fanout
  // thread emits `audio_source_lost`, then `audio_source_recovered` once it
  // sees the device return. The STT provider stays alive across the gap.
  useTauriEvent("audio_source_lost", () => {
    useAudioStore.getState().setSourceLost(true)
    toast.warning("Audio source disconnected", {
      description: "Waiting for the device to come back…",
      id: "audio-source-status",
    })
  })
  useTauriEvent("audio_source_recovered", () => {
    useAudioStore.getState().setSourceLost(false)
    toast.success("Audio source reconnected", {
      id: "audio-source-status",
    })
  })

  useTauriEvent<TranscriptPartialPayload>("transcript_partial", (payload) => {
    recordWorkflowTrace(
      "transcription.partial",
      "Partial transcript received",
      {
        ...traceTranscriptDetails({
          text: payload.text,
          confidence: payload.confidence,
          isFinal: false,
          wordCount: payload.words.length,
        }),
      }
    )
    // A partial that arrives after an error/disconnect must not resurrect a
    // ghost "live" line or wipe the persistent error card — the only visible
    // error UI while idle. Gate on connection status (set by stt_connected /
    // stt_disconnected / error), not the start() flag: providers emit partials
    // as soon as they connect, and the e2e replay harness drives the same
    // event path without invoking start_transcription.
    const transcript = useTranscriptStore.getState()
    if (
      transcript.connectionStatus === "error" ||
      transcript.connectionStatus === "disconnected"
    ) {
      return
    }
    // Single write, skipped entirely when nothing visible would change
    // (providers frequently re-emit identical partial text).
    if (
      transcript.currentPartial !== payload.text ||
      transcript.lastIssue !== null
    ) {
      useTranscriptStore.setState({
        currentPartial: payload.text,
        lastIssue: null,
      })
    }
  })

  useTauriEvent<TranscriptPartialPayload>("transcript_final", (payload) => {
    void handleTranscriptFinalPayload(payload)
  })

  useTauriEvent<{ rms: number; peak: number }>("audio_level", (payload) => {
    useAudioStore.getState().setLevel(payload)
  })

  // Voice translation commands: "read in NIV", "switch to ESV"
  useTauriEvent<{ abbreviation: string; translation_id: number }>(
    "translation_command",
    (data) => {
      useBibleStore.getState().setActiveTranslation(data.translation_id)
      void refreshLiveTranslation()
      if (import.meta.env.DEV) {
        console.log(`[VOICE] Translation switched to ${data.abbreviation}`)
      }
    }
  )

  useTauriEvent<DetectionResult[]>("verse_detections", (detections) => {
    observeDetectionCandidates(detections)
    recordWorkflowTrace("detection.event", "Verse detections event received", {
      ...traceDetectionBatchDetails(detections),
    })
    profileDetectionEvent("verse_detections", detections.length, () => {
      return scheduleVerseDetections(detections)
    })
  })

  useTauriEvent<ReadingAdvance>("reading_mode_verse", (advance) => {
    recordWorkflowTrace("reading.event", "Reading-mode verse event received", {
      ...traceReadingAdvanceDetails(advance),
    })
    profileDetectionEvent("reading_mode_verse", 1, () => {
      handleReadingAdvance(advance)
    })
  })

  useEffect(() => {
    const id = setInterval(() => {
      useDetectionStore.getState().evictStale()
    }, 2_000)
    return () => clearInterval(id)
  }, [])
}

export function useTranscription(options?: UseTranscriptionOptions) {
  const segments = useTranscriptStore((s) => s.segments)
  const isTranscribing = useTranscriptStore((s) => s.isTranscribing)
  const connectionStatus = useTranscriptStore((s) => s.connectionStatus)

  const onMissingApiKey = options?.onMissingApiKey

  const startTranscription = useCallback(
    () => transcriptionActions.start(onMissingApiKey),
    [onMissingApiKey]
  )
  const dumpTranscriptMemory = useCallback(
    () => transcriptionActions.dumpMemory(onMissingApiKey),
    [onMissingApiKey]
  )

  return {
    segments,
    isTranscribing,
    connectionStatus,
    startTranscription,
    stopTranscription: transcriptionActions.stop,
    dumpTranscriptMemory,
  }
}
