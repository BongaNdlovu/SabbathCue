import type {
  BroadcastTheme,
  HymnPresentationSectionKind,
  VerseRenderData,
  PresentationRenderData,
} from "@/types"
import {
  wrapText,
  textForPresentation,
  alignX,
  resolveHorizontalAlign,
  resolveTextTransform,
  resolveTextDecoration,
  applyTextTransform,
  lineHeightForPresentation,
  clampCornerRadius,
  type VerseLayoutRect,
} from "@/lib/verse-layout"
import { drawKineticBackground } from "@/lib/kinetic-theme-renderer"

function drawTextDecorationLine(
  ctx: CanvasRenderingContext2D,
  decoration: "none" | "underline" | "line-through",
  color: string,
  align: "left" | "center" | "right" | "justify",
  x: number,
  y: number,
  width: number,
  fontSize: number,
  fallbackLeftX?: number
): void {
  if (decoration === "none" || width <= 0) return
  const startX =
    align === "left"
      ? x
      : align === "center"
        ? x - width / 2
        : align === "right"
          ? x - width
          : (fallbackLeftX ?? x)
  const lineY =
    decoration === "underline" ? y + fontSize * 0.92 : y + fontSize * 0.52
  ctx.save()
  ctx.strokeStyle = color
  ctx.lineWidth = Math.max(1, fontSize * 0.06)
  ctx.beginPath()
  ctx.moveTo(startX, lineY)
  ctx.lineTo(startX + width, lineY)
  ctx.stroke()
  ctx.restore()
}

export function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number
): void {
  const cornerRadius = clampCornerRadius(width, height, radius)
  ctx.beginPath()
  ctx.moveTo(x + cornerRadius, y)
  ctx.lineTo(x + width - cornerRadius, y)
  ctx.arcTo(x + width, y, x + width, y + cornerRadius, cornerRadius)
  ctx.lineTo(x + width, y + height - cornerRadius)
  ctx.arcTo(
    x + width,
    y + height,
    x + width - cornerRadius,
    y + height,
    cornerRadius
  )
  ctx.lineTo(x + cornerRadius, y + height)
  ctx.arcTo(x, y + height, x, y + height - cornerRadius, cornerRadius)
  ctx.lineTo(x, y + cornerRadius)
  ctx.arcTo(x, y, x + cornerRadius, y, cornerRadius)
  ctx.closePath()
}

function drawGradientBackground(
  ctx: CanvasRenderingContext2D,
  bg: BroadcastTheme["background"],
  width: number,
  height: number
): void {
  if (!bg.gradient) return

  let grad: CanvasGradient

  if (bg.gradient.type === "linear") {
    const angle = (bg.gradient.angle * Math.PI) / 180
    const cx = width / 2
    const cy = height / 2
    const len = Math.sqrt(width * width + height * height) / 2
    grad = ctx.createLinearGradient(
      cx - Math.cos(angle) * len,
      cy - Math.sin(angle) * len,
      cx + Math.cos(angle) * len,
      cy + Math.sin(angle) * len
    )
  } else {
    grad = ctx.createRadialGradient(
      width / 2,
      height / 2,
      0,
      width / 2,
      height / 2,
      Math.max(width, height) / 2
    )
  }

  for (const stop of bg.gradient.stops) {
    grad.addColorStop(stop.position / 100, stop.color)
  }

  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)
}

