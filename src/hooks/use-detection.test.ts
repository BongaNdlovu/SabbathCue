import { beforeEach, describe, expect, it, vi } from "vitest"

const invokeMock = vi.fn()
const reportOutputIssueMock = vi.fn()
const addDetectionsMock = vi.fn()

function makeDetection(source: "direct" | "semantic") {
  return {
    verse_ref: source === "direct" ? "John 3:16" : "Romans 8:28",
    verse_text:
      source === "direct"
        ? "For God so loved the world"
        : "All things work together for good",
    book_name: source === "direct" ? "John" : "Romans",
    book_number: source === "direct" ? 43 : 45,
    chapter: source === "direct" ? 3 : 8,
    verse: source === "direct" ? 16 : 28,
    confidence: source === "direct" ? 0.96 : 0.72,
    source,
    auto_queued: source === "direct",
    transcript_snippet:
      source === "direct" ? "John three sixteen" : "works together for good",
    is_chapter_only: false,
  }
}

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

vi.mock("@/stores/detection-store", () => ({
  useDetectionStore: {
    getState: () => ({
      addDetections: addDetectionsMock,
      clearDetections: vi.fn(),
      removeDetection: vi.fn(),
      detections: [],
    }),
  },
}))

describe("useDetection manual detection failures", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    reportOutputIssueMock.mockReset()
    addDetectionsMock.mockReset()
    vi.resetModules()
  })

  it("reports a manual-detection issue and returns an empty result on failure", async () => {
    invokeMock.mockRejectedValueOnce(new Error("backend offline"))
    const { detectionActions } = await import("./use-detection")

    const results = await detectionActions.detectVerses("John 3:16")

    expect(results).toEqual([])
    expect(addDetectionsMock).not.toHaveBeenCalled()
    expect(reportOutputIssueMock).toHaveBeenCalledWith(
      expect.objectContaining({
        outputId: "global",
        kind: "manual-detection",
        title: "Detection failed",
      })
    )
  })

  it("adds backend direct and semantic detections on manual detection success", async () => {
    const backendResults = [makeDetection("direct"), makeDetection("semantic")]
    invokeMock.mockResolvedValueOnce(backendResults)
    const { detectionActions } = await import("./use-detection")

    const results = await detectionActions.detectVerses(
      "John 3:16 and a promise about all things working together"
    )

    expect(results).toBe(backendResults)
    expect(invokeMock).toHaveBeenCalledWith("detect_verses", {
      text: "John 3:16 and a promise about all things working together",
    })
    expect(addDetectionsMock).toHaveBeenCalledWith(backendResults)
    expect(reportOutputIssueMock).not.toHaveBeenCalled()
  })

  it("returns detection model status flags from the backend", async () => {
    invokeMock.mockResolvedValueOnce({
      has_direct: true,
      has_semantic: true,
      paraphrase_enabled: false,
      bible_detection_enabled: true,
      semantic_detection_enabled: true,
    })
    const { detectionActions } = await import("./use-detection")

    await expect(detectionActions.getDetectionStatus()).resolves.toEqual({
      has_direct: true,
      has_semantic: true,
      paraphrase_enabled: false,
      bible_detection_enabled: true,
      semantic_detection_enabled: true,
    })
    expect(invokeMock).toHaveBeenCalledWith("detection_status")
  })
})
