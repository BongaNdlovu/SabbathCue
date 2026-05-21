// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest"
import React, { act } from "react"
import { createRoot, type Root } from "react-dom/client"

const mockSetLive = vi.fn()
const mockSetLiveVerse = vi.fn()
const mockSelectVerse = vi.fn()
const mockSetReadingModeAutoLive = vi.fn()
const mockSetDetectionPaused = vi.fn().mockResolvedValue(true)
const mockStop = vi.fn()
const mockInvoke = vi.fn().mockResolvedValue(undefined)

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

vi.mock("@/stores/audio-store", () => ({
  useAudioStore: (selector: (s: Record<string, unknown>) => unknown) =>
    selector({ level: { rms: 0, peak: 0 } }),
}))

let broadcastIsLive = false
let broadcastLiveVerse: unknown = null
let broadcastReadingModeAutoLive = false
let transcriptIsTranscribing = false
let bibleSelectedVerse: unknown = null

vi.mock("@/stores/broadcast-store", () => {
  const useBroadcastStore = (selector: (s: Record<string, unknown>) => unknown) =>
    selector({
      isLive: broadcastIsLive,
      liveVerse: broadcastLiveVerse,
      readingModeAutoLive: broadcastReadingModeAutoLive,
      themes: [],
      activeThemeId: "",
    })
  useBroadcastStore.getState = () => ({
    setLive: mockSetLive,
    setLiveVerse: mockSetLiveVerse,
    setReadingModeAutoLive: mockSetReadingModeAutoLive,
  })
  return { useBroadcastStore }
})

vi.mock("@/stores/transcript-store", () => ({
  useTranscriptStore: (selector: (s: Record<string, unknown>) => unknown) =>
    selector({ isTranscribing: transcriptIsTranscribing }),
}))

vi.mock("@/stores/queue-store", () => ({
  useQueueStore: (selector: (s: Record<string, unknown>) => unknown) =>
    selector({ items: [] }),
}))

vi.mock("@/stores/bible-store", () => {
  const useBibleStore = (selector: (s: Record<string, unknown>) => unknown) =>
    selector({
      selectedVerse: bibleSelectedVerse,
    })
  useBibleStore.getState = () => ({ selectVerse: mockSelectVerse })
  return { useBibleStore }
})

vi.mock("@/hooks/use-detection", () => ({
  detectionActions: {
    setDetectionPaused: mockSetDetectionPaused,
    getDetectionControlStatus: () => Promise.resolve({ detection_paused: false }),
  },
}))

vi.mock("@/hooks/use-transcription", () => ({
  transcriptionActions: { stop: mockStop },
}))

vi.mock("@/components/ui/level-meter", () => ({
  LevelMeter: () => React.createElement("div", { "data-testid": "level-meter" }),
}))

function resetState() {
  broadcastIsLive = false
  broadcastLiveVerse = null
  broadcastReadingModeAutoLive = false
  transcriptIsTranscribing = false
  bibleSelectedVerse = null
}