function drawImageBackground(
  ctx: CanvasRenderingContext2D,
  bg: BroadcastTheme["background"],
  width: number,
  height: number,
  imageCache?: Map<string, HTMLImageElement>
): void {
  if (!bg.image) {
    ctx.fillStyle = "#000"
    ctx.fillRect(0, 0, width, height)
    return
  }

  const img = imageCache?.get(bg.image.url)
  if (!img) {
    ctx.fillStyle = bg.image.tint ?? "#000"
    ctx.fillRect(0, 0, width, height)
    return
  }

  ctx.save()

  if (bg.image.blur > 0) {
    ctx.filter = `blur(${bg.image.blur}px) brightness(${bg.image.brightness / 100})`
  } else if (bg.image.brightness !== 100) {
    ctx.filter = `brightness(${bg.image.brightness / 100})`
  }

  let drawX = 0
  let drawY = 0
  let drawW = width
  let drawH = height

  const imgRatio = img.naturalWidth / img.naturalHeight
  const canvasRatio = width / height

  switch (bg.image.fit) {
    case "cover":
      if (imgRatio > canvasRatio) {
        drawH = height
        drawW = height * imgRatio
        drawX = (width - drawW) / 2
      } else {
        drawW = width
        drawH = width / imgRatio
        drawY = (height - drawH) / 2
      }
      break
    case "contain":
      if (imgRatio > canvasRatio) {
        drawW = width
        drawH = width / imgRatio
        drawY = (height - drawH) / 2
      } else {
        drawH = height
        drawW = height * imgRatio
        drawX = (width - drawW) / 2
      }
      break
    case "stretch":
      break
  }

  ctx.drawImage(img, drawX, drawY, drawW, drawH)
  ctx.restore()

  if (bg.image.tint) {
    ctx.fillStyle = bg.image.tint
    ctx.fillRect(0, 0, width, height)
  }
}

export function drawBackground(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  imageCache?: Map<string, HTMLImageElement>,
  timeMs?: number,
  hymnSectionKind?: HymnPresentationSectionKind
): void {
  const { width, height } = theme.resolution
  const bg = theme.background

  // Kinetic themes paint a canvas-native moving background. On failure the
  // renderer returns false and we fall through to the static background below,
  // so a kinetic preset always has the gradient/solid fallback to rely on.
  if (theme.kinetic) {
    if (drawKineticBackground(ctx, theme, timeMs ?? 0, hymnSectionKind)) return
  }

  switch (bg.type) {
    case "solid":
      ctx.fillStyle = bg.color
      ctx.fillRect(0, 0, width, height)
      break

    case "gradient":
      drawGradientBackground(ctx, bg, width, height)
      break

    case "image":
      drawImageBackground(ctx, bg, width, height, imageCache)
      break

    case "transparent":
      ctx.clearRect(0, 0, width, height)
      break
  }
}

export function drawReference(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  text: string,
  textRectX: number,
  textRectWidth: number,
  y: number
): number {
  const ref = theme.reference
  const transformed = applyTextTransform(
    ref.uppercase ? text.toUpperCase() : text,
    resolveTextTransform(ref.textTransform)
  )
  const refAlign = resolveHorizontalAlign(
    ref.horizontalAlign,
    theme.layout.textAlign,
    false
  )
  const refDecoration = resolveTextDecoration(ref.textDecoration)

  ctx.save()
  ctx.font = `${ref.fontWeight} ${ref.fontSize}px "${ref.fontFamily}", sans-serif`
  ctx.fillStyle = ref.color
  ctx.textBaseline = "top"

  if (ref.letterSpacing > 0) {
    try {
      ctx.letterSpacing = `${ref.letterSpacing}px`
    } catch {
      /* unsupported in some WebViews */
    }
  }

  const canvasAlign = refAlign === "justify" ? "left" : refAlign
  ctx.textAlign = canvasAlign
  const x = alignX(canvasAlign, textRectX, textRectWidth)
  ctx.fillText(transformed, x, y)
  const drawnWidth = Math.min(
    textRectWidth,
    Math.max(1, ctx.measureText(transformed).width)
  )
  drawTextDecorationLine(
    ctx,
    refDecoration,
    ref.color,
    refAlign,
    x,
    y,
    drawnWidth,
    ref.fontSize,
    textRectX
  )
  ctx.restore()

  return ref.fontSize * 1.5
}

export function drawHymnSectionLabel(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  data: PresentationRenderData,
  textRectX: number,
  textRectWidth: number,
  verseY: number
): void {
  const slide = data.hymnSlide
  if (!theme.hymnPresentation?.showSectionLabel || !slide?.sectionLabel) return

  const isRefrain =
    slide.sectionKind === "refrain" || slide.sectionKind === "chorus"
  const label =
    isRefrain && theme.hymnPresentation.refrainLabel
      ? theme.hymnPresentation.refrainLabel
      : slide.sectionLabel
  const fontSize = Math.max(18, Math.round(theme.reference.fontSize * 0.72))
  const y = Math.max(theme.layout.padding.top, verseY - fontSize * 2.15)
  const align = theme.layout.textAlign
  const x = alignX(align, textRectX, textRectWidth)

  ctx.save()
  ctx.font = `600 ${fontSize}px "${theme.reference.fontFamily}", sans-serif`
  ctx.fillStyle = theme.reference.color
  ctx.textAlign = align
  ctx.textBaseline = "top"
  ctx.globalAlpha *= 0.92
  ctx.fillText(label.toUpperCase(), x, y)
  ctx.restore()
}

