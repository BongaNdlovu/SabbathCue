import { useCallback, useEffect, useRef, type RefObject } from "react"
import { invoke } from "@tauri-apps/api/core"
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow"
import { getBroadcastRenderKey } from "@/lib/broadcast-render-key"
import { renderVerse } from "@/lib/verse-renderer"
import type { BroadcastTheme, VerseRenderData } from "@/types/broadcast"
import type { NdiConfigEventPayload, NdiFrameRequest } from "@/types"

export interface BroadcastPayload {
  theme: BroadcastTheme
  verse: VerseRenderData | null
}

declare global {
  interface Window {
    __SABBATHCUE_BROADCAST_TEST__?: {
      render: (payload: BroadcastPayload) => void
    }
  }
}

const DEFAULT_NDI_CONFIG: NdiConfigEventPayload = {
  active: false,
  fps: 24,
  width: 1920,
  height: 1080,
}

function uint8ToBase64(bytes: Uint8Array | Uint8ClampedArray): string {
  const chunk = 0x8000
  const parts: string[] = []
  for (let i = 0; i < bytes.length; i += chunk) {
    parts.push(
      String.fromCharCode.apply(
        null,
        bytes.subarray(i, i + chunk) as unknown as number[],
      ),
    )
  }
  return btoa(parts.join(""))
}

function fillBlack(canvas: HTMLCanvasElement): void {
  const ctx = canvas.getContext("2d")
  if (!ctx) return
  ctx.fillStyle = "#000"
  ctx.fillRect(0, 0, canvas.width, canvas.height)
}

interface UseBroadcastOutputRuntimeOptions {
  canvasRef: RefObject<HTMLCanvasElement | null>
  outputId: string
}

