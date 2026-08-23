import type {
  DetectionResult,
  PresentationRenderData,
  ReadingAdvance,
  Verse,
} from "@/types"

export const WORKFLOW_TRACE_LIMIT = 500

export type WorkflowTraceStage =
  | "transcription.connected"
  | "transcription.disconnected"
  | "transcription.error"
  | "transcription.partial"
  | "transcription.final"
  | "queue.voice"
  | "detection.event"
  | "detection.batch"
  | "detection.preview.selected"
  | "detection.preview.skipped"
  | "detection.queue.skipped"
  | "detection.queue.added"
  | "detection.ai.suggested"
  | "reading.event"
  | "reading.ignored"
  | "reading.accepted"
  | "preview.selected"
  | "preview.state"
  | "live.auto_commit"
  | "live.commit"
  | "live.digit_growth_hold"
  | "live.state"

export interface WorkflowTraceEntry {
  id: number
  at: number
  stage: WorkflowTraceStage
  summary: string
  details?: Record<string, unknown>
}

export interface WorkflowTraceApi {
  entries: () => WorkflowTraceEntry[]
  clear: () => void
  exportJson: () => string
  /** Override the cached console-trace flag without reloading the app. */
  setFlag: (value: boolean) => void
}

declare global {
  interface Window {
    __SABBATHCUE_WORKFLOW_TRACE__?: WorkflowTraceApi
  }
}

const entries: WorkflowTraceEntry[] = []
let nextId = 1

function trimText(value: string, maxLength = 180): string {
  if (value.length <= maxLength) return value
  return `${value.slice(0, maxLength - 3)}...`
}

// The trace flag is checked on every transcript partial (the highest-frequency
// event in the app). URL parsing + a synchronous localStorage read per event
// added avoidable main-thread work, so the result is cached after the first
// check. `window.__SABBATHCUE_WORKFLOW_TRACE__.setFlag()` re-reads it when a
// session flips the flag without a reload.
let consoleTraceCache: boolean | null = null

function shouldConsoleTrace(): boolean {
  if (typeof window === "undefined") return false
  if (consoleTraceCache !== null) return consoleTraceCache

  const params = new URLSearchParams(window.location.search)
  let enabled: boolean
  if (params.has("workflowTrace") || params.has("e2e")) {
    enabled = true
  } else {
    try {
      enabled = window.localStorage.getItem("sabbathcue.workflowTrace") === "1"
    } catch {
      enabled = false
    }
  }
  consoleTraceCache = enabled
  return enabled
}

function installWindowTraceApi(): void {
  if (typeof window === "undefined") return
  window.__SABBATHCUE_WORKFLOW_TRACE__ ??= {
    entries: getWorkflowTrace,
    clear: clearWorkflowTrace,
    exportJson: exportWorkflowTraceJson,
    setFlag: (value: boolean) => {
      consoleTraceCache = value
    },
  }
}

export function recordWorkflowTrace(
  stage: WorkflowTraceStage,
  summary: string,
  details?: Record<string, unknown>
): WorkflowTraceEntry {
  const entry: WorkflowTraceEntry = {
    id: nextId,
    at: Date.now(),
    stage,
    summary,
    details,
  }
  nextId += 1

  entries.push(entry)
  if (entries.length > WORKFLOW_TRACE_LIMIT) {
    entries.splice(0, entries.length - WORKFLOW_TRACE_LIMIT)
  }

  installWindowTraceApi()
  if (shouldConsoleTrace()) {
    console.info("[workflow-trace]", entry)
  }

  return entry
}

export function getWorkflowTrace(): WorkflowTraceEntry[] {
  return [...entries]
}

export function clearWorkflowTrace(): void {
  entries.length = 0
  nextId = 1
  installWindowTraceApi()
}

export function exportWorkflowTraceJson(): string {
  return JSON.stringify(getWorkflowTrace(), null, 2)
}

export function traceTranscriptDetails(input: {
  text?: string
  confidence?: number
  isFinal?: boolean
  wordCount?: number
}): Record<string, unknown> {
  return {
    text: input.text ? trimText(input.text) : "",
    confidence: input.confidence,
    isFinal: input.isFinal,
    wordCount:
      input.wordCount ?? input.text?.split(/\s+/).filter(Boolean).length ?? 0,
  }
}

export function traceDetectionDetails(
  detection: DetectionResult
): Record<string, unknown> {
  return {
    reference: detection.verse_ref,
    source: detection.source,
    confidence: Number(detection.confidence.toFixed(3)),
    autoQueued: detection.auto_queued,
    contentType: detection.content_type ?? "bible",
    isChapterOnly: detection.is_chapter_only,
    snippet: trimText(detection.transcript_snippet),
  }
}

export function traceDetectionBatchDetails(
  detections: DetectionResult[]
): Record<string, unknown> {
  return {
    count: detections.length,
    top: detections.slice(0, 3).map(traceDetectionDetails),
  }
}

export function traceReadingAdvanceDetails(
  advance: ReadingAdvance
): Record<string, unknown> {
  return {
    reference: advance.reference,
    bookNumber: advance.book_number,
    bookName: advance.book_name,
    chapter: advance.chapter,
    verse: advance.verse,
    confidence: Number(advance.confidence.toFixed(3)),
    text: trimText(advance.verse_text),
  }
}

export function traceVerseDetails(verse: Verse): Record<string, unknown> {
  return {
    reference: `${verse.book_name} ${verse.chapter}:${verse.verse}`,
    bookNumber: verse.book_number,
    bookName: verse.book_name,
    chapter: verse.chapter,
    verse: verse.verse,
    translationId: verse.translation_id,
    text: trimText(verse.text),
  }
}

export function tracePresentationDetails(
  item: PresentationRenderData | null
): Record<string, unknown> {
  if (!item) {
    return { reference: null, kind: null, segmentCount: 0 }
  }

  return {
    reference: item.reference,
    kind: item.kind ?? "unknown",
    segmentCount: item.segments.length,
    firstSegment: item.segments[0]?.text ? trimText(item.segments[0].text) : "",
  }
}

installWindowTraceApi()
