import { afterEach, describe, expect, it, vi } from "vitest"
import {
  DASHBOARD_LAYOUT_PRESETS,
  DASHBOARD_LAYOUT_STORAGE_KEY,
  clampNumber,
  loadDashboardLayoutState,
  normalizeDashboardLayoutState,
  saveDashboardLayoutState,
  type DashboardViewMode,
} from "./dashboard-layout"

describe("dashboard layout helpers", () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it("clamps values to the configured range", () => {
    expect(clampNumber(10, 20, 40)).toBe(20)
    expect(clampNumber(30, 20, 40)).toBe(30)
    expect(clampNumber(50, 20, 40)).toBe(40)
  })

  it("defines complete presets for every view mode", () => {
    const modes: DashboardViewMode[] = ["balanced", "broadcast", "study"]

    for (const mode of modes) {
      const preset = DASHBOARD_LAYOUT_PRESETS[mode]
      expect(preset.topHeightPercent).toBeGreaterThanOrEqual(34)
      expect(preset.topHeightPercent).toBeLessThanOrEqual(68)
      expect(preset.transcriptWidth).toBeGreaterThanOrEqual(240)
      expect(preset.queueWidth).toBeGreaterThanOrEqual(240)
      expect(preset.detectionsWidth).toBeGreaterThanOrEqual(360)
    }
  })

  it("normalizes invalid persisted layout values", () => {
    expect(
      normalizeDashboardLayoutState({
        viewMode: "broadcast",
        topHeightPercent: 90,
        transcriptWidth: 100,
        queueWidth: 900,
        detectionsWidth: 10,
      })
    ).toEqual({
      viewMode: "broadcast",
      topHeightPercent: 68,
      transcriptWidth: 240,
      queueWidth: 520,
      detectionsWidth: 360,
    })
  })

  it("saves and loads dashboard layout from localStorage", () => {
    const storage = new Map<string, string>()
    vi.stubGlobal("window", {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
      },
    })

    saveDashboardLayoutState({
      viewMode: "study",
      topHeightPercent: 42,
      transcriptWidth: 360,
      queueWidth: 280,
      detectionsWidth: 420,
    })

    expect(storage.has(DASHBOARD_LAYOUT_STORAGE_KEY)).toBe(true)
    expect(loadDashboardLayoutState()).toEqual({
      viewMode: "study",
      topHeightPercent: 42,
      transcriptWidth: 360,
      queueWidth: 280,
      detectionsWidth: 420,
    })
  })
})
