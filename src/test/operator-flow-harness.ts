import {
  createPresentationItem,
  createScriptureQueueItem,
  detectionToVerse,
  presentItem,
  presentVerse,
  selectPreviewVerse,
} from "@/lib/presentation-workflow"
import {
  clearWorkflowTrace,
  exportWorkflowTraceJson,
  getWorkflowTrace,
  type WorkflowTraceEntry,
} from "@/lib/workflow-trace"
import { useBroadcastStore } from "@/stores/broadcast-store"
import { useDetectionStore } from "@/stores/detection-store"
import { useQueueStore } from "@/stores/queue-store"
import { useSettingsStore } from "@/stores/settings-store"
import { useTranscriptStore } from "@/stores/transcript-store"
import type {
  BroadcastTheme,
  DetectionResult,
  PresentationRenderData,
  QueueItem,
  ReadingAdvance,
} from "@/types"

export interface OperatorBroadcastPayload {
  theme: BroadcastTheme
  item: PresentationRenderData | null
  opacity?: number
}

export interface OperatorFlowSnapshot {
  queueLength: number
  activeIndex: number | null
  previewReference: string | null
  liveReference: string | null
  isLive: boolean
  activeThemeId: string
  detectionCount: number
  connectionStatus: string
  transcriptPartial: string
  lastTranscriptFinal: string | null
}

export interface OperatorFlowTimelineEntry {
  event: string
  payload: unknown
  at: number
}

type TranscriptReplayPayload = {
  text: string
  is_final: boolean
  confidence: number
  words: {
    text: string
    start: number
    end: number
    confidence: number
    punctuated: string
  }[]
}

declare global {
  interface Window {
    __SABBATHCUE_EVENT_TAP__?: (event: string, payload: unknown) => void
    __SABBATHCUE_OPERATOR_E2E__?: {
      queue: {
        addItems: (items: QueueItem[]) => void
        setActive: (index: number | null) => void
        clear: () => void
        getLength: () => number
        getActiveIndex: () => number | null
      }
      detection: {
        add: (detection: DetectionResult) => void
        previewFromDetection: (detection: DetectionResult) => void
        queueFromDetection: (detection: DetectionResult) => void
      }
      settings: {
        setAutoMode: (autoMode: boolean) => void
        setSemanticDetectionEnabled: (enabled: boolean) => void
        setConfidenceThreshold: (confidenceThreshold: number) => void
        setSemanticConfidenceThreshold: (
          semanticConfidenceThreshold: number
        ) => void
      }
      transcription: {
        connect: () => void
        disconnect: () => void
        partial: (text: string, confidence?: number) => void
        final: (text: string, confidence?: number) => void
        detections: (detections: DetectionResult[]) => void
        readingAdvance: (advance: ReadingAdvance) => void
        timeline: () => OperatorFlowTimelineEntry[]
        clearTimeline: () => void
      }
      remote: {
        next: () => void
        prev: () => void
        show: () => void
        hide: () => void
        setTheme: (name: string) => boolean
        setOpacity: (value: number) => void
      }
      live: {
        goLive: () => void
        hide: () => void
        isLive: () => boolean
      }
      theme: {
        add: (theme: BroadcastTheme) => void
        setActive: (themeId: string) => void
        getActiveId: () => string
        listNames: () => string[]
      }
      workflowTrace: {
        entries: () => WorkflowTraceEntry[]
        stages: () => string[]
        clear: () => void
        exportJson: () => string
      }
      broadcast: {
        getPreview: () => PresentationRenderData | null
        getLive: () => PresentationRenderData | null
        getPayload: () => OperatorBroadcastPayload
        setPreview: (item: PresentationRenderData | null) => void
        setLive: (
          item: PresentationRenderData | null,
          options?: { makeLive?: boolean }
        ) => void
        applyPayload: (payload: OperatorBroadcastPayload) => void
      }
      snapshot: () => OperatorFlowSnapshot
    }
  }
}

const replayTimeline: OperatorFlowTimelineEntry[] = []

function dispatchReplayEvent(event: string, payload?: unknown) {
  window.dispatchEvent(
    new CustomEvent(`sabbathcue:e2e:${event}`, { detail: payload })
  )
}

