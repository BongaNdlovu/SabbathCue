import type {
  BroadcastTheme,
  VerseRenderData,
  PresentationRenderData,
  RenderOptions,
} from "@/types"
import {
  computeVerseLayoutMetrics,
  calculateMaxAvailableVerseHeight,
  calculateScaledFontSize,
  wrapText,
  textForPresentation,
  referenceForPresentation,
  clampCornerRadius,
  type VerseLayoutMetrics,
  type VerseLayoutRect,
} from "@/lib/verse-layout"
import {
  drawBackground,
  drawReference,
  drawHymnSectionLabel,
  drawHymnSlideCounter,
  drawSlideDeckImage,
  drawVerseText,
  roundRect,
} from "@/lib/verse-draw"
import { resolveHymnSectionTheme } from "@/lib/hymn-theme-style"

export type { VerseLayoutMetrics, VerseLayoutRect }
export {
  wrapText,
  textForPresentation,
  clampCornerRadius,
  computeVerseLayoutMetrics,
}

export function renderVerse(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  data: VerseRenderData | null,
  options?: RenderOptions
): VerseLayoutMetrics | null {
  try {
    return renderPresentationImpl(ctx, theme, data, options)
  } catch (e) {
    console.error("[verse-renderer] render error:", e)
    return null
  }
}

export function renderPresentation(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  data: PresentationRenderData | null,
  options?: RenderOptions
): VerseLayoutMetrics | null {
  try {
    return renderPresentationImpl(ctx, theme, data, options)
  } catch (e) {
    console.error("[verse-renderer] render error:", e)
    return null
  }
}

function renderPresentationImpl(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  data: VerseRenderData | PresentationRenderData | null,
  options?: RenderOptions
): VerseLayoutMetrics {
  const presentationData =
    data && "kind" in data ? (data as PresentationRenderData) : null
  const resolvedTheme = resolveHymnSectionTheme(theme, presentationData)
  const metrics = computeVerseLayoutMetrics(
    ctx,
    resolvedTheme,
    data as VerseRenderData | null,
    options
  )
  const scaledTheme = metrics.scaledTheme

  ctx.save()

  if (options?.opacity !== undefined) {
    ctx.globalAlpha = options.opacity
  }

  drawBackground(
    ctx,
    scaledTheme,
    options?.imageCache,
    options?.timeMs,
    presentationData?.hymnSlide?.sectionKind
  )

  if (scaledTheme.textBox.enabled) {
    ctx.save()
    ctx.globalAlpha = (options?.opacity ?? 1) * scaledTheme.textBox.opacity
    ctx.fillStyle = scaledTheme.textBox.color
    roundRect(
      ctx,
      metrics.textAreaRect.x,
      metrics.textAreaRect.y,
      metrics.textAreaRect.width,
      metrics.textAreaRect.height,
      scaledTheme.textBox.borderRadius
    )
    ctx.fill()
    ctx.restore()
  }

  if (!data) {
    ctx.restore()
    return metrics
  }

  if (
    drawSlideDeckImage(
      ctx,
      scaledTheme,
      data as PresentationRenderData,
      options?.imageCache,
      metrics.textAreaRect
    )
  ) {
    drawHymnSlideCounter(ctx, scaledTheme, data as PresentationRenderData)
    ctx.restore()
    return metrics
  }

  const referenceRect = metrics.referenceRect
  const verseRect = metrics.verseRect
  if (verseRect) {
    const maxAvailableVerseHeight = calculateMaxAvailableVerseHeight(
      scaledTheme,
      metrics.textRect,
      referenceRect?.height ?? 0
    )

    const scaledFontSize = calculateScaledFontSize(
      ctx,
      scaledTheme,
      data,
      metrics.textRect.width,
      maxAvailableVerseHeight
    )

    drawHymnSectionLabel(
      ctx,
      scaledTheme,
      data as PresentationRenderData,
      metrics.textRect.x,
      metrics.textRect.width,
      verseRect.y
    )
    drawVerseText(
      ctx,
      scaledTheme,
      data,
      metrics.textRect.x,
      metrics.textRect.width,
      verseRect.y,
      scaledFontSize
    )
  }
  if (referenceRect) {
    drawReference(
      ctx,
      scaledTheme,
      referenceForPresentation(scaledTheme, data),
      metrics.textRect.x,
      metrics.textRect.width,
      referenceRect.y
    )
  }
  drawHymnSlideCounter(ctx, scaledTheme, data as PresentationRenderData)

  ctx.restore()
  return metrics
}
