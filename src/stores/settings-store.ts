import { create } from "zustand"
import { load, type Store } from "@tauri-apps/plugin-store"
import { isTauriRuntime, invokeTauri } from "@/lib/tauri-runtime"
import { useBroadcastOutputIssueStore } from "@/stores/broadcast/output-issue-store"

export type SttProvider = "deepgram" | "soniox" | "speechmatics" | "vosk"
export type SttLanguage = "en" | "af" | "es" | "fr" | "pt"

const DEFAULT_CONFIDENCE_THRESHOLD = 0.9
const DEFAULT_SEMANTIC_CONFIDENCE_THRESHOLD = 0.7
const LEGACY_DEFAULT_CONFIDENCE_THRESHOLD = 0.8
const LEGACY_AUTO_LIVE_THRESHOLD = 0.85

function normalizeConfidenceThreshold(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_CONFIDENCE_THRESHOLD
  return Math.min(Math.max(value, 0), 1)
}

interface SettingsState {
  hasDeepgramApiKey: boolean
  audioDeviceId: string | null
  gain: number
  autoMode: boolean
  autoPreviewDetections: boolean
  bibleDetectionEnabled: boolean
  semanticDetectionEnabled: boolean
  confidenceThreshold: number
  semanticConfidenceThreshold: number
  cooldownMs: number
  onboardingComplete: boolean
  sttProvider: SttProvider
  sttLanguage: SttLanguage
  hasSonioxApiKey: boolean
  hasSpeechmaticsApiKey: boolean
  hasDeepseekApiKey: boolean
  /** Cloud AI candidate ranking for indirect (semantic) references. */
  deepseekRankingEnabled: boolean
  /** Reduce CPU/RAM use on weaker machines (semantic detection runs on
   *  finished sentences only). */
  lowPowerMode: boolean
  /** Show a brief toast for operator actions (queue, send live, clear). */
  actionNotificationsEnabled: boolean

  setHasDeepgramApiKey: (has: boolean) => void
  setAudioDeviceId: (id: string | null) => void
  setGain: (gain: number) => void
  setAutoMode: (auto: boolean) => void
  setAutoPreviewDetections: (enabled: boolean) => void
  setBibleDetectionEnabled: (enabled: boolean) => void
  setSemanticDetectionEnabled: (enabled: boolean) => void
  setConfidenceThreshold: (threshold: number) => void
  setSemanticConfidenceThreshold: (threshold: number) => void
  setCooldownMs: (ms: number) => void
  setOnboardingComplete: (complete: boolean) => void
  setSttProvider: (provider: SttProvider) => void
  setSttLanguage: (language: SttLanguage) => void
  setHasSonioxApiKey: (has: boolean) => void
  setHasSpeechmaticsApiKey: (has: boolean) => void
  setHasDeepseekApiKey: (has: boolean) => void
  setDeepseekRankingEnabled: (enabled: boolean) => void
  setLowPowerMode: (enabled: boolean) => void
  setActionNotificationsEnabled: (enabled: boolean) => void
}

export const useSettingsStore = create<SettingsState>((set) => ({
  hasDeepgramApiKey: false,
  hasSonioxApiKey: false,
  hasSpeechmaticsApiKey: false,
  hasDeepseekApiKey: false,
  deepseekRankingEnabled: false,
  audioDeviceId: null,
  gain: 1.0,
  autoMode: false,
  autoPreviewDetections: true,
  bibleDetectionEnabled: true,
  semanticDetectionEnabled: true,
  confidenceThreshold: DEFAULT_CONFIDENCE_THRESHOLD,
  semanticConfidenceThreshold: DEFAULT_SEMANTIC_CONFIDENCE_THRESHOLD,
  cooldownMs: 2500,
  onboardingComplete: false,
  sttProvider: "vosk",
  sttLanguage: "en",
  lowPowerMode: false,
  actionNotificationsEnabled: false,

  setHasDeepgramApiKey: (hasDeepgramApiKey) => set({ hasDeepgramApiKey }),
  setHasSonioxApiKey: (hasSonioxApiKey) => set({ hasSonioxApiKey }),
  setHasSpeechmaticsApiKey: (hasSpeechmaticsApiKey) =>
    set({ hasSpeechmaticsApiKey }),
  setHasDeepseekApiKey: (hasDeepseekApiKey) => set({ hasDeepseekApiKey }),
  setDeepseekRankingEnabled: (deepseekRankingEnabled) =>
    set({ deepseekRankingEnabled }),
  setAudioDeviceId: (audioDeviceId) => set({ audioDeviceId }),
  setGain: (gain) => set({ gain }),
  setAutoMode: (autoMode) => set({ autoMode }),
  setAutoPreviewDetections: (autoPreviewDetections) =>
    set({ autoPreviewDetections }),
  setBibleDetectionEnabled: (bibleDetectionEnabled) =>
    set({ bibleDetectionEnabled }),
  setSemanticDetectionEnabled: (semanticDetectionEnabled) =>
    set({ semanticDetectionEnabled }),
  setConfidenceThreshold: (confidenceThreshold) =>
    set({
      confidenceThreshold: normalizeConfidenceThreshold(confidenceThreshold),
    }),
  setSemanticConfidenceThreshold: (semanticConfidenceThreshold) =>
    set({ semanticConfidenceThreshold }),
  setCooldownMs: (cooldownMs) => set({ cooldownMs }),
  setOnboardingComplete: (onboardingComplete) => set({ onboardingComplete }),
  setSttProvider: (sttProvider) => set({ sttProvider }),
  setSttLanguage: (sttLanguage) => set({ sttLanguage }),
  setLowPowerMode: (lowPowerMode) => set({ lowPowerMode }),
  setActionNotificationsEnabled: (actionNotificationsEnabled) =>
    set({ actionNotificationsEnabled }),
}))