function transcriptPayload(
  text: string,
  isFinal: boolean,
  confidence = 0.95
): TranscriptReplayPayload {
  return {
    text,
    is_final: isFinal,
    confidence,
    words: text
      .split(/\s+/)
      .filter(Boolean)
      .map((word, index) => ({
        text: word,
        start: index * 0.25,
        end: index * 0.25 + 0.2,
        confidence,
        punctuated: word,
      })),
  }
}

function getActiveTheme(): BroadcastTheme {
  const broadcast = useBroadcastStore.getState()
  return (
    broadcast.themes.find((theme) => theme.id === broadcast.activeThemeId) ??
    broadcast.themes[0]
  )
}

function buildBroadcastPayload(): OperatorBroadcastPayload {
  const broadcast = useBroadcastStore.getState()
  return {
    theme: getActiveTheme(),
    item: broadcast.previewItem ?? broadcast.liveItem,
    opacity: broadcast.opacity,
  }
}

function snapshot(): OperatorFlowSnapshot {
  const queue = useQueueStore.getState()
  const broadcast = useBroadcastStore.getState()
  const detection = useDetectionStore.getState()
  const transcript = useTranscriptStore.getState()
  const lastSegment = transcript.segments.at(-1)

  return {
    queueLength: queue.items.length,
    activeIndex: queue.activeIndex,
    previewReference: broadcast.previewItem?.reference ?? null,
    liveReference: broadcast.liveItem?.reference ?? null,
    isLive: broadcast.isLive,
    activeThemeId: broadcast.activeThemeId,
    detectionCount: detection.detections.length,
    connectionStatus: transcript.connectionStatus,
    transcriptPartial: transcript.currentPartial,
    lastTranscriptFinal: lastSegment?.text ?? null,
  }
}

function navigateQueue(direction: "next" | "prev") {
  const queue = useQueueStore.getState()
  if (queue.items.length === 0) return

  const current =
    queue.activeIndex ??
    (() => {
      const liveReference = useBroadcastStore.getState().liveItem?.reference
      if (!liveReference) return 0
      const index = queue.items.findIndex(
        (item) => item.presentation.reference === liveReference
      )
      return index >= 0 ? index : 0
    })()

  const nextIndex =
    direction === "next"
      ? Math.min(current + 1, queue.items.length - 1)
      : Math.max(current - 1, 0)

  queue.setActive(nextIndex)
  const item = queue.items[nextIndex]
  if (!item) return
  if (item.presentation.kind === "scripture") {
    presentVerse(item.presentation.verse, { navigate: false })
  } else {
    presentItem(item.presentation, { navigate: false })
  }
}

