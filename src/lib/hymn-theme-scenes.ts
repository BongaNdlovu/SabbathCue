import type { BroadcastKineticTheme } from "@/types/broadcast"
import type { HymnPresentationSectionKind } from "@/types/presentation"

const TAU = Math.PI * 2

const HYMN_SCENE_KINDS = new Set([
  "hymn-midnight",
  "hymn-dawn",
  "hymn-minimal",
  "hymn-glass",
  "hymn-water",
  "hymn-heritage",
  "hymn-upper-room",
])

function loopPhase(timeMs: number, durationMs: number): number {
  if (!Number.isFinite(timeMs) || durationMs <= 0) return 0
  return (((timeMs % durationMs) + durationMs) % durationMs) / durationMs
}

function wave(timeMs: number, durationMs: number, offset = 0): number {
  return Math.sin((loopPhase(timeMs, durationMs) + offset) * TAU)
}

function seeded(index: number): number {
  const value = Math.sin(index * 127.1) * 43758.5453
  return value - Math.floor(value)
}

function isRefrain(kind: HymnPresentationSectionKind | undefined): boolean {
  return kind === "refrain" || kind === "chorus"
}

function fillLinear(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  colors: Array<[number, string]>,
  degrees = 145
): void {
  const angle = (degrees * Math.PI) / 180
  const cx = width / 2
  const cy = height / 2
  const length = Math.sqrt(width * width + height * height) / 2
  const gradient = ctx.createLinearGradient(
    cx - Math.cos(angle) * length,
    cy - Math.sin(angle) * length,
    cx + Math.cos(angle) * length,
    cy + Math.sin(angle) * length
  )
  for (const [stop, color] of colors) gradient.addColorStop(stop, color)
  ctx.fillStyle = gradient
  ctx.fillRect(0, 0, width, height)
}

function radialWash(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  x: number,
  y: number,
  radius: number,
  color: string,
  alphaEdge = 0
): void {
  const gradient = ctx.createRadialGradient(x, y, 0, x, y, radius)
  gradient.addColorStop(0, color)
  gradient.addColorStop(1, `rgba(0, 0, 0, ${alphaEdge})`)
  ctx.fillStyle = gradient
  ctx.fillRect(0, 0, width, height)
}

function drawVignette(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  color = "rgba(1, 8, 12, 0.5)"
): void {
  const vertical = ctx.createLinearGradient(0, 0, 0, height)
  vertical.addColorStop(0, color)
  vertical.addColorStop(0.24, "rgba(0, 0, 0, 0)")
  vertical.addColorStop(0.74, "rgba(0, 0, 0, 0)")
  vertical.addColorStop(1, color)
  ctx.fillStyle = vertical
  ctx.fillRect(0, 0, width, height)

  const horizontal = ctx.createLinearGradient(0, 0, width, 0)
  horizontal.addColorStop(0, "rgba(0, 0, 0, 0.18)")
  horizontal.addColorStop(0.18, "rgba(0, 0, 0, 0)")
  horizontal.addColorStop(0.82, "rgba(0, 0, 0, 0)")
  horizontal.addColorStop(1, "rgba(0, 0, 0, 0.18)")
  ctx.fillStyle = horizontal
  ctx.fillRect(0, 0, width, height)
}

function drawAuroraOrb(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  x: number,
  y: number,
  color: string,
  timeMs: number,
  durationMs: number,
  direction: number
): void {
  const motion = wave(timeMs, durationMs)
  const radius = Math.max(width, height) * (0.54 + motion * 0.035)
  const px = x + motion * width * 0.07 * direction
  const py = y + motion * height * 0.045
  radialWash(ctx, width, height, px, py, radius, color)
}

