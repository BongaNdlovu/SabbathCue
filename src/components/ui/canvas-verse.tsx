import { useRef, useEffect, useState, useCallback, useMemo, memo } from "react"
import { getBroadcastRenderKey } from "@/lib/broadcast-render-key"
import { renderPresentation } from "@/lib/verse-renderer"
import {
  isKineticTheme,
  onClothPortraitLoaded,
} from "@/lib/kinetic-theme-renderer"
import type { BroadcastTheme, PresentationRenderData } from "@/types"
import { cn } from "@/lib/utils"

const KINETIC_PREVIEW_TARGET_FPS = 15
const KINETIC_PREVIEW_FRAME_INTERVAL_MS = 1000 / KINETIC_PREVIEW_TARGET_FPS

interface CanvasPresentationProps {
  theme: BroadcastTheme
  item: PresentationRenderData | null
  className?: string
  /**
   * When true (default) a kinetic theme animates its moving background via a
   * requestAnimationFrame loop. Static themes ignore this and render once. Set
   * false to render a deterministic static frame even for kinetic themes (e.g.
   * library cards that are not selected/hovered).
   */
  animate?: boolean
}

interface CanvasVerseProps {
  theme: BroadcastTheme
  verse: PresentationRenderData | null
  className?: string
  animate?: boolean
}