export function installOperatorFlowHarness(): void {
  if (typeof window === "undefined") return
  if (!new URLSearchParams(window.location.search).has("e2e")) return

  window.__SABBATHCUE_EVENT_TAP__ = (event, payload) => {
    replayTimeline.push({ event, payload, at: Date.now() })
  }

  window.__SABBATHCUE_OPERATOR_E2E__ = {
    queue: {
      addItems: (items) => useQueueStore.getState().addItems(items),
      setActive: (index) => useQueueStore.getState().setActive(index),
      clear: () => useQueueStore.getState().clearQueue(),
      getLength: () => useQueueStore.getState().items.length,
      getActiveIndex: () => useQueueStore.getState().activeIndex,
    },
    detection: {
      add: (detection) => useDetectionStore.getState().addDetection(detection),
      previewFromDetection: (detection) => {
        selectPreviewVerse(detectionToVerse(detection), { navigate: false })
      },
      queueFromDetection: (detection) => {
        const verse = detectionToVerse(detection)
        useQueueStore.getState().addOrFlashItem(
          createScriptureQueueItem(verse, {
            reference: detection.verse_ref,
            confidence: detection.confidence,
            source: detection.source === "direct" ? "ai-direct" : "ai-semantic",
          })
        )
      },
    },
    settings: {
      setAutoMode: (autoMode) =>
        useSettingsStore.getState().setAutoMode(autoMode),
      setSemanticDetectionEnabled: (enabled) =>
        useSettingsStore.getState().setSemanticDetectionEnabled(enabled),
      setConfidenceThreshold: (confidenceThreshold) =>
        useSettingsStore.getState().setConfidenceThreshold(confidenceThreshold),
      setSemanticConfidenceThreshold: (semanticConfidenceThreshold) =>
        useSettingsStore
          .getState()
          .setSemanticConfidenceThreshold(semanticConfidenceThreshold),
    },
    transcription: {
      connect: () => dispatchReplayEvent("stt_connected"),
      disconnect: () => dispatchReplayEvent("stt_disconnected"),
      partial: (text, confidence) =>
        dispatchReplayEvent(
          "transcript_partial",
          transcriptPayload(text, false, confidence)
        ),
      final: (text, confidence) =>
        dispatchReplayEvent(
          "transcript_final",
          transcriptPayload(text, true, confidence)
        ),
      detections: (detections) =>
        dispatchReplayEvent("verse_detections", detections),
      readingAdvance: (advance) =>
        dispatchReplayEvent("reading_mode_verse", advance),
      timeline: () => [...replayTimeline],
      clearTimeline: () => {
        replayTimeline.length = 0
      },
    },
    remote: {
      next: () => navigateQueue("next"),
      prev: () => navigateQueue("prev"),
      show: () => useBroadcastStore.getState().setLive(true),
      hide: () => useBroadcastStore.getState().setLive(false),
      setTheme: (name) => {
        const theme = useBroadcastStore
          .getState()
          .themes.find(
            (entry) => entry.name.toLowerCase() === name.toLowerCase()
          )
        if (!theme) return false
        useBroadcastStore.getState().setActiveTheme(theme.id)
        return true
      },
      setOpacity: (value) => useBroadcastStore.getState().setOpacity(value),
    },
    live: {
      goLive: () => useBroadcastStore.getState().setLive(true),
      hide: () => useBroadcastStore.getState().setLive(false),
      isLive: () => useBroadcastStore.getState().isLive,
    },
    theme: {
      add: (theme) => useBroadcastStore.getState().saveTheme(theme),
      setActive: (themeId) =>
        useBroadcastStore.getState().setActiveTheme(themeId),
      getActiveId: () => useBroadcastStore.getState().activeThemeId,
      listNames: () =>
        useBroadcastStore.getState().themes.map((theme) => theme.name),
    },
    workflowTrace: {
      entries: getWorkflowTrace,
      stages: () => getWorkflowTrace().map((entry) => entry.stage),
      clear: clearWorkflowTrace,
      exportJson: exportWorkflowTraceJson,
    },
    broadcast: {
      getPreview: () => useBroadcastStore.getState().previewItem,
      getLive: () => useBroadcastStore.getState().liveItem,
      getPayload: () => buildBroadcastPayload(),
      setPreview: (item) => useBroadcastStore.getState().setPreviewItem(item),
      setLive: (item, options) => {
        if (!item) {
          useBroadcastStore.getState().setLiveItem(null)
          return
        }
        useBroadcastStore.getState().commitLiveItem(item, options)
      },
      applyPayload: (payload) => {
        useBroadcastStore.getState().setActiveTheme(payload.theme.id)
        useBroadcastStore.getState().setPreviewItem(payload.item)
        if (payload.opacity !== undefined) {
          useBroadcastStore.getState().setOpacity(payload.opacity)
        }
        if (payload.item) {
          useBroadcastStore
            .getState()
            .commitLiveItem(payload.item, { makeLive: true })
        }
      },
    },
    snapshot,
  }
}

export function seedOperatorQueue(items: QueueItem[]): void {
  useQueueStore.getState().clearQueue()
  useQueueStore.getState().addItems(items)
}

export function makeHarnessDetection(reference = "John 3:16"): DetectionResult {
  return {
    verse_ref: `${reference} (KJV)`,
    book_number: 43,
    book_name: "John",
    chapter: 3,
    verse: 16,
    verse_text: "For God so loved the world.",
    confidence: 0.95,
    source: "direct",
    auto_queued: true,
    transcript_snippet: "For God so loved the world",
    is_chapter_only: false,
    authorization: "reading-authorized",
    job: "citation",
  }
}

export function makeHarnessQueueItem(
  reference = "John 3:16 (KJV)",
  verse = 16
): QueueItem {
  return createScriptureQueueItem(
    createPresentationItem({
      id: verse,
      translation_id: 1,
      book_number: 43,
      book_name: "John",
      book_abbreviation: "Jn",
      chapter: 3,
      verse,
      text:
        verse === 16
          ? "For God so loved the world."
          : "For God sent not his Son.",
    }).verse,
    { reference }
  )
}