const PERSISTED_KEYS = [
  "audioDeviceId",
  "gain",
  "autoMode",
  "autoPreviewDetections",
  "bibleDetectionEnabled",
  "semanticDetectionEnabled",
  "confidenceThreshold",
  "semanticConfidenceThreshold",
  "cooldownMs",
  "onboardingComplete",
  "sttProvider",
  "sttLanguage",
  "lowPowerMode",
  "actionNotificationsEnabled",
  "deepseekRankingEnabled",
] as const satisfies readonly (keyof SettingsState)[]

function parseSttProvider(value: unknown): SttProvider {
  if (
    value === "deepgram" ||
    value === "soniox" ||
    value === "speechmatics" ||
    value === "vosk"
  ) {
    return value
  }
  // Migrate removed/legacy local providers (whisper, faster-whisper,
  // legacy-whisper, sherpa) to the supported local model.
  return "vosk"
}

function parseSttLanguage(value: unknown): SttLanguage {
  if (value === "es" || value === "fr" || value === "pt") return value
  if (value === "af") return "af"
  return "en"
}

function parseConfidenceThreshold(value: unknown): unknown {
  if (
    typeof value === "number" &&
    (Math.abs(value - LEGACY_DEFAULT_CONFIDENCE_THRESHOLD) < Number.EPSILON ||
      Math.abs(value - LEGACY_AUTO_LIVE_THRESHOLD) < Number.EPSILON)
  ) {
    return DEFAULT_CONFIDENCE_THRESHOLD
  }
  if (typeof value === "number") {
    return normalizeConfidenceThreshold(value)
  }
  return value
}

let tauriStore: Store | null = null
let hydrationPromise: Promise<void> | null = null
let settingsUnsubscribe: (() => void) | null = null
async function getStore(): Promise<Store> {
  if (!tauriStore) {
    tauriStore = await load("settings.json", { autoSave: false, defaults: {} })
  }
  return tauriStore
}

function ensureSettingsPersistenceSubscription() {
  if (settingsUnsubscribe) return

  settingsUnsubscribe = useSettingsStore.subscribe((state, prevState) => {
    const changed = PERSISTED_KEYS.some((k) => state[k] !== prevState[k])
    if (!changed) return
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(() => {
      saveTimer = null
      pendingSave = pendingSave.then(() =>
        persistAll(useSettingsStore.getState())
      )
    }, SAVE_DEBOUNCE_MS)
  })
}

/** Load all persisted settings into the Zustand store. Idempotent and
 *  safe against concurrent callers — the first call owns the work and
 *  subsequent callers await the same promise. */
export function hydrateSettings(): Promise<void> {
  if (!isTauriRuntime()) return Promise.resolve()

  if (hydrationPromise) return hydrationPromise
  hydrationPromise = (async () => {
    try {
      const store = await getStore()
      const patch: Partial<SettingsState> = {}
      for (const key of PERSISTED_KEYS) {
        const value = await store.get(key)
        if (value !== undefined && value !== null) {
          if (key === "sttProvider") {
            patch.sttProvider = parseSttProvider(value)
          } else if (key === "sttLanguage") {
            patch.sttLanguage = parseSttLanguage(value)
          } else if (key === "confidenceThreshold") {
            ;(patch as Record<string, unknown>)[key] =
              parseConfidenceThreshold(value)
          } else {
            ;(patch as Record<string, unknown>)[key] = value
          }
        }
      }

      // Resolve keyring-backed secret presence and write only boolean flags.
      // Best-effort and independent: if a command isn't available (web/dev), keep the default.
      const [deepgram, soniox, speechmatics, deepseek] = await Promise.all([
        invokeTauri<boolean>("has_deepgram_api_key").catch(() => undefined),
        invokeTauri<boolean>("has_soniox_api_key").catch(() => undefined),
        invokeTauri<boolean>("has_speechmatics_api_key").catch(
          () => undefined
        ),
        invokeTauri<boolean>("has_deepseek_api_key").catch(() => undefined),
      ])
      if (deepgram !== undefined) patch.hasDeepgramApiKey = deepgram
      if (soniox !== undefined) patch.hasSonioxApiKey = soniox
      if (speechmatics !== undefined)
        patch.hasSpeechmaticsApiKey = speechmatics
      if (deepseek !== undefined) patch.hasDeepseekApiKey = deepseek

      if (Object.keys(patch).length > 0) {
        useSettingsStore.setState(patch)
      }
      // Attach only after successful hydration so as not to overwrite disk with defaults.
      // Debounce writes, so a dragged slider (e.g. gain) coalesces into a single disk write.
      ensureSettingsPersistenceSubscription()
    } catch {
      console.warn("[settings] Failed to load persisted state, using defaults")
      useBroadcastOutputIssueStore.getState().reportOutputIssue({
        outputId: "global",
        kind: "persistence",
        title: "Settings load failed",
        description: "Could not load saved settings; using defaults.",
        id: "global:persistence:settings-load",
      })
    }
  })()
  return hydrationPromise
}

let saveTimer: ReturnType<typeof setTimeout> | null = null
let pendingSave: Promise<void> = Promise.resolve()
const SAVE_DEBOUNCE_MS = 250

async function persistAll(state: SettingsState): Promise<void> {
  try {
    const store = await getStore()
    for (const key of PERSISTED_KEYS) {
      await store.set(key, state[key] as unknown)
    }
    await store.save()
  } catch {
    console.warn("[settings] Failed to persist settings")
    useBroadcastOutputIssueStore.getState().reportOutputIssue({
      outputId: "global",
      kind: "persistence",
      title: "Settings save failed",
      description: "Could not save settings to disk.",
    })
  }
}