function drawHalo(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  color: string,
  glass = false
): void {
  const pulse = 0.5 + 0.5 * wave(timeMs, glass ? 14000 : 11000)
  const haloWidth = width * (glass ? 0.46 : 0.62) * (0.96 + pulse * 0.075)
  const haloHeight = glass ? height * 0.82 : haloWidth
  const x = width / 2
  const y = glass ? height * 0.46 : -height * 0.02
  ctx.save()
  ctx.globalAlpha = 0.56 + pulse * 0.34
  ctx.strokeStyle = color
  ctx.lineWidth = Math.max(1, width / 1920)
  ctx.beginPath()
  ctx.ellipse(x, y, haloWidth / 2, haloHeight / 2, 0, 0, TAU)
  ctx.stroke()
  ctx.restore()
}

function drawBeam(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  color: string,
  alpha = 0.62
): void {
  const drift = wave(timeMs, 13000)
  const center = width * (0.5 + drift * 0.025)
  const topHalf = width * 0.018
  const bottomHalf = width * 0.18
  const gradient = ctx.createLinearGradient(0, -height * 0.15, 0, height * 0.78)
  gradient.addColorStop(0, color)
  gradient.addColorStop(1, "rgba(235, 199, 117, 0.01)")
  ctx.save()
  ctx.globalAlpha = alpha * (0.72 + drift * 0.18)
  ctx.fillStyle = gradient
  ctx.beginPath()
  ctx.moveTo(center - topHalf, -height * 0.15)
  ctx.lineTo(center + topHalf, -height * 0.15)
  ctx.lineTo(center + bottomHalf, height * 0.78)
  ctx.lineTo(center - bottomHalf, height * 0.78)
  ctx.closePath()
  ctx.fill()
  ctx.restore()
}

function drawDust(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  color: string,
  alpha = 0.55
): void {
  const phase = loopPhase(timeMs, 38000)
  ctx.save()
  ctx.fillStyle = color
  for (let index = 0; index < 74; index += 1) {
    const x = (seeded(index) * width - phase * width * 0.03 + width) % width
    const rawY = seeded(index + 97) * height
    const y = (rawY - phase * height * 0.16 + height) % height
    const edgeFade = Math.sin((y / height) * Math.PI)
    const radius = 0.7 + seeded(index + 193) * 1.15
    ctx.globalAlpha = alpha * edgeFade * (0.35 + seeded(index + 211) * 0.65)
    ctx.beginPath()
    ctx.arc(x, y, radius, 0, TAU)
    ctx.fill()
  }
  ctx.restore()
}

function drawRidge(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  points: number[],
  baseY: number,
  color: string,
  timeMs: number,
  durationMs: number,
  direction: number
): void {
  const shift = wave(timeMs, durationMs) * width * 0.008 * direction
  ctx.beginPath()
  ctx.moveTo(-width * 0.04 + shift, height)
  points.forEach((value, index) => {
    const x =
      -width * 0.04 + (index / (points.length - 1)) * width * 1.08 + shift
    const y = baseY + value * height
    ctx.lineTo(x, y)
  })
  ctx.lineTo(width * 1.04 + shift, height)
  ctx.closePath()
  ctx.fillStyle = color
  ctx.fill()
}

const BACK_RIDGE = [
  0.16, 0.09, 0.14, 0.06, 0.13, 0.03, 0.11, 0.02, 0.12, 0.06, 0.13, 0.07,
]
const FRONT_RIDGE = [0.13, 0.05, 0.13, 0.02, 0.11, 0.07, 0.15, 0.04, 0.11, 0.02]

function drawSanctuaryLayers(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  accent: string,
  options: {
    auroraA?: string
    auroraB?: string
    ridgeBack?: string
    ridgeFront?: string
    beamAlpha?: number
    dustAlpha?: number
  } = {}
): void {
  drawAuroraOrb(
    ctx,
    width,
    height,
    width * 0.18,
    -height * 0.22,
    options.auroraA ?? "rgba(31, 153, 139, 0.2)",
    timeMs,
    26000,
    1
  )
  drawAuroraOrb(
    ctx,
    width,
    height,
    width * 0.82,
    height * 1.04,
    options.auroraB ?? accent,
    timeMs,
    34000,
    -1
  )
  drawHalo(ctx, width, height, timeMs, "rgba(255, 224, 151, 0.12)")
  drawBeam(
    ctx,
    width,
    height,
    timeMs,
    "rgba(255, 234, 177, 0.12)",
    options.beamAlpha
  )
  drawDust(
    ctx,
    width,
    height,
    timeMs,
    "rgba(255, 237, 190, 0.7)",
    options.dustAlpha
  )
  drawRidge(
    ctx,
    width,
    height,
    BACK_RIDGE,
    height * 0.7,
    options.ridgeBack ?? "rgba(12, 41, 43, 0.47)",
    timeMs,
    24000,
    1
  )
  drawRidge(
    ctx,
    width,
    height,
    FRONT_RIDGE,
    height * 0.77,
    options.ridgeFront ?? "#061316",
    timeMs,
    20000,
    -1
  )
}