export function drawHymnSlideCounter(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  data: PresentationRenderData
): void {
  const slide = data.hymnSlide
  if (
    !["hymn", "slideDeck"].includes(data.kind ?? "") ||
    !slide ||
    slide.slideCount <= 0
  )
    return

  const counter = theme.hymnPresentation?.slideCounter
  const format = counter?.format ?? "of"
  const text =
    format === "slash"
      ? `${slide.slideIndex + 1}/${slide.slideCount}`
      : `${slide.slideIndex + 1} of ${slide.slideCount}`
  const position = counter?.position ?? "top-right"
  const style = counter?.style ?? "badge"

  if (position === "bottom-right" && style === "plain") {
    const fontSize = Math.max(
      18,
      Math.round(
        theme.verseText.fontSize * 0.65 * (theme.resolution.width / 1920)
      )
    )
    const margin = Math.round(theme.resolution.width * 0.05)

    ctx.save()
    ctx.font = `${theme.verseText.fontWeight} ${fontSize}px "${theme.verseText.fontFamily}", sans-serif`
    ctx.textBaseline = "bottom"
    ctx.textAlign = "right"
    ctx.fillStyle = theme.verseText.color
    ctx.globalAlpha = 0.9
    ctx.fillText(
      text,
      theme.resolution.width - margin,
      theme.resolution.height - margin
    )
    ctx.restore()
    return
  }

  const fontSize = Math.max(14, Math.round(theme.resolution.width * 0.018))
  const paddingX = Math.round(fontSize * 0.75)
  const paddingY = Math.round(fontSize * 0.35)
  const margin = Math.round(fontSize * 1.2)

  ctx.save()
  ctx.font = `600 ${fontSize}px "Inter", sans-serif`
  ctx.textBaseline = "middle"
  ctx.textAlign = "center"

  const textWidth = ctx.measureText(text).width
  const boxWidth = textWidth + paddingX * 2
  const boxHeight = fontSize + paddingY * 2
  const x = theme.resolution.width - margin - boxWidth
  const y = margin

  ctx.fillStyle = "rgba(0, 0, 0, 0.38)"
  roundRect(ctx, x, y, boxWidth, boxHeight, Math.max(4, fontSize * 0.25))
  ctx.fill()

  ctx.fillStyle = "rgba(255, 255, 255, 0.86)"
  ctx.fillText(text, x + boxWidth / 2, y + boxHeight / 2)
  ctx.restore()
}

export function drawSlideDeckImage(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  data: PresentationRenderData,
  imageCache?: Map<string, HTMLImageElement>,
  targetRect?: VerseLayoutRect
): boolean {
  if (data.kind !== "slideDeck" || !data.slideImageUrl) return false
  const img = imageCache?.get(data.slideImageUrl)
  if (!img) return false

  const { width, height } = theme.resolution
  const bounds =
    data.applyTheme && targetRect ? targetRect : { x: 0, y: 0, width, height }
  const targetWidth = Math.max(1, bounds.width)
  const targetHeight = Math.max(1, bounds.height)
  const imgRatio = img.naturalWidth / img.naturalHeight
  const targetRatio = targetWidth / targetHeight
  let drawX = bounds.x
  let drawY = bounds.y
  let drawW = targetWidth
  let drawH = targetHeight

  if (imgRatio > targetRatio) {
    drawW = targetWidth
    drawH = targetWidth / imgRatio
    drawY = bounds.y + (targetHeight - drawH) / 2
  } else {
    drawH = targetHeight
    drawW = targetHeight * imgRatio
    drawX = bounds.x + (targetWidth - drawW) / 2
  }

  ctx.save()
  if (!data.applyTheme) {
    // Default: full-bleed slide on black bars.
    ctx.fillStyle = "#000"
    ctx.fillRect(0, 0, width, height)
  }
  // When themed, the theme background drawn earlier shows through the bars.
  ctx.drawImage(img, drawX, drawY, drawW, drawH)
  if (data.applyTheme) {
    const tint =
      theme.background.type === "image" ? theme.background.image?.tint : null
    if (tint) {
      ctx.fillStyle = tint
      ctx.fillRect(drawX, drawY, drawW, drawH)
    }
  }
  ctx.restore()
  return true
}