export const CanvasPresentation = memo(function CanvasPresentation({
  theme,
  item,
  className,
  animate = true,
}: CanvasPresentationProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const bufferCanvasRef = useRef<HTMLCanvasElement | null>(null)
  const imageCacheRef = useRef<Map<string, HTMLImageElement>>(new Map())
  const [containerSize, setContainerSize] = useState({ width: 0, height: 0 })
  const pendingSizeFrameRef = useRef<number | null>(null)
  const renderKey = useMemo(
    () => getBroadcastRenderKey(theme, item),
    [theme, item]
  )
  const lastDrawKeyRef = useRef<string | null>(null)

  // Measure available canvas box with ResizeObserver.
  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect
      if (!rect) return

      const nextSize = {
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      }

      if (pendingSizeFrameRef.current !== null) {
        cancelAnimationFrame(pendingSizeFrameRef.current)
      }
      pendingSizeFrameRef.current = requestAnimationFrame(() => {
        pendingSizeFrameRef.current = null
        setContainerSize((current) =>
          current.width === nextSize.width && current.height === nextSize.height
            ? current
            : nextSize
        )
      })
    })
    observer.observe(container)
    return () => {
      observer.disconnect()
      if (pendingSizeFrameRef.current !== null) {
        cancelAnimationFrame(pendingSizeFrameRef.current)
        pendingSizeFrameRef.current = null
      }
    }
  }, [])

  const draw = useCallback(
    (force = false, timeMs = 0) => {
      const canvas = canvasRef.current
      if (!canvas || containerSize.width === 0 || containerSize.height === 0)
        return
      const visibleCtx = canvas.getContext("2d")
      if (!visibleCtx) return

      const dpr = window.devicePixelRatio || 1
      const aspectRatio = theme.resolution.width / theme.resolution.height
      const maxW = containerSize.width
      const maxH = containerSize.height
      let displayW = maxW
      let displayH = displayW / aspectRatio

      if (displayH > maxH) {
        displayH = maxH
        displayW = displayH * aspectRatio
      }

      const drawKey = `${renderKey}:${Math.round(displayW)}x${Math.round(displayH)}:${window.devicePixelRatio || 1}`
      if (!force && lastDrawKeyRef.current === drawKey) return
      lastDrawKeyRef.current = drawKey

      const buffer = bufferCanvasRef.current ?? document.createElement("canvas")
      bufferCanvasRef.current = buffer
      buffer.width = Math.max(1, Math.round(displayW * dpr))
      buffer.height = Math.max(1, Math.round(displayH * dpr))
      const bufferCtx = buffer.getContext("2d")
      if (!bufferCtx) return

      bufferCtx.setTransform(1, 0, 0, 1, 0, 0)
      bufferCtx.clearRect(0, 0, buffer.width, buffer.height)
      bufferCtx.scale(dpr, dpr)
      const scale = displayW / theme.resolution.width
      renderPresentation(bufferCtx, theme, item, {
        scale,
        imageCache: imageCacheRef.current,
        timeMs,
      })

      if (canvas.width !== buffer.width) canvas.width = buffer.width
      if (canvas.height !== buffer.height) canvas.height = buffer.height
      canvas.style.width = `${displayW}px`
      canvas.style.height = `${displayH}px`

      visibleCtx.setTransform(1, 0, 0, 1, 0, 0)
      visibleCtx.clearRect(0, 0, canvas.width, canvas.height)
      visibleCtx.drawImage(buffer, 0, 0)
    },
    [theme, item, containerSize, renderKey]
  )

  // Preload background image so the renderer can find it in the cache.
  useEffect(() => {
    const bg = theme.background
    if (bg.type !== "image" || !bg.image?.url) return
    const url = bg.image.url
    const cache = imageCacheRef.current
    if (cache.has(url)) return

    const img = new Image()
    img.onload = () => {
      cache.set(url, img)
      draw(true)
    }
    img.onerror = () => {
      console.warn("[canvas-verse] failed to load background image", {
        url: url.slice(0, 100),
      })
    }
    img.src = url
  }, [theme.background, draw])

  useEffect(() => {
    if (item?.kind !== "slideDeck" || !item.slideImageUrl) return
    const url = item.slideImageUrl
    const cache = imageCacheRef.current
    if (cache.has(url)) return

    const img = new Image()
    img.onload = () => {
      cache.set(url, img)
      draw(true)
    }
    img.onerror = () => {
      console.warn("[canvas-verse] failed to load slide image", {
        url: url.slice(0, 100),
      })
    }
    img.src = url
  }, [item, draw])

  useEffect(() => {
    if (theme.kinetic?.backgroundKind !== "cloth") return
    return onClothPortraitLoaded(() => draw(true))
  }, [theme.kinetic?.backgroundKind, draw])

  // Redraw whenever theme, verse, or container size changes.
  useEffect(() => {
    draw()
  }, [draw])

  // Procedural themes use bundled canvas-only display fonts that are not
  // referenced by the DOM. Request the family for animated and frozen variants,
  // then redraw once it is available.
  useEffect(() => {
    if (!theme.kinetic || typeof document === "undefined") return
    const fontSet = document.fonts
    if (!fontSet || typeof fontSet.load !== "function") return
    let cancelled = false
    // Redraw once the fonts resolve; on failure the fallback face already drew,
    // so redraw either way rather than leave a rejected promise unhandled.
    const redraw = () => {
      if (!cancelled) draw(true)
    }
    Promise.all([
      fontSet.load(
        `${theme.verseText.fontWeight} 76px "${theme.verseText.fontFamily}"`
      ),
      fontSet.load(
        `${theme.reference.fontWeight} 36px "${theme.reference.fontFamily}"`
      ),
    ]).then(redraw, redraw)
    return () => {
      cancelled = true
    }
  }, [theme, draw])

  // Kinetic themes animate their moving background. The loop only runs while a
  // kinetic theme is shown AND `animate` is enabled, so static themes (and
  // non-selected library cards) never spin a RAF loop.
  useEffect(() => {
    if (!animate || !isKineticTheme(theme)) return
    if (typeof window === "undefined" || !window.requestAnimationFrame) return

    let frame = 0
    const start = performance.now()
    let lastDrawAt = 0
    const loop = (now: number) => {
      if (
        lastDrawAt === 0 ||
        now - lastDrawAt >= KINETIC_PREVIEW_FRAME_INTERVAL_MS
      ) {
        lastDrawAt = now
        draw(true, now - start)
      }
      frame = window.requestAnimationFrame(loop)
    }
    frame = window.requestAnimationFrame(loop)
    return () => window.cancelAnimationFrame(frame)
  }, [animate, theme, draw])

  return (
    <div
      ref={containerRef}
      className={cn(
        "flex h-full w-full items-center justify-center",
        className
      )}
    >
      <canvas ref={canvasRef} className="max-h-full max-w-full rounded-md" />
    </div>
  )
})

export const CanvasVerse = memo(function CanvasVerse({
  theme,
  verse,
  className,
  animate = true,
}: CanvasVerseProps) {
  return (
    <CanvasPresentation
      theme={theme}
      item={verse}
      className={className}
      animate={animate}
    />
  )
})