function drawMidnight(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  refrain: boolean
): void {
  fillLinear(
    ctx,
    width,
    height,
    refrain
      ? [
          [0, "#07181c"],
          [0.46, "#14312f"],
          [1, "#151b20"],
        ]
      : [
          [0, "#06151b"],
          [0.44, "#09262b"],
          [1, "#071315"],
        ],
    refrain ? 145 : 150
  )
  radialWash(
    ctx,
    width,
    height,
    width * 0.5,
    height * (refrain ? 0.16 : 0.18),
    width * 0.32,
    refrain ? "rgba(255, 217, 145, 0.19)" : "rgba(232, 199, 124, 0.13)"
  )
  drawSanctuaryLayers(ctx, width, height, timeMs, "rgba(232, 199, 124, 0.18)")
  radialWash(
    ctx,
    width,
    height,
    width * 0.5,
    height,
    width * 0.2,
    "rgba(232, 199, 124, 0.12)"
  )
  drawVignette(ctx, width, height)
}

function drawDawn(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  refrain: boolean
): void {
  fillLinear(
    ctx,
    width,
    height,
    refrain
      ? [
          [0, "#a7c3c1"],
          [0.52, "#e4c7a5"],
          [1, "#c77b61"],
        ]
      : [
          [0, "#aabfc0"],
          [0.47, "#d6c7b0"],
          [1, "#d99875"],
        ],
    180
  )
  const pulse = 1 + wave(timeMs, 12000) * 0.055
  radialWash(
    ctx,
    width,
    height,
    width * 0.5,
    height * 0.28,
    width * 0.255 * pulse,
    refrain ? "rgba(255, 251, 222, 0.94)" : "rgba(255, 248, 216, 0.82)"
  )
  radialWash(
    ctx,
    width,
    height,
    width * 0.5,
    height * 0.25,
    width * 0.13 * pulse,
    "rgba(255, 250, 218, 0.98)"
  )
  drawRidge(
    ctx,
    width,
    height,
    BACK_RIDGE,
    height * 0.7,
    "rgba(127, 135, 129, 0.26)",
    timeMs,
    24000,
    1
  )
  drawRidge(
    ctx,
    width,
    height,
    FRONT_RIDGE,
    height * 0.77,
    "rgba(77, 85, 79, 0.42)",
    timeMs,
    20000,
    -1
  )
  drawVignette(ctx, width, height, "rgba(75, 47, 39, 0.16)")
}

function drawMinimalGrid(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number
): void {
  ctx.save()
  ctx.strokeStyle = "rgba(255, 255, 255, 0.018)"
  ctx.lineWidth = 1
  const spacing = width * (84 / 1920)
  for (let x = 0; x <= width; x += spacing) {
    ctx.beginPath()
    ctx.moveTo(x, 0)
    ctx.lineTo(x, height)
    ctx.stroke()
  }
  for (let y = 0; y <= height; y += spacing) {
    ctx.beginPath()
    ctx.moveTo(0, y)
    ctx.lineTo(width, y)
    ctx.stroke()
  }
  ctx.restore()
}

