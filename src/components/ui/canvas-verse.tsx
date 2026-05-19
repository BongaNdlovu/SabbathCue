import { useRef, useEffect, useState, useCallback, useMemo, memo } from "react"
import { getBroadcastRenderKey } from "@/lib/broadcast-render-key"
import { renderVerse } from "@/lib/verse-renderer"
import type { BroadcastTheme, VerseRenderData } from "@/types"
import { cn } from "@/lib/utils"

interface CanvasVerseProps {
  theme: BroadcastTheme
  verse: VerseRenderData | null
  className?: string
}

export const CanvasVerse = memo(function CanvasVerse({
  theme,
  verse,
  className,
}: CanvasVerseProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const imageCacheRef = useRef<Map<string, HTMLImageElement>>(new Map())
  const [containerSize, setContainerSize] = useState({ width: 0, height: 0 })
  const renderKey = useMemo(
    () => getBroadcastRenderKey(theme, verse),
    [theme, verse],
  )
  const lastDrawKeyRef = useRef<string | null>(null)

  // Measure available canvas box with ResizeObserver.
  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect
      if (!rect) return
      setContainerSize({
        width: rect.width,
        height: rect.height,
      })
    })
    observer.observe(container)
    return () => observer.disconnect()
  }, [])

  const draw = useCallback((force = false) => {
    const canvas = canvasRef.current
    if (!canvas || containerSize.width === 0 || containerSize.height === 0) return
    const ctx = canvas.getContext("2d")
    if (!ctx) return

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

    canvas.width = displayW * dpr
    canvas.height = displayH * dpr
    canvas.style.width = `${displayW}px`
    canvas.style.height = `${displayH}px`

    ctx.scale(dpr, dpr)
    const scale = displayW / theme.resolution.width
    renderVerse(ctx, theme, verse, {
      scale,
      imageCache: imageCacheRef.current,
    })
  }, [theme, verse, containerSize, renderKey])

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

  // Redraw whenever theme, verse, or container size changes.
  useEffect(() => {
    draw()
  }, [draw])

  return (
    <div ref={containerRef} className={cn("flex h-full w-full items-center justify-center", className)}>
      <canvas ref={canvasRef} className="max-h-full max-w-full rounded-md" />
    </div>
  )
})