describe("OperatorStatusStrip emergency controls", () => {
  let root: Root | null = null
  let container: HTMLDivElement | null = null

  beforeEach(() => {
    vi.clearAllMocks()
    resetState()
  })

  afterEach(async () => {
    if (root) {
      await act(async () => {
        root?.unmount()
      })
    }
    container?.remove()
    root = null
    container = null
  })

  async function renderStrip() {
    const { OperatorStatusStrip } = await import("./operator-status-strip")
    container = document.createElement("div")
    document.body.appendChild(container)
    root = createRoot(container)
    await act(async () => {
      root?.render(React.createElement(OperatorStatusStrip))
    })
  }

  function getButtonByTitle(title: string): HTMLButtonElement {
    const button = document.querySelector<HTMLButtonElement>(`button[title="${title}"]`)
    expect(button).toBeTruthy()
    return button as HTMLButtonElement
  }

  async function click(button: HTMLButtonElement) {
    await act(async () => {
      button.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }))
    })
  }

  describe("Hide Live Output", () => {
    it("is disabled when not live", async () => {
      broadcastIsLive = false
      await renderStrip()
      const btn = getButtonByTitle("Hide Live Output")
      expect(btn.disabled).toBe(true)
    })

    it("is enabled when live and calls setLive(false) when clicked", async () => {
      broadcastIsLive = true
      broadcastLiveVerse = { reference: "John 3:16" }
      await renderStrip()
      const btn = getButtonByTitle("Hide Live Output")
      expect(btn.disabled).toBe(false)
      await click(btn)
      expect(mockSetLive).toHaveBeenCalledWith(false)
    })
  })

  describe("Stop Transcription", () => {
    it("is disabled when not transcribing", async () => {
      transcriptIsTranscribing = false
      await renderStrip()
      const btn = getButtonByTitle("Stop Transcription")
      expect(btn.disabled).toBe(true)
    })

    it("is enabled when transcribing and calls stop()", async () => {
      transcriptIsTranscribing = true
      broadcastLiveVerse = { reference: "John 3:16" }
      await renderStrip()
      const btn = getButtonByTitle("Stop Transcription")
      expect(btn.disabled).toBe(false)
      await click(btn)
      expect(mockStop).toHaveBeenCalled()
    })
  })

  describe("Clear Live Output", () => {
    it("is disabled when no live verse", async () => {
      broadcastLiveVerse = null
      await renderStrip()
      const btn = getButtonByTitle("Clear Live Output")
      expect(btn.disabled).toBe(true)
    })

    it("clears liveVerse and hides output when clicked", async () => {
      broadcastLiveVerse = { reference: "John 3:16" }
      broadcastIsLive = true
      await renderStrip()
      const btn = getButtonByTitle("Clear Live Output")
      expect(btn.disabled).toBe(false)
      await click(btn)
      expect(mockSetLiveVerse).toHaveBeenCalledWith(null)
      expect(mockSetLive).toHaveBeenCalledWith(false)
    })
  })

  describe("Clear Preview", () => {
    it("is disabled when no preview verse", async () => {
      bibleSelectedVerse = null
      await renderStrip()
      const btn = getButtonByTitle("Clear Preview")
      expect(btn.disabled).toBe(true)
    })

    it("calls selectVerse(null) when clicked", async () => {
      bibleSelectedVerse = { book_number: 1, chapter: 1, verse: 1 }
      await renderStrip()
      const btn = getButtonByTitle("Clear Preview")
      expect(btn.disabled).toBe(false)
      await click(btn)
      expect(mockSelectVerse).toHaveBeenCalledWith(null)
    })
  })

  describe("Pause Auto-Live", () => {
    it("is disabled when auto-live is off", async () => {
      broadcastReadingModeAutoLive = false
      await renderStrip()
      const btn = getButtonByTitle("Pause Auto-Live")
      expect(btn.disabled).toBe(true)
    })

    it("sets readingModeAutoLive false and invokes stop_reading_mode", async () => {
      broadcastReadingModeAutoLive = true
      await renderStrip()
      const btn = getButtonByTitle("Pause Auto-Live")
      expect(btn.disabled).toBe(false)
      await click(btn)
      expect(mockSetReadingModeAutoLive).toHaveBeenCalledWith(false)
      expect(mockInvoke).toHaveBeenCalledWith("stop_reading_mode")
    })
  })

  describe("Pause/Resume Suggestions", () => {
    it("calls setDetectionPaused(true) when Pause Suggestions is clicked", async () => {
      await renderStrip()
      await click(getButtonByTitle("Pause Suggestions"))
      expect(mockSetDetectionPaused).toHaveBeenCalledWith(true)
    })

    it("calls setDetectionPaused(false) when Resume Suggestions is clicked", async () => {
      await renderStrip()
      await click(getButtonByTitle("Pause Suggestions"))
      const resumeButton = getButtonByTitle("Resume Suggestions")
      await click(resumeButton)
      expect(mockSetDetectionPaused).toHaveBeenCalledWith(false)
    })
  })
})
