import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Verse } from "@/types"

const emitToMock = vi.fn()

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: emitToMock,
}))

const sampleVerse: Verse = {
  id: 1,
  translation_id: 1,
  book_number: 43,
  book_name: "John",
  book_abbreviation: "John",
  chapter: 3,
  verse: 16,
  text: "For God so loved the world.",
}

describe("presentation workflow", () => {
  beforeEach(async () => {
    emitToMock.mockReset()
    emitToMock.mockResolvedValue(undefined)
    vi.resetModules()
  })

  it("selectPreviewVerse only updates the Bible preview", async () => {
    const { useBibleStore } = await import("@/stores/bible-store")
    const { selectPreviewVerse } = await import("./presentation-workflow")

    selectPreviewVerse(sampleVerse)

    expect(useBibleStore.getState().selectedVerse).toMatchObject({
      book_name: "John",
      chapter: 3,
      verse: 16,
    })
    expect(emitToMock).not.toHaveBeenCalled()
  })

  it("commitPreviewToLive sends the selected verse live", async () => {
    const { useBibleStore } = await import("@/stores/bible-store")
    const { useBroadcastStore } = await import("@/stores/broadcast-store")
    const { toVerseRenderData } = await import("@/hooks/use-broadcast")
    const { commitPreviewToLive } = await import("./presentation-workflow")

    useBibleStore.setState({
      selectedVerse: sampleVerse,
      translations: [
        {
          id: 1,
          abbreviation: "KJV",
          title: "King James Version",
          language: "en",
          is_copyrighted: false,
          is_downloaded: true,
        },
      ],
      activeTranslationId: 1,
    })

    const committed = commitPreviewToLive()
    const previewPayload = toVerseRenderData(sampleVerse, "KJV")

    expect(committed).toBe(true)
    expect(useBroadcastStore.getState().isLive).toBe(true)
    expect(useBroadcastStore.getState().liveVerse).toEqual(previewPayload)
    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({
        verse: previewPayload,
      }),
    )
  })

  it("commitPreviewToLive returns false when no verse is staged", async () => {
    const { useBibleStore } = await import("@/stores/bible-store")
    const { commitPreviewToLive } = await import("./presentation-workflow")

    useBibleStore.setState({ selectedVerse: null })

    expect(commitPreviewToLive()).toBe(false)
  })
})