export function drawVerseText(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  verse: VerseRenderData | PresentationRenderData,
  textRectX: number,
  textRectWidth: number,
  startY: number,
  scaledFontSize?: number
): number {
  const vt = theme.verseText
  const vn = theme.verseNumbers
  const verseAlign = resolveHorizontalAlign(
    vt.horizontalAlign,
    theme.layout.textAlign,
    true
  )
  const verseDecoration = resolveTextDecoration(vt.textDecoration)
  const actualFontSize = scaledFontSize ?? vt.fontSize
  const lineHeightPx = lineHeightForPresentation(theme, verse, actualFontSize)

  ctx.save()
  const fontStylePrefix = vt.fontStyle === "italic" ? "italic " : ""
  ctx.font = `${fontStylePrefix}${vt.fontWeight} ${actualFontSize}px "${vt.fontFamily}", serif`
  ctx.fillStyle = vt.color
  ctx.textBaseline = "top"
  ctx.textAlign = verseAlign === "justify" ? "left" : verseAlign

  if (vt.letterSpacing > 0) {
    try {
      ctx.letterSpacing = `${vt.letterSpacing}px`
    } catch {
      /* unsupported in some WebViews */
    }
  }

  const fullText = applyTextTransform(
    textForPresentation(verse, vn.visible),
    resolveTextTransform(vt.textTransform)
  )

  const wrappedLines = wrapText(ctx, fullText, textRectWidth)

  let currentY = startY
  const x = alignX(
    verseAlign === "justify" ? "left" : verseAlign,
    textRectX,
    textRectWidth
  )

  const drawStyledLine = (line: string, drawX: number, drawY: number) => {
    if (vt.shadow) {
      ctx.save()
      ctx.shadowColor = vt.shadow.color
      ctx.shadowBlur = vt.shadow.blur
      ctx.shadowOffsetX = vt.shadow.x
      ctx.shadowOffsetY = vt.shadow.y
      ctx.fillText(line, drawX, drawY)
      ctx.restore()
    }

    if (vt.outline) {
      ctx.save()
      ctx.strokeStyle = vt.outline.color
      ctx.lineWidth = vt.outline.width
      ctx.strokeText(line, drawX, drawY)
      ctx.restore()
    }

    if (!vt.shadow) {
      ctx.fillText(line, drawX, drawY)
    }
  }

  for (const [index, line] of wrappedLines.entries()) {
    const isJustifiedLine =
      verseAlign === "justify" &&
      index < wrappedLines.length - 1 &&
      /\s+/.test(line)
    if (isJustifiedLine) {
      const words = line.trim().split(/\s+/).filter(Boolean)
      if (words.length > 1) {
        const wordsWidth = words.reduce(
          (sum, word) => sum + ctx.measureText(word).width,
          0
        )
        const gap = (textRectWidth - wordsWidth) / (words.length - 1)
        let cursorX = textRectX
        for (const word of words) {
          drawStyledLine(word, cursorX, currentY)
          cursorX += ctx.measureText(word).width + gap
        }
      } else {
        drawStyledLine(line, textRectX, currentY)
      }
      drawTextDecorationLine(
        ctx,
        verseDecoration,
        vt.color,
        "left",
        textRectX,
        currentY,
        textRectWidth,
        vt.fontSize,
        textRectX
      )
    } else {
      drawStyledLine(line, x, currentY)
      const lineWidth = Math.min(
        textRectWidth,
        Math.max(1, ctx.measureText(line).width)
      )
      drawTextDecorationLine(
        ctx,
        verseDecoration,
        vt.color,
        verseAlign,
        x,
        currentY,
        lineWidth,
        vt.fontSize,
        textRectX
      )
    }
    currentY += lineHeightPx
  }

  ctx.restore()

  return currentY - startY
}