export function useBroadcastOutputRuntime({
  canvasRef,
  outputId,
}: UseBroadcastOutputRuntimeOptions): void {
  const latestData = useRef<BroadcastPayload | null>(null)
  const imageCacheRef = useRef<Map<string, HTMLImageElement>>(new Map())
  const ndiConfigRef = useRef<NdiConfigEventPayload>(DEFAULT_NDI_CONFIG)
  const ndiCanvasRef = useRef<HTMLCanvasElement | null>(null)
  const lastPushRef = useRef(0)
  const pushingRef = useRef(false)
  const lastRenderKeyRef = useRef<string | null>(null)

  const logDebug = useCallback((message: string, meta?: unknown) => {
    if (!import.meta.env.DEV) return
    if (meta === undefined) {
      console.debug(`[broadcast-output] ${message}`)
      return
    }
    console.debug(`[broadcast-output] ${message}`, meta)
  }, [])

  const draw = useCallback(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext("2d")
    if (!ctx) return

    const data = latestData.current
    if (!data) {
      fillBlack(canvas)
      return
    }

    const { theme, verse } = data
    canvas.width = theme.resolution.width
    canvas.height = theme.resolution.height
    const result = renderVerse(ctx, theme, verse, {
      scale: 1,
      imageCache: imageCacheRef.current,
    })

    if (!result) {
      fillBlack(canvas)
      logDebug("renderVerse returned null; drew fallback frame")
    }
  }, [canvasRef, logDebug])

  const preloadBackgroundImage = useCallback((theme: BroadcastTheme) => {
    const bg = theme.background
    if (bg.type !== "image" || !bg.image?.url) return

    const url = bg.image.url
    const cache = imageCacheRef.current
    if (cache.has(url)) return

    const img = new Image()
    img.onload = () => {
      cache.set(url, img)
      logDebug("Background image loaded", { url })
      draw()
    }
    img.onerror = () => {
      console.warn("[broadcast-output] failed to load background image", { url })
    }
    img.src = url
  }, [draw, logDebug])

  const pushNdiFrame = useCallback(async () => {
    if (!ndiConfigRef.current.active) return
    if (pushingRef.current) return
    pushingRef.current = true

    try {
      const canvas = canvasRef.current
      if (!canvas) return
      const ctx = canvas.getContext("2d")
      if (!ctx) return

      const targetWidth = ndiConfigRef.current.width
      const targetHeight = ndiConfigRef.current.height

      let sourceCtx = ctx
      let sourceWidth = canvas.width
      let sourceHeight = canvas.height

      if (canvas.width !== targetWidth || canvas.height !== targetHeight) {
        const ndiCanvas = ndiCanvasRef.current ?? document.createElement("canvas")
        ndiCanvas.width = targetWidth
        ndiCanvas.height = targetHeight
        const ndiCtx = ndiCanvas.getContext("2d")
        if (!ndiCtx) return
        ndiCtx.drawImage(canvas, 0, 0, targetWidth, targetHeight)
        ndiCanvasRef.current = ndiCanvas
        sourceCtx = ndiCtx
        sourceWidth = targetWidth
        sourceHeight = targetHeight
      }

      const imageData = sourceCtx.getImageData(0, 0, sourceWidth, sourceHeight)
      const request: NdiFrameRequest = {
        outputId,
        width: sourceWidth,
        height: sourceHeight,
        rgbaBase64: uint8ToBase64(imageData.data),
      }

      await invoke("push_ndi_frame", { request })
      lastPushRef.current = Date.now()
    } catch (error) {
      console.warn("[broadcast-output] push_ndi_frame failed", error)
    } finally {
      pushingRef.current = false
    }
  }, [canvasRef, outputId])

  const pushNdiBurst = useCallback(() => {
    if (!ndiConfigRef.current.active) return
    void pushNdiFrame()
    setTimeout(() => void pushNdiFrame(), 150)
    setTimeout(() => void pushNdiFrame(), 300)
  }, [pushNdiFrame])

  useEffect(() => {
    const canvas = canvasRef.current
    if (canvas) {
      canvas.width = 1920
      canvas.height = 1080
      fillBlack(canvas)
    }

    const applyPayload = (payload: BroadcastPayload) => {
      const renderKey = getBroadcastRenderKey(payload.theme, payload.verse)

      if (lastRenderKeyRef.current === renderKey) {
        logDebug("Skipped duplicate broadcast payload", {
          hasVerse: Boolean(payload.verse),
          themeId: payload.theme.id,
        })
        return
      }

      lastRenderKeyRef.current = renderKey
      latestData.current = payload
      preloadBackgroundImage(payload.theme)
      logDebug("Received broadcast payload", {
        hasVerse: Boolean(payload.verse),
        themeId: payload.theme.id,
      })
      draw()
      pushNdiBurst()
    }

    if (import.meta.env.DEV || import.meta.env.MODE === "test") {
      window.__SABBATHCUE_BROADCAST_TEST__ = {
        render: applyPayload,
      }
    }

    let unlisten: Promise<() => void> | null = null
    let unlistenNdiConfig: Promise<() => void> | null = null

    try {
      const currentWindow = getCurrentWebviewWindow()
      logDebug("Listener registration started", { label: currentWindow.label })
      unlisten = currentWindow.listen<BroadcastPayload>("broadcast:verse-update", (event) => {
        applyPayload(event.payload)
      })

      unlistenNdiConfig = currentWindow.listen<NdiConfigEventPayload>(
        "broadcast:ndi-config",
        (event) => {
          ndiConfigRef.current = event.payload
          logDebug("Received broadcast:ndi-config", event.payload)
          if (event.payload.active) pushNdiBurst()
        },
      )

      void currentWindow.emitTo("main", "broadcast:output-ready").then(() => {
        logDebug("Sent broadcast:output-ready")
      }).catch(() => {
        console.warn("[broadcast-output] failed to send output-ready event")
      })
    } catch (error) {
      logDebug("Tauri broadcast listeners unavailable", error)
    }

    void invoke<{ active: boolean; width: number; height: number; fps: number } | null>(
      "get_ndi_status",
      { outputId },
    )
      .then((status) => {
        if (!status?.active) return
        ndiConfigRef.current = {
          active: true,
          fps: status.fps,
          width: status.width,
          height: status.height,
        }
        logDebug("Fetched NDI status on mount", status)
      })
      .catch(() => {
        // The browser e2e harness runs outside Tauri, so this command is optional.
      })

    return () => {
      delete window.__SABBATHCUE_BROADCAST_TEST__
      unlisten?.then((fn) => fn())
      unlistenNdiConfig?.then((fn) => fn())
    }
  }, [canvasRef, draw, logDebug, outputId, preloadBackgroundImage, pushNdiBurst])

  useEffect(() => {
    const timer = setInterval(() => {
      if (!ndiConfigRef.current.active) return
      const elapsed = Date.now() - lastPushRef.current
      if (elapsed > 2000) void pushNdiFrame()
    }, 2000)
    return () => clearInterval(timer)
  }, [pushNdiFrame])
}