function drawMinimal(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  refrain: boolean
): void {
  ctx.fillStyle = refrain ? "#0a0d0b" : "#090b0a"
  ctx.fillRect(0, 0, width, height)
  drawMinimalGrid(ctx, width, height)
  if (refrain) {
    radialWash(
      ctx,
      width,
      height,
      width * 0.78,
      height * 0.48,
      width * 0.26,
      "rgba(173, 231, 153, 0.1)"
    )
  }
  const phase = loopPhase(timeMs, 14000)
  const archX = width * (0.72 + Math.sin(phase * TAU) * 0.006)
  const archY = height * 0.14
  const archW = Math.min(width * 0.28, height * 0.72)
  const archH = Math.min(height * 0.64, archW * 1.5)
  ctx.strokeStyle = "rgba(255, 255, 255, 0.1)"
  ctx.lineWidth = Math.max(1, width / 1920)
  ctx.beginPath()
  ctx.moveTo(archX - archW / 2, archY + archH)
  ctx.lineTo(archX - archW / 2, archY + archW / 2)
  ctx.arc(archX, archY + archW / 2, archW / 2, Math.PI, 0)
  ctx.lineTo(archX + archW / 2, archY + archH)
  ctx.stroke()
  ctx.beginPath()
  ctx.moveTo(archX, archY)
  ctx.lineTo(archX, archY + archH)
  ctx.moveTo(archX - archW / 2, archY + archH * 0.6)
  ctx.lineTo(archX + archW / 2, archY + archH * 0.6)
  ctx.stroke()
  drawVignette(ctx, width, height, "rgba(0, 0, 0, 0.32)")
}

function drawGlassField(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number
): void {
  const phase = loopPhase(timeMs, 68000)
  const angle = (18 * Math.PI) / 180 + ((phase - 0.5) * (10 * Math.PI)) / 180
  const cx = width * 0.5
  const cy = height * 0.44
  if (typeof ctx.createConicGradient === "function") {
    const conic = ctx.createConicGradient(angle, cx, cy)
    const stops: Array<[number, string]> = [
      [0, "rgba(0,0,0,0)"],
      [0.07, "rgba(51,130,176,0.36)"],
      [0.14, "rgba(0,0,0,0)"],
      [0.21, "rgba(174,39,83,0.36)"],
      [0.27, "rgba(0,0,0,0)"],
      [0.36, "rgba(234,156,50,0.3)"],
      [0.43, "rgba(0,0,0,0)"],
      [0.52, "rgba(98,54,171,0.36)"],
      [0.6, "rgba(0,0,0,0)"],
      [0.7, "rgba(29,145,125,0.28)"],
      [0.77, "rgba(0,0,0,0)"],
      [1, "rgba(0,0,0,0)"],
    ]
    for (const [stop, color] of stops) conic.addColorStop(stop, color)
    ctx.fillStyle = conic
    ctx.fillRect(0, 0, width, height)
    return
  }

  const colors = [
    "rgba(51,130,176,0.28)",
    "rgba(174,39,83,0.28)",
    "rgba(234,156,50,0.24)",
    "rgba(98,54,171,0.28)",
    "rgba(29,145,125,0.22)",
  ]
  colors.forEach((color, index) => {
    const start = angle + index * (TAU / colors.length)
    ctx.beginPath()
    ctx.moveTo(cx, cy)
    ctx.arc(cx, cy, Math.max(width, height), start, start + TAU * 0.08)
    ctx.closePath()
    ctx.fillStyle = color
    ctx.fill()
  })
}

function drawGlass(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  refrain: boolean
): void {
  fillLinear(
    ctx,
    width,
    height,
    refrain
      ? [
          [0, "#101126"],
          [0.53, "#21112b"],
          [1, "#090b16"],
        ]
      : [
          [0, "#091221"],
          [0.48, "#111228"],
          [1, "#080b15"],
        ],
    refrain ? 145 : 150
  )
  radialWash(
    ctx,
    width,
    height,
    width * 0.5,
    height * 0.42,
    width * 0.45,
    refrain ? "rgba(138, 47, 82, 0.3)" : "rgba(47, 79, 103, 0.38)"
  )
  drawGlassField(ctx, width, height, timeMs)
  drawHalo(ctx, width, height, timeMs, "rgba(255, 226, 168, 0.18)", true)
  drawVignette(ctx, width, height, "rgba(4, 5, 13, 0.62)")
}

