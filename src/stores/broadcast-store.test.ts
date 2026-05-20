import { beforeEach, describe, expect, it, vi } from "vitest"

const emitToMock = vi.fn()

vi.mock("@tauri-apps/api/event", () => ({
  emitTo: emitToMock,
}))

describe("broadcast store sync", () => {
  beforeEach(async () => {
    emitToMock.mockReset()
    emitToMock.mockResolvedValue(undefined)
    vi.resetModules()
  })

  it("syncBroadcastOutput emits current theme and verse to broadcast window", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")
    const theme = useBroadcastStore.getState().themes[0]
    useBroadcastStore.setState({
      activeThemeId: theme.id,
      isLive: true,
      liveVerse: {
        reference: "John 3:16",
        segments: [{ text: "For God so loved the world", verseNumber: 16 }],
      },
    })

    emitToMock.mockClear()
    useBroadcastStore.getState().syncBroadcastOutput()

    expect(emitToMock).toHaveBeenCalledTimes(2)
    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({
        theme: expect.objectContaining({ id: theme.id }),
        verse: expect.objectContaining({ reference: "John 3:16" }),
      }),
    )
    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast-alt",
      "broadcast:verse-update",
      expect.objectContaining({
        theme: expect.objectContaining({ id: theme.id }),
        verse: expect.objectContaining({ reference: "John 3:16" }),
      }),
    )
  })

  it("emits a blank verse when live output is off", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")
    const theme = useBroadcastStore.getState().themes[0]
    useBroadcastStore.setState({
      activeThemeId: theme.id,
      isLive: false,
      liveVerse: {
        reference: "John 3:16",
        segments: [{ text: "For God so loved the world", verseNumber: 16 }],
      },
    })

    emitToMock.mockClear()
    useBroadcastStore.getState().syncBroadcastOutputFor("main")

    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({
        verse: null,
      }),
    )
  })

  it("syncs the broadcast output when live mode changes", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")
    const theme = useBroadcastStore.getState().themes[0]
    useBroadcastStore.setState({
      activeThemeId: theme.id,
      isLive: false,
      liveVerse: {
        reference: "John 3:16",
        segments: [{ text: "For God so loved the world", verseNumber: 16 }],
      },
    })

    emitToMock.mockClear()
    useBroadcastStore.getState().setLive(true)

    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({
        verse: expect.objectContaining({ reference: "John 3:16" }),
      }),
    )

    emitToMock.mockClear()
    useBroadcastStore.getState().setLive(false)

    expect(emitToMock).toHaveBeenCalledWith(
      "broadcast",
      "broadcast:verse-update",
      expect.objectContaining({
        verse: null,
      }),
    )
  })

  it("stores the reading mode auto-live preference without emitting output", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")

    emitToMock.mockClear()
    useBroadcastStore.getState().setReadingModeAutoLive(false)

    expect(useBroadcastStore.getState().readingModeAutoLive).toBe(false)
    expect(emitToMock).not.toHaveBeenCalled()
  })

  it("stores the main display monitor index without emitting output", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")

    emitToMock.mockClear()
    useBroadcastStore.getState().setMainDisplayMonitorIndex(2)

    expect(useBroadcastStore.getState().mainDisplayMonitorIndex).toBe(2)
    expect(emitToMock).not.toHaveBeenCalled()
  })

  it("stores the alt display monitor index without emitting output", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")

    emitToMock.mockClear()
    useBroadcastStore.getState().setAltDisplayMonitorIndex(1)

    expect(useBroadcastStore.getState().altDisplayMonitorIndex).toBe(1)
    expect(emitToMock).not.toHaveBeenCalled()
  })

  it("stores the main projector fullscreen preference without emitting output", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")

    emitToMock.mockClear()
    useBroadcastStore.getState().setMainProjectorFullscreen(true)

    expect(useBroadcastStore.getState().mainProjectorFullscreen).toBe(true)
    expect(emitToMock).not.toHaveBeenCalled()
  })

  it("stores the alt projector fullscreen preference without emitting output", async () => {
    const { useBroadcastStore } = await import("./broadcast-store")

    emitToMock.mockClear()
    useBroadcastStore.getState().setAltProjectorFullscreen(true)

    expect(useBroadcastStore.getState().altProjectorFullscreen).toBe(true)
    expect(emitToMock).not.toHaveBeenCalled()
  })
})
