import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  handleHymnVoiceControl,
  parseHymnCommand,
  resetHymnVoiceControlState,
  shouldSuppressDuplicateHymnCommand,
} from "./hymn-voice-control"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"

const selectPreviewItemMock = vi.fn()
const getHymnByNumberMock = vi.fn()
const generateHymnScreensMock = vi.fn()
const addRecentHymnMock = vi.fn()

vi.mock("@/lib/presentation-workflow", () => ({
  selectPreviewItem: (...args: unknown[]) => selectPreviewItemMock(...args),
}))

vi.mock("@/services/hymnal/hymnal-repository", () => ({
  getHymnByNumber: (...args: unknown[]) => getHymnByNumberMock(...args),
}))

vi.mock("@/services/hymnal/generate-hymn-screens", () => ({
  generateHymnScreens: (...args: unknown[]) => generateHymnScreensMock(...args),
}))

vi.mock("@/services/hymnal/hymnal-history", () => ({
  addRecentHymn: (...args: unknown[]) => addRecentHymnMock(...args),
}))

const sampleHymn = {
  id: "sda-12",
  number: 12,
  title: "Joyful, Joyful, We Adore Thee",
  sections: [{ id: "v1", label: "Verse 1", kind: "verse" as const, lines: ["Line one"] }],
}

const sampleScreen = {
  id: "v1-screen-1",
  hymnId: "sda-12",
  hymnNumber: 12,
  hymnTitle: "Joyful, Joyful, We Adore Thee",
  sectionId: "v1",
  sectionLabel: "Verse 1",
  sectionKind: "verse" as const,
  screenIndex: 0,
  sectionScreenIndex: 0,
  sectionScreenCount: 1,
  totalScreens: 1,
  lines: ["Line one"],
}

const secondSampleScreen = {
  ...sampleScreen,
  id: "v1-screen-2",
  screenIndex: 1,
  sectionScreenIndex: 1,
  sectionScreenCount: 2,
  totalScreens: 2,
  lines: ["Line two"],
}

describe("hymn voice control", () => {
  beforeEach(() => {
    resetHymnVoiceControlState()
    selectPreviewItemMock.mockReset()
    getHymnByNumberMock.mockReset()
    generateHymnScreensMock.mockReset()
    addRecentHymnMock.mockReset()
    useHymnSlideStore.getState().setDeck([], 0)
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
    resetHymnVoiceControlState()
    useHymnSlideStore.getState().setDeck([], 0)
  })

  describe("parseHymnCommand", () => {
    it("accepts explicit hymn commands with digits", () => {
      expect(parseHymnCommand("hymn 12")).toBe(12)
      expect(parseHymnCommand("song 251")).toBe(251)
      expect(parseHymnCommand("SDA hymn 100")).toBe(100)
      expect(parseHymnCommand("hymn number 12")).toBe(12)
    })

    it("accepts spoken hymn numbers", () => {
      expect(parseHymnCommand("hymn twelve")).toBe(12)
      expect(parseHymnCommand("song two hundred fifty one")).toBe(251)
    })

    it("rejects bare numbers and scripture-like text", () => {
      expect(parseHymnCommand("12")).toBeNull()
      expect(parseHymnCommand("John 3 16")).toBeNull()
      expect(parseHymnCommand("chapter 3 verse 16")).toBeNull()
    })

    it("rejects out-of-range hymn numbers before loading data", () => {
      expect(parseHymnCommand("hymn 9999")).toBeNull()
    })
  })

  describe("handleHymnVoiceControl", () => {
    it("stages the full hymn deck and first screen in preview for valid commands", async () => {
      getHymnByNumberMock.mockResolvedValue(sampleHymn)
      generateHymnScreensMock.mockReturnValue([sampleScreen, secondSampleScreen])

      const handled = await handleHymnVoiceControl("please open hymn 12")

      expect(handled).toBe(true)
      expect(getHymnByNumberMock).toHaveBeenCalledWith(12)
      const hymnSlides = useHymnSlideStore.getState()
      expect(hymnSlides.deck).toHaveLength(2)
      expect(hymnSlides.activeIndex).toBe(0)
      expect(selectPreviewItemMock).toHaveBeenCalledWith(
        expect.objectContaining({
          kind: "hymn",
          hymnNumber: 12,
          screenId: "v1-screen-1",
        }),
      )
      expect(addRecentHymnMock).toHaveBeenCalledWith("sda-12")
    })

    it("leaves preview unchanged when hymn lookup fails", async () => {
      getHymnByNumberMock.mockResolvedValue(null)

      const handled = await handleHymnVoiceControl("hymn 12")

      expect(handled).toBe(false)
      expect(selectPreviewItemMock).not.toHaveBeenCalled()
    })

    it("leaves preview unchanged when screen generation is empty", async () => {
      getHymnByNumberMock.mockResolvedValue(sampleHymn)
      generateHymnScreensMock.mockReturnValue([])

      const handled = await handleHymnVoiceControl("hymn 12")

      expect(handled).toBe(false)
      expect(selectPreviewItemMock).not.toHaveBeenCalled()
    })

    it("does not load hymnal data when no explicit command is present", async () => {
      const handled = await handleHymnVoiceControl("John 3 16")

      expect(handled).toBe(false)
      expect(getHymnByNumberMock).not.toHaveBeenCalled()
    })

    it("suppresses duplicate commands inside the dedupe window", async () => {
      getHymnByNumberMock.mockResolvedValue(sampleHymn)
      generateHymnScreensMock.mockReturnValue([sampleScreen])

      expect(await handleHymnVoiceControl("hymn 12")).toBe(true)
      expect(await handleHymnVoiceControl("song 12")).toBe(false)
      expect(selectPreviewItemMock).toHaveBeenCalledTimes(1)
    })

    it("allows a different hymn after a duplicate command", async () => {
      getHymnByNumberMock.mockImplementation(async (number: number) =>
        number === 12 ? sampleHymn : { ...sampleHymn, id: "sda-13", number: 13 },
      )
      generateHymnScreensMock.mockReturnValue([sampleScreen])

      expect(await handleHymnVoiceControl("hymn 12")).toBe(true)
      expect(await handleHymnVoiceControl("hymn 13")).toBe(true)
      expect(selectPreviewItemMock).toHaveBeenCalledTimes(2)
    })
  })

  describe("shouldSuppressDuplicateHymnCommand", () => {
    it("expires duplicate suppression after the dedupe window", async () => {
      getHymnByNumberMock.mockResolvedValue(sampleHymn)
      generateHymnScreensMock.mockReturnValue([sampleScreen])

      await handleHymnVoiceControl("hymn 12")
      expect(shouldSuppressDuplicateHymnCommand(12)).toBe(true)

      vi.advanceTimersByTime(5001)
      expect(shouldSuppressDuplicateHymnCommand(12)).toBe(false)
    })
  })
})