function drawWater(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  refrain: boolean
): void {
  fillLinear(
    ctx,
    width,
    height,
    refrain
      ? [
          [0, "#07303b"],
          [0.52, "#0c4d53"],
          [1, "#0a2c37"],
        ]
      : [
          [0, "#06232f"],
          [0.52, "#0a3c47"],
          [1, "#0a2733"],
        ],
    160
  )
  radialWash(
    ctx,
    width,
    height,
    width * 0.5,
    height * 0.04,
    width * 0.34,
    refrain ? "rgba(194, 242, 232, 0.22)" : "rgba(173, 229, 224, 0.17)"
  )
  drawAuroraOrb(
    ctx,
    width,
    height,
    width * 0.18,
    -height * 0.2,
    "rgba(74, 203, 196, 0.35)",
    timeMs,
    26000,
    1
  )
  drawAuroraOrb(
    ctx,
    width,
    height,
    width * 0.82,
    height * 1.02,
    "rgba(138, 195, 244, 0.3)",
    timeMs,
    34000,
    -1
  )
  const drift = wave(timeMs, 18000)
  const centerX = width * 0.5 + drift * width * 0.02
  const centerY = height * (1.03 + drift * 0.015)
  ctx.save()
  ctx.strokeStyle = "rgba(183, 238, 243, 0.18)"
  ctx.lineWidth = Math.max(1, width / 1920)
  for (let index = 0; index < 4; index += 1) {
    const radiusX = width * (0.58 - index * 0.06)
    const radiusY = height * (0.5 - index * 0.055)
    ctx.globalAlpha = 1 - index * 0.18
    ctx.beginPath()
    ctx.ellipse(
      centerX,
      centerY - index * height * 0.045,
      radiusX,
      radiusY,
      -0.03,
      Math.PI,
      TAU
    )
    ctx.stroke()
  }
  ctx.restore()
  drawVignette(ctx, width, height, "rgba(0, 17, 25, 0.44)")
}

function drawPaperGrain(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number
): void {
  ctx.save()
  for (let index = 0; index < 210; index += 1) {
    const x = seeded(index + 401) * width
    const y = seeded(index + 809) * height
    const length = 0.5 + seeded(index + 1201) * 2.2
    ctx.globalAlpha = 0.025 + seeded(index + 1601) * 0.04
    ctx.strokeStyle = index % 2 === 0 ? "#453926" : "#ffffff"
    ctx.lineWidth = 0.5
    ctx.beginPath()
    ctx.moveTo(x, y)
    ctx.lineTo(x + length, y + seeded(index + 2003) * 0.8)
    ctx.stroke()
  }
  ctx.restore()
}

function drawHeritage(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  refrain: boolean
): void {
  ctx.fillStyle = refrain ? "#ddd5c1" : "#e9e1cf"
  ctx.fillRect(0, 0, width, height)
  const phase = loopPhase(timeMs, 14000)
  radialWash(
    ctx,
    width,
    height,
    width * (0.5 + Math.sin(phase * TAU) * 0.006),
    height * (0.4 + Math.cos(phase * TAU) * 0.004),
    width * 0.48,
    refrain ? "rgba(255, 255, 244, 0.67)" : "rgba(255, 254, 244, 0.52)"
  )
  drawPaperGrain(ctx, width, height)
  const inset = Math.min(width * 0.04, 66 * (width / 1920))
  ctx.strokeStyle = "rgba(50, 66, 51, 0.21)"
  ctx.lineWidth = Math.max(1, width / 1920)
  ctx.strokeRect(inset, inset, width - inset * 2, height - inset * 2)
  ctx.strokeStyle = "rgba(255, 255, 255, 0.25)"
  ctx.lineWidth = Math.max(4, 8 * (width / 1920))
  ctx.strokeRect(
    inset + ctx.lineWidth,
    inset + ctx.lineWidth,
    width - inset * 2 - ctx.lineWidth * 2,
    height - inset * 2 - ctx.lineWidth * 2
  )
  const diamond = Math.max(5, width * 0.004)
  for (const y of [inset, height - inset]) {
    ctx.save()
    ctx.translate(width / 2, y)
    ctx.rotate(Math.PI / 4)
    ctx.fillStyle = "rgba(62, 75, 58, 0.52)"
    ctx.fillRect(-diamond / 2, -diamond / 2, diamond, diamond)
    ctx.restore()
  }
  drawVignette(ctx, width, height, "rgba(69, 57, 38, 0.08)")
}

function drawCandleGlow(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  x: number,
  y: number,
  timeMs: number,
  offset: number
): void {
  const flicker =
    1 +
    wave(timeMs, 8000, offset) * 0.035 +
    wave(timeMs, 2300, offset * 1.7) * 0.012
  radialWash(
    ctx,
    width,
    height,
    x,
    y,
    width * 0.21 * flicker,
    "rgba(255, 171, 66, 0.2)"
  )
}

function drawUpperRoom(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  timeMs: number,
  refrain: boolean
): void {
  fillLinear(
    ctx,
    width,
    height,
    refrain
      ? [
          [0, "#17100c"],
          [0.5, "#321b10"],
          [1, "#100a08"],
        ]
      : [
          [0, "#130c0a"],
          [0.48, "#27160f"],
          [1, "#0c0908"],
        ],
    145
  )
  radialWash(
    ctx,
    width,
    height,
    width * 0.5,
    height * 0.29,
    width * 0.43,
    refrain ? "rgba(171, 91, 37, 0.27)" : "rgba(130, 72, 34, 0.2)"
  )
  drawCandleGlow(ctx, width, height, width * 0.17, height * 0.76, timeMs, 0)
  drawCandleGlow(ctx, width, height, width * 0.84, height * 0.71, timeMs, 0.35)
  drawAuroraOrb(
    ctx,
    width,
    height,
    width * 0.2,
    -height * 0.24,
    "rgba(207, 119, 45, 0.18)",
    timeMs,
    26000,
    1
  )
  drawHalo(ctx, width, height, timeMs, "rgba(244, 174, 88, 0.09)")
  drawBeam(ctx, width, height, timeMs, "rgba(244, 174, 88, 0.08)", 0.25)
  drawDust(ctx, width, height, timeMs, "rgba(255, 204, 133, 0.7)", 0.25)
  drawRidge(
    ctx,
    width,
    height,
    BACK_RIDGE,
    height * 0.7,
    "rgba(41, 23, 15, 0.22)",
    timeMs,
    24000,
    1
  )
  drawRidge(
    ctx,
    width,
    height,
    FRONT_RIDGE,
    height * 0.77,
    "rgba(13, 9, 8, 0.82)",
    timeMs,
    20000,
    -1
  )
  drawVignette(ctx, width, height, "rgba(0, 0, 0, 0.62)")
}

export function isHymnThemeScene(k: BroadcastKineticTheme): boolean {
  return HYMN_SCENE_KINDS.has(k.backgroundKind)
}

export function drawHymnThemeScene(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  timeMs: number,
  sectionKind?: HymnPresentationSectionKind
): boolean {
  const refrain = isRefrain(sectionKind)
  switch (k.backgroundKind) {
    case "hymn-midnight":
      drawMidnight(ctx, width, height, timeMs, refrain)
      return true
    case "hymn-dawn":
      drawDawn(ctx, width, height, timeMs, refrain)
      return true
    case "hymn-minimal":
      drawMinimal(ctx, width, height, timeMs, refrain)
      return true
    case "hymn-glass":
      drawGlass(ctx, width, height, timeMs, refrain)
      return true
    case "hymn-water":
      drawWater(ctx, width, height, timeMs, refrain)
      return true
    case "hymn-heritage":
      drawHeritage(ctx, width, height, timeMs, refrain)
      return true
    case "hymn-upper-room":
      drawUpperRoom(ctx, width, height, timeMs, refrain)
      return true
    default:
      return false
  }
}
