import type { BroadcastKineticTheme, BroadcastTheme } from "@/types/broadcast"
import type { HymnPresentationSectionKind } from "@/types/presentation"
import worshipPortraitUrl from "@/assets/worship-portrait.webp"
import { drawHymnThemeScene, isHymnThemeScene } from "@/lib/hymn-theme-scenes"

// ---------------------------------------------------------------------------
// Canvas-native kinetic background renderer
//
// Mirrors the HTML prototype's CSS motion (liquidMesh + vigorousDrift + the
// cyberpunk dot-grid and brutalist diagonal stripes) using only 2D canvas draw
// calls so the same moving background works for the live output AND the NDI
// frame path. It is deterministic: a given (theme, timeMs) always produces the
// same draw calls, which keeps tests stable and makes timeMs=0 a usable static
// thumbnail frame.
//
// No CSS, no DOM measurement, no external images — except the bundled Desert
// Cloth portrait asset, which is decoded once and skipped until loaded so
// determinism per (theme, timeMs, assetsLoaded) still holds.
// ---------------------------------------------------------------------------

const TAU = Math.PI * 2

export function isKineticTheme(
  theme: BroadcastTheme
): theme is BroadcastTheme & { kinetic: BroadcastKineticTheme } {
  return Boolean(theme.kinetic && theme.kinetic.animate !== false)
}

/** Normalized loop position in [0, 1). Stable at timeMs=0; 0 when no duration. */
export function kineticLoopPhase(timeMs: number, durationMs: number): number {
  if (!Number.isFinite(timeMs) || durationMs <= 0) return 0
  const wrapped = ((timeMs % durationMs) + durationMs) % durationMs
  return wrapped / durationMs
}

function colorsOf(k: BroadcastKineticTheme): string[] {
  return k.colors.length > 0 ? k.colors : ["#05060f", "#11142b"]
}

function drawMeshBase(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  phase: number
): void {
  const colors = colorsOf(k)
  // Rotate the gradient axis over the loop (the prototype shifts
  // background-position; an angle sweep reads the same on a flat canvas).
  const angle =
    (135 + Math.sin(phase * TAU) * 25 * Math.max(0.1, k.motion.driftAmount)) *
    (Math.PI / 180)
  const cx = width / 2
  const cy = height / 2
  const len = Math.sqrt(width * width + height * height) / 2
  const grad = ctx.createLinearGradient(
    cx - Math.cos(angle) * len,
    cy - Math.sin(angle) * len,
    cx + Math.cos(angle) * len,
    cy + Math.sin(angle) * len
  )
  for (let i = 0; i < colors.length; i++) {
    grad.addColorStop(i / Math.max(1, colors.length - 1), colors[i])
  }
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)
}

function drawDriftBlobs(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  phase: number
): void {
  const colors = colorsOf(k)
  const drift = Math.max(0, k.motion.driftAmount)
  const radius = Math.max(width, height) * 0.55

  ctx.save()
  // Soft, additive luminous blobs like the ambient drift layer. Filters are
  // guarded because some WebViews / test contexts don't support them.
  try {
    ctx.globalAlpha = 0.55
  } catch {
    /* noop */
  }

  const blobs = Math.min(4, colors.length)
  for (let i = 0; i < blobs; i++) {
    const t = phase * TAU + (i / blobs) * TAU
    const ox = Math.cos(t) * drift * width * 0.22
    const oy = Math.sin(t * 0.8 + i) * drift * height * 0.22
    const baseX = width * (0.3 + 0.4 * (i % 2 === 0 ? 0 : 1))
    const baseY = height * (0.32 + 0.36 * (i < 2 ? 0 : 1))
    const grad = ctx.createRadialGradient(
      baseX + ox,
      baseY + oy,
      0,
      baseX + ox,
      baseY + oy,
      radius
    )
    grad.addColorStop(0, colors[i % colors.length])
    grad.addColorStop(1, "rgba(0,0,0,0)")
    ctx.fillStyle = grad
    ctx.fillRect(0, 0, width, height)
  }

  // Accent glow that breathes with the loop (cyberPulse).
  const pulse = 0.3 + 0.25 * (0.5 + 0.5 * Math.sin(phase * TAU))
  const glowX = width * 0.5 + Math.cos(phase * TAU) * width * 0.18 * drift
  const glowY = height * 0.5 + Math.sin(phase * TAU) * height * 0.18 * drift
  const glow = ctx.createRadialGradient(
    glowX,
    glowY,
    0,
    glowX,
    glowY,
    radius * 0.7
  )
  glow.addColorStop(0, k.accentColor)
  glow.addColorStop(1, "rgba(0,0,0,0)")
  ctx.save()
  ctx.globalAlpha = pulse
  ctx.fillStyle = glow
  ctx.fillRect(0, 0, width, height)
  ctx.restore()

  ctx.restore()
}

function drawDotGrid(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  phase: number
): void {
  const spacing = Math.max(24, Math.round(width / 48))
  const dotRadius = Math.max(1.5, spacing * 0.08)
  // Scroll the grid diagonally over the loop (gridScroll).
  const shift = (phase * spacing) % spacing
  ctx.save()
  ctx.globalAlpha = 0.5
  ctx.fillStyle = k.accentColor
  for (let y = -spacing; y < height + spacing; y += spacing) {
    for (let x = -spacing; x < width + spacing; x += spacing) {
      ctx.beginPath()
      ctx.arc(x + shift, y + shift, dotRadius, 0, TAU)
      ctx.fill()
    }
  }
  ctx.restore()
}

function drawDiagonalStripes(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  phase: number
): void {
  const band = Math.max(28, Math.round(width / 30))
  const period = band * 2
  const shift = (phase * period) % period
  const span = width + height
  ctx.save()
  ctx.globalAlpha = 0.08
  ctx.fillStyle = k.accentColor
  // Diagonal (45deg) stripes as filled parallelograms sweeping across.
  for (let offset = -span; offset < span; offset += period) {
    const x = offset + shift
    ctx.beginPath()
    ctx.moveTo(x, 0)
    ctx.lineTo(x + band, 0)
    ctx.lineTo(x + band - height, height)
    ctx.lineTo(x - height, height)
    ctx.closePath()
    ctx.fill()
  }
  ctx.restore()
}

// ---------------------------------------------------------------------------
// Nature scenes
//
// Deterministic particle systems (rain, snow, drifting leaves/petals, glowing
// motes, stars, aurora). Every element's position is a pure function of its
// index seed and timeMs, so a given (theme, timeMs) always produces the same
// frame — timeMs=0 is a stable thumbnail — with no per-frame state retained.
// Only cheap primitives are used (arcs, lines, filled polygons, one gradient);
// no shadow blur or per-particle gradients, so the 15fps CPU budget holds.
// ---------------------------------------------------------------------------

const NATURE_KINDS: ReadonlySet<string> = new Set([
  "foliage",
  "forest",
  "rain",
  "autumn",
  "blossom",
  "snow",
  "fireflies",
  "stars",
  "meadow",
  "aurora",
])

/** Deterministic [0, 1) hash for a particle index. */
function srand(n: number): number {
  const x = Math.sin(n * 127.1) * 43758.5453
  return x - Math.floor(x)
}

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "")
  const full =
    h.length === 3
      ? h
          .split("")
          .map((c) => c + c)
          .join("")
      : h
  const int = Number.parseInt(full, 16)
  return [(int >> 16) & 255, (int >> 8) & 255, int & 255]
}

/** Overall particle speed multiplier from the preset's drift amount. */
function driftSpeed(k: BroadcastKineticTheme): number {
  return 0.5 + Math.max(0, k.motion.driftAmount)
}

/** Calm vertical gradient backdrop behind the particles. */
function drawNatureBackdrop(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme
): void {
  const colors = colorsOf(k)
  const grad = ctx.createLinearGradient(0, 0, 0, height)
  for (let i = 0; i < colors.length; i++) {
    grad.addColorStop(i / Math.max(1, colors.length - 1), colors[i])
  }
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)
}

// --- Static environment layer -----------------------------------------------
//
// Each nature scene sits in a painted environment (silhouettes, haze, light
// shafts, vignette). That layer never animates, so it is painted once to an
// offscreen canvas and blitted per frame; the gradient-heavy richness costs
// nothing on the 15fps budget. Without a DOM (tests, NDI edge cases) the same
// painter runs inline on the target context instead.

function rgba(r: number, g: number, b: number, a: number): string {
  return `rgba(${Math.max(0, Math.min(255, Math.round(r)))}, ${Math.max(0, Math.min(255, Math.round(g)))}, ${Math.max(0, Math.min(255, Math.round(b)))}, ${a})`
}

/** A soft horizontal ridge/treeline silhouette filled down to the bottom. */
function paintRidge(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  opts: {
    baseY: number
    amp: number
    seed: number
    color: string
    jagged?: boolean
  }
): void {
  const steps = opts.jagged ? 14 : 8
  ctx.beginPath()
  ctx.moveTo(0, height + 2)
  let py = opts.baseY + (srand(opts.seed) - 0.5) * 2 * opts.amp
  ctx.lineTo(0, py)
  for (let s = 1; s <= steps; s++) {
    const x = (s / steps) * width
    const y = opts.baseY + (srand(opts.seed + s) - 0.5) * 2 * opts.amp
    const peak = opts.jagged
      ? opts.amp * (0.5 + srand(opts.seed + s + 57))
      : opts.amp * 0.4
    ctx.quadraticCurveTo(x - width / steps / 2, Math.min(py, y) - peak, x, y)
    py = y
  }
  ctx.lineTo(width, height + 2)
  ctx.closePath()
  ctx.fillStyle = opts.color
  ctx.fill()
}

/** Darkened corners so scenes read as lit from within, not flat. */
function paintVignette(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  strength: number
): void {
  const grad = ctx.createRadialGradient(
    width / 2,
    height / 2,
    Math.min(width, height) * 0.42,
    width / 2,
    height / 2,
    Math.max(width, height) * 0.78
  )
  grad.addColorStop(0, "rgba(0, 0, 0, 0)")
  grad.addColorStop(1, `rgba(0, 0, 0, ${strength})`)
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)
}

/** Diagonal translucent light shafts from an upper corner. */
function paintLightShafts(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  tint: [number, number, number]
): void {
  ctx.save()
  ctx.globalCompositeOperation = "screen"
  for (let i = 0; i < 4; i++) {
    const topX = width * (0.14 + i * 0.19 + srand(i + 71) * 0.07)
    const spread = width * (0.014 + srand(i + 73) * 0.02)
    const botX = topX + width * 0.16
    // Fade out before the ground so shafts read as light, not painted wedges.
    const grad = ctx.createLinearGradient(topX, 0, botX, height)
    const alpha = 0.1 - i * 0.018
    grad.addColorStop(0, rgba(tint[0], tint[1], tint[2], alpha))
    grad.addColorStop(0.55, rgba(tint[0], tint[1], tint[2], alpha * 0.5))
    grad.addColorStop(0.85, rgba(tint[0], tint[1], tint[2], 0))
    grad.addColorStop(1, rgba(tint[0], tint[1], tint[2], 0))
    ctx.beginPath()
    ctx.moveTo(topX - spread * 0.4, -2)
    ctx.lineTo(topX + spread, -2)
    ctx.lineTo(botX + spread * 2.4, height)
    ctx.lineTo(botX - spread * 1.4, height)
    ctx.closePath()
    ctx.fillStyle = grad
    ctx.fill()
  }
  ctx.restore()
}

/** Trunk-and-canopy silhouette layers for forest-family scenes. */
function paintForestEnvironment(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  warm: boolean
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  // Far-to-near tree layers: farther layers are lighter and hazier.
  for (let layer = 0; layer < 3; layer++) {
    const near = layer / 2
    const alpha = 0.14 + near * 0.24
    const shade = 0.3 - near * 0.16
    const trunkColor = warm
      ? rgba(r * shade * 1.5, g * shade * 0.8, b * shade * 0.6, alpha)
      : rgba(r * shade * 0.7, g * shade, b * shade * 0.7, alpha)
    const count = 6 - layer * 2
    for (let i = 0; i < count; i++) {
      const seed = layer * 37 + i
      const x = ((i + 0.18 + srand(seed) * 0.66) / count) * width
      const trunkW =
        width * (0.006 + near * 0.011) * (0.8 + srand(seed + 3) * 0.5)
      const lean = (srand(seed + 5) - 0.5) * width * 0.03
      ctx.beginPath()
      ctx.moveTo(x - trunkW, height)
      ctx.quadraticCurveTo(
        x - trunkW * 0.5 + lean * 0.5,
        height * 0.4,
        x + lean - trunkW * 0.32,
        height * 0.04
      )
      ctx.lineTo(x + lean + trunkW * 0.32, height * 0.04)
      ctx.quadraticCurveTo(
        x + trunkW * 0.6 + lean * 0.5,
        height * 0.42,
        x + trunkW,
        height
      )
      ctx.closePath()
      ctx.fillStyle = trunkColor
      ctx.fill()
      // Branch hints partway up the trunk.
      ctx.strokeStyle = trunkColor
      ctx.lineWidth = Math.max(1, trunkW * 0.4)
      for (let br = 0; br < 2; br++) {
        const by = height * (0.18 + srand(seed + 11 + br) * 0.3)
        const dir = srand(seed + 17 + br) > 0.5 ? 1 : -1
        ctx.beginPath()
        ctx.moveTo(x + lean * (1 - by / height), by)
        ctx.quadraticCurveTo(
          x + dir * width * 0.03,
          by - height * 0.04,
          x + dir * width * (0.05 + srand(seed + 19 + br) * 0.03),
          by - height * (0.06 + srand(seed + 23 + br) * 0.04)
        )
        ctx.stroke()
      }
    }
    // Canopy mass hanging from the top of this layer.
    const canopy = ctx.createLinearGradient(
      0,
      0,
      0,
      height * (0.3 + near * 0.1)
    )
    canopy.addColorStop(
      0,
      warm
        ? rgba(r * 0.5, g * 0.3, b * 0.16, 0.2 + near * 0.16)
        : rgba(r * 0.2, g * 0.34, b * 0.2, 0.2 + near * 0.16)
    )
    canopy.addColorStop(1, "rgba(0, 0, 0, 0)")
    ctx.fillStyle = canopy
    ctx.fillRect(0, 0, width, height * (0.3 + near * 0.1))
  }
  paintLightShafts(ctx, width, height, warm ? [255, 214, 150] : [214, 240, 200])
  // Dappled ground shading.
  const ground = ctx.createLinearGradient(0, height * 0.82, 0, height)
  ground.addColorStop(0, "rgba(0, 0, 0, 0)")
  ground.addColorStop(
    1,
    warm ? rgba(r * 0.34, g * 0.2, b * 0.1, 0.4) : "rgba(6, 14, 8, 0.42)"
  )
  ctx.fillStyle = ground
  ctx.fillRect(0, height * 0.82, width, height * 0.18)
}

/** Carpet of settled leaves along the bottom (autumn). */
function paintLeafCarpet(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  for (let i = 0; i < 46; i++) {
    const x = srand(i + 201) * width
    const y = height - srand(i + 203) * height * 0.05
    const s = 3 + srand(i + 207) * 6
    const shade = 0.5 + srand(i + 211) * 0.6
    ctx.beginPath()
    ctx.ellipse(x, y, s, s * 0.42, srand(i + 213) * TAU, 0, TAU)
    ctx.fillStyle = rgba(r * shade, g * shade * 0.8, b * shade * 0.6, 0.4)
    ctx.fill()
  }
}

/** Blossom branches reaching in from the top corners. */
function paintBlossomBranches(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  const branchColor = "rgba(52, 34, 38, 0.85)"
  for (let side = 0; side < 2; side++) {
    const sx = side === 0 ? -width * 0.02 : width * 1.02
    const dir = side === 0 ? 1 : -1
    for (let br = 0; br < 3; br++) {
      const seed = side * 53 + br * 7
      const sy = height * (0.02 + srand(seed) * 0.16)
      const ex = sx + dir * width * (0.2 + srand(seed + 3) * 0.16)
      const ey = sy + height * (0.08 + srand(seed + 5) * 0.14)
      const cx = sx + dir * width * 0.1
      const cy = sy + height * 0.01
      // Tapered branch: stroke twice, thick near the trunk-side edge.
      ctx.strokeStyle = branchColor
      ctx.lineWidth = Math.max(2, width * 0.006 * (1 - br * 0.2))
      ctx.beginPath()
      ctx.moveTo(sx, sy)
      ctx.quadraticCurveTo((sx + cx) / 2, (sy + cy) / 2, cx, cy)
      ctx.stroke()
      ctx.lineWidth = Math.max(1, width * 0.0028 * (1 - br * 0.2))
      ctx.beginPath()
      ctx.moveTo(cx, cy)
      ctx.quadraticCurveTo(cx + dir * width * 0.05, cy + height * 0.02, ex, ey)
      ctx.stroke()
      // Small twigs with tight blossom tufts hugging the branch line.
      for (let c = 0; c < 9; c++) {
        const p = 0.24 + (c / 9) * 0.76
        const bx =
          sx + (ex - sx) * p + (srand(seed + c + 11) - 0.5) * width * 0.008
        const by =
          sy + (ey - sy) * p * p + (srand(seed + c + 13) - 0.5) * height * 0.014
        for (let petal = 0; petal < 3; petal++) {
          const px =
            bx + (srand(seed + c * 3 + petal + 29) - 0.5) * width * 0.007
          const py =
            by + (srand(seed + c * 3 + petal + 31) - 0.5) * width * 0.007
          const cr = width * 0.0016 * (1 + srand(seed + c + petal + 17))
          ctx.beginPath()
          ctx.arc(px, py, cr, 0, TAU)
          ctx.fillStyle = rgba(
            Math.min(255, r * 1.1 + 40),
            g * 0.92 + 30,
            b * 0.95 + 30,
            0.32 + srand(seed + c + petal + 19) * 0.3
          )
          ctx.fill()
        }
      }
    }
  }
}

/** Heavy cloud bank, misty horizon and wet-ground sheen for rain. */
function paintRainEnvironment(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  // Layered cloud bank: overlapping soft ellipses across the top.
  for (let i = 0; i < 12; i++) {
    const x = ((i + srand(i + 301) * 0.9) / 12) * width
    const y = height * (0.02 + srand(i + 303) * 0.1)
    const rw = width * (0.14 + srand(i + 307) * 0.12)
    const shade = 0.06 + srand(i + 311) * 0.08
    const cloud = ctx.createRadialGradient(x, y, 0, x, y, rw)
    cloud.addColorStop(
      0,
      rgba(r * 0.3 + 14, g * 0.3 + 16, b * 0.3 + 22, 0.3 + shade)
    )
    cloud.addColorStop(1, "rgba(0, 0, 0, 0)")
    ctx.beginPath()
    ctx.ellipse(x, y, rw, rw * 0.36, 0, 0, TAU)
    ctx.fillStyle = cloud
    ctx.fill()
  }
  // Misty horizon band.
  const mist = ctx.createLinearGradient(0, height * 0.55, 0, height * 0.78)
  mist.addColorStop(0, "rgba(0, 0, 0, 0)")
  mist.addColorStop(0.5, rgba(r * 0.5 + 40, g * 0.5 + 44, b * 0.5 + 52, 0.1))
  mist.addColorStop(1, "rgba(0, 0, 0, 0)")
  ctx.fillStyle = mist
  ctx.fillRect(0, height * 0.55, width, height * 0.23)
  // Wet ground with a reflective sheen.
  paintRidge(ctx, width, height, {
    baseY: height * 0.9,
    amp: height * 0.008,
    seed: 331,
    color: "rgba(8, 12, 18, 0.5)",
  })
  const sheen = ctx.createLinearGradient(0, height * 0.9, 0, height)
  sheen.addColorStop(0, rgba(r * 0.7 + 50, g * 0.7 + 54, b * 0.7 + 64, 0.12))
  sheen.addColorStop(1, "rgba(0, 0, 0, 0)")
  ctx.fillStyle = sheen
  ctx.fillRect(0, height * 0.9, width, height * 0.1)
}

/** Moonlit sky, snowy hills and ground band for snow. */
function paintSnowEnvironment(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number
): void {
  const moonX = width * 0.76
  const moonY = height * 0.16
  const glow = ctx.createRadialGradient(
    moonX,
    moonY,
    0,
    moonX,
    moonY,
    width * 0.24
  )
  glow.addColorStop(0, "rgba(232, 240, 255, 0.3)")
  glow.addColorStop(0.14, "rgba(226, 236, 255, 0.14)")
  glow.addColorStop(1, "rgba(0, 0, 0, 0)")
  ctx.fillStyle = glow
  ctx.fillRect(0, 0, width, height)
  ctx.beginPath()
  ctx.arc(moonX, moonY, width * 0.023, 0, TAU)
  ctx.fillStyle = "rgba(240, 246, 255, 0.85)"
  ctx.fill()
  paintRidge(ctx, width, height, {
    baseY: height * 0.78,
    amp: height * 0.05,
    seed: 401,
    color: "rgba(198, 212, 234, 0.16)",
  })
  paintRidge(ctx, width, height, {
    baseY: height * 0.88,
    amp: height * 0.035,
    seed: 409,
    color: "rgba(214, 226, 244, 0.24)",
  })
}

/** Milky-way band and horizon silhouette for night skies. */
function paintNightEnvironment(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  // Diagonal milky-way glow.
  ctx.save()
  ctx.globalCompositeOperation = "screen"
  const bandGrad = ctx.createLinearGradient(
    width * 0.1,
    height * 0.9,
    width * 0.9,
    0
  )
  bandGrad.addColorStop(0, "rgba(0, 0, 0, 0)")
  bandGrad.addColorStop(
    0.5,
    rgba(r * 0.4 + 40, g * 0.4 + 44, b * 0.4 + 60, 0.09)
  )
  bandGrad.addColorStop(1, "rgba(0, 0, 0, 0)")
  ctx.fillStyle = bandGrad
  ctx.fillRect(0, 0, width, height)
  // Dust: faint static pinpricks concentrated along the band diagonal.
  for (let i = 0; i < 130; i++) {
    const p = srand(i + 501)
    const bx = width * (0.08 + p * 0.86)
    const by = height * (0.88 - p * 0.82)
    const off = (srand(i + 503) - 0.5) * height * 0.24
    const x = bx + off * 0.5
    const y = by + off
    ctx.beginPath()
    ctx.arc(x, y, 0.3 + srand(i + 507) * 0.9, 0, TAU)
    ctx.fillStyle = rgba(220, 226, 244, 0.06 + srand(i + 509) * 0.16)
    ctx.fill()
  }
  ctx.restore()
  // Horizon treeline.
  paintRidge(ctx, width, height, {
    baseY: height * 0.93,
    amp: height * 0.028,
    seed: 521,
    color: "rgba(4, 8, 12, 0.75)",
    jagged: true,
  })
}

/** Dusk treeline and grass edge for fireflies. */
function paintFirefliesEnvironment(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number
): void {
  paintRidge(ctx, width, height, {
    baseY: height * 0.72,
    amp: height * 0.05,
    seed: 601,
    color: "rgba(8, 16, 14, 0.45)",
    jagged: true,
  })
  paintRidge(ctx, width, height, {
    baseY: height * 0.88,
    amp: height * 0.03,
    seed: 607,
    color: "rgba(4, 10, 8, 0.7)",
    jagged: true,
  })
  // Static foreground grass blades.
  ctx.strokeStyle = "rgba(3, 8, 6, 0.8)"
  for (let i = 0; i < 30; i++) {
    const x = srand(i + 611) * width
    const bladeH = height * (0.05 + srand(i + 613) * 0.08)
    const bend = (srand(i + 617) - 0.5) * 26
    ctx.lineWidth = 0.8 + srand(i + 619) * 1.6
    ctx.beginPath()
    ctx.moveTo(x, height + 2)
    ctx.quadraticCurveTo(
      x + bend * 0.4,
      height - bladeH * 0.55,
      x + bend,
      height - bladeH
    )
    ctx.stroke()
  }
}

/** Sunlit rolling hills and flower heads for the meadow. */
function paintMeadowEnvironment(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  const sun = ctx.createRadialGradient(
    width * 0.22,
    height * 0.1,
    0,
    width * 0.22,
    height * 0.1,
    width * 0.4
  )
  sun.addColorStop(0, "rgba(255, 244, 214, 0.26)")
  sun.addColorStop(1, "rgba(0, 0, 0, 0)")
  ctx.fillStyle = sun
  ctx.fillRect(0, 0, width, height)
  paintRidge(ctx, width, height, {
    baseY: height * 0.7,
    amp: height * 0.05,
    seed: 701,
    color: rgba(r * 0.5, g * 0.62, b * 0.4, 0.2),
  })
  paintRidge(ctx, width, height, {
    baseY: height * 0.84,
    amp: height * 0.04,
    seed: 709,
    color: rgba(r * 0.38, g * 0.5, b * 0.3, 0.3),
  })
  // Scattered flower heads near the foreground.
  for (let i = 0; i < 18; i++) {
    const x = srand(i + 721) * width
    const y = height * (0.9 + srand(i + 723) * 0.08)
    const fr = 1.2 + srand(i + 727) * 2
    ctx.beginPath()
    ctx.arc(x, y, fr, 0, TAU)
    ctx.fillStyle =
      srand(i + 729) > 0.5
        ? "rgba(255, 240, 210, 0.5)"
        : rgba(Math.min(255, r * 1.2 + 40), g, b, 0.45)
    ctx.fill()
  }
}

/** Paint the full static environment for a nature scene onto a context. */
function paintNatureEnvironment(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme
): void {
  drawNatureBackdrop(ctx, width, height, k)
  switch (k.backgroundKind) {
    case "forest":
    case "foliage":
      paintForestEnvironment(ctx, width, height, k, false)
      break
    case "autumn":
      paintForestEnvironment(ctx, width, height, k, true)
      paintLeafCarpet(ctx, width, height, k)
      break
    case "blossom":
      paintBlossomBranches(ctx, width, height, k)
      break
    case "rain":
      paintRainEnvironment(ctx, width, height, k)
      break
    case "snow":
      paintSnowEnvironment(ctx, width, height)
      break
    case "stars":
    case "aurora":
      paintNightEnvironment(ctx, width, height, k)
      break
    case "fireflies":
      paintFirefliesEnvironment(ctx, width, height)
      break
    case "meadow":
      paintMeadowEnvironment(ctx, width, height, k)
      break
    default:
      break
  }
  paintVignette(ctx, width, height, 0.34)
}

interface EnvCacheEntry {
  key: string
  canvas: HTMLCanvasElement
}

const envLayerCache: EnvCacheEntry[] = []
const ENV_LAYER_CACHE_MAX = 6

/**
 * Cached offscreen canvas holding a scene's static environment. Returns null
 * when no DOM canvas is available (tests, headless paths); callers then paint
 * the environment inline instead.
 */
export function getNatureEnvironmentLayer(
  k: BroadcastKineticTheme,
  width: number,
  height: number
): HTMLCanvasElement | null {
  if (!(width > 0) || !(height > 0)) return null
  if (
    typeof document === "undefined" ||
    typeof document.createElement !== "function"
  ) {
    return null
  }
  const key = `${k.backgroundKind}|${width}x${height}|${colorsOf(k).join(",")}|${k.accentColor}`
  const hit = envLayerCache.find((e) => e.key === key)
  if (hit) return hit.canvas
  let canvas: HTMLCanvasElement
  try {
    canvas = document.createElement("canvas")
  } catch {
    return null
  }
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext("2d")
  if (!ctx) return null
  paintNatureEnvironment(ctx, width, height, k)
  envLayerCache.push({ key, canvas })
  if (envLayerCache.length > ENV_LAYER_CACHE_MAX) envLayerCache.shift()
  return canvas
}

function drawFallingLeaves(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number,
  opts: { count: number; size: number; petal: boolean }
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  const speed = 0.12 * driftSpeed(k)
  const range = height + 80
  for (let i = 0; i < opts.count; i++) {
    const phase = srand(i + 31) * TAU
    const swayAmp = 20 + srand(i + 17) * 30
    const fall =
      (srand(i + 3) * range + t * speed * (0.6 + srand(i + 5) * 0.8)) % range
    const y = fall - 40
    // Periodic wind gusts slide leaves sideways in coherent pushes; nearer
    // (larger) leaves are pushed harder for parallax.
    const sizeF = 0.7 + srand(i + 13) * 0.6
    const gust =
      Math.pow(0.5 + 0.5 * Math.sin(t * 0.00021 + phase * 0.5), 3) *
      55 *
      ((sizeF - 0.7) / 0.6 + 0.35)
    const x =
      srand(i) * width +
      gust +
      Math.sin(t * 0.0006 + phase) * swayAmp +
      Math.sin(t * 0.0014 + phase * 0.7) * swayAmp * 0.22
    const s = opts.size * sizeF
    // Rock around a resting angle instead of spinning like confetti, and
    // flip about the long axis so leaves periodically turn edge-on.
    const dir = srand(i + 7) > 0.5 ? 1 : -1
    const rot =
      phase + dir * Math.sin(t * 0.0009 + phase * 2) * 0.6 + t * 0.00008 * dir
    const flip = 0.3 + 0.7 * Math.abs(Math.cos(t * 0.0011 + phase * 3))
    // Smaller leaves read as farther away: fade and desaturate for depth.
    const depth = (sizeF - 0.7) / 0.6
    const alpha = 0.22 + depth * 0.62
    // Per-leaf shade of the accent so a fall isn't one flat color.
    const shade = 0.7 + srand(i + 23) * 0.5
    const lr = Math.min(255, Math.round(r * shade))
    const lg = Math.min(255, Math.round(g * (0.85 + srand(i + 29) * 0.3)))
    const lb = Math.min(255, Math.round(b * shade))
    const darkR = Math.round(lr * 0.48)
    const darkG = Math.round(lg * 0.48)
    const darkB = Math.round(lb * 0.48)
    ctx.save()
    ctx.translate(x, y)
    ctx.rotate(rot)
    ctx.scale(flip, 1)
    const grad = ctx.createLinearGradient(-s * 0.65, -s, s * 0.7, s)
    grad.addColorStop(
      0,
      `rgba(${Math.min(255, Math.round(lr * 1.18))}, ${Math.min(255, Math.round(lg * 1.14))}, ${Math.min(255, Math.round(lb * 1.08))}, ${alpha})`
    )
    grad.addColorStop(0.56, `rgba(${lr}, ${lg}, ${lb}, ${alpha})`)
    grad.addColorStop(1, `rgba(${darkR}, ${darkG}, ${darkB}, ${alpha})`)
    ctx.shadowColor = `rgba(0, 0, 0, ${0.12 + depth * 0.08})`
    ctx.shadowBlur = Math.max(1, s * 0.12)
    ctx.shadowOffsetY = Math.max(0.5, s * 0.06)
    ctx.beginPath()
    if (opts.petal) {
      const petalW = s * (0.36 + srand(i + 37) * 0.18)
      ctx.moveTo(0, -s * 0.5)
      ctx.quadraticCurveTo(petalW, -s * 0.28, petalW * 0.66, s * 0.18)
      ctx.quadraticCurveTo(petalW * 0.3, s * 0.58, 0, s * 0.5)
      ctx.quadraticCurveTo(-petalW * 0.72, s * 0.28, -petalW * 0.56, -s * 0.16)
      ctx.quadraticCurveTo(-petalW * 0.35, -s * 0.42, 0, -s * 0.5)
    } else {
      const rightW = s * (0.42 + srand(i + 37) * 0.2)
      const leftW = s * (0.38 + srand(i + 41) * 0.22)
      const waist = -s * (0.12 + srand(i + 43) * 0.2)
      ctx.moveTo(0, -s)
      ctx.quadraticCurveTo(rightW * 0.95, -s * 0.68, rightW * 0.78, waist)
      ctx.quadraticCurveTo(rightW * 0.55, s * 0.52, 0, s)
      ctx.quadraticCurveTo(-leftW * 0.62, s * 0.44, -leftW * 0.78, waist)
      ctx.quadraticCurveTo(-leftW * 0.9, -s * 0.72, 0, -s)
    }
    ctx.closePath()
    ctx.fillStyle = grad
    ctx.fill()
    ctx.shadowColor = "transparent"
    ctx.shadowBlur = 0
    ctx.shadowOffsetY = 0
    if (!opts.petal) {
      ctx.strokeStyle = `rgba(${darkR}, ${darkG}, ${darkB}, ${alpha * 0.72})`
      ctx.lineWidth = Math.max(0.7, s * 0.045)
      ctx.beginPath()
      ctx.moveTo(0, -s * 0.9)
      ctx.quadraticCurveTo(s * 0.05, s * 0.08, 0, s * 1.22)
      ctx.stroke()
      ctx.lineWidth = Math.max(0.45, s * 0.025)
      // Far leaves are too small/soft to show veins; skip for depth of field.
      for (let vein = 0; vein < (depth > 0.3 ? 3 : 0); vein++) {
        const vy = -s * 0.48 + vein * s * 0.42
        const right = s * (0.3 - vein * 0.045)
        const left = s * (0.25 - vein * 0.035)
        ctx.beginPath()
        ctx.moveTo(0, vy)
        ctx.lineTo(right, vy - s * (0.16 - vein * 0.025))
        ctx.moveTo(0, vy + s * 0.1)
        ctx.lineTo(-left, vy - s * (0.03 - vein * 0.015))
        ctx.stroke()
      }
    } else {
      ctx.strokeStyle = `rgba(${darkR}, ${darkG}, ${darkB}, ${alpha * 0.32})`
      ctx.lineWidth = Math.max(0.45, s * 0.035)
      ctx.beginPath()
      ctx.moveTo(0, -s * 0.38)
      ctx.quadraticCurveTo(s * 0.04, 0, 0, s * 0.42)
      ctx.stroke()
    }
    ctx.restore()
  }
}

function drawRain(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  const speed = 0.7 * driftSpeed(k)
  const range = height + 40
  const mist = ctx.createLinearGradient(0, height * 0.64, 0, height)
  mist.addColorStop(0, `rgba(${r}, ${g}, ${b}, 0)`)
  mist.addColorStop(1, `rgba(${r}, ${g}, ${b}, 0.13)`)
  ctx.fillStyle = mist
  ctx.fillRect(0, height * 0.64, width, height * 0.36)

  for (let layer = 0; layer < 3; layer++) {
    const count = 48 + layer * 34
    const alpha = 0.12 + layer * 0.08
    const slant = 3 + layer * 2.4
    ctx.strokeStyle = `rgba(${r}, ${g}, ${b}, ${alpha})`
    ctx.lineWidth = 0.7 + layer * 0.45
    for (let i = 0; i < count; i++) {
      const seed = i + layer * 173
      const len = 8 + layer * 8 + srand(seed + 7) * (12 + layer * 7)
      const drift = t * speed * (0.35 + layer * 0.25)
      const x =
        ((srand(seed) * (width + 80) + drift * 0.22) % (width + 80)) - 40
      const y =
        (srand(seed + 3) * range +
          t * speed * (0.55 + layer * 0.32 + srand(seed + 9) * 0.45)) %
        range
      ctx.beginPath()
      ctx.moveTo(x, y)
      ctx.lineTo(x - slant, y - len)
      ctx.stroke()
    }
  }
}

function drawSnow(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  const speed = 0.06 * driftSpeed(k)
  const range = height + 20
  for (let i = 0; i < 80; i++) {
    const depth = srand(i + 5)
    const radius = 1 + depth * 3
    const y = (srand(i + 3) * range + t * speed * (0.4 + depth)) % range
    // Two-harmonic wind gives coherent gusts; deeper (nearer) flakes drift
    // farther for parallax.
    const wind =
      (Math.sin(t * 0.00025) + 0.55 * Math.sin(t * 0.00047 + 1.7)) * 22 * depth
    const x =
      srand(i) * width +
      wind +
      Math.sin(t * 0.0008 + srand(i + 11) * TAU) * (4 + depth * 8)
    const glow = ctx.createRadialGradient(x, y, 0, x, y, radius * 2.3)
    glow.addColorStop(0, `rgba(248, 252, 255, ${0.32 + depth * 0.34})`)
    glow.addColorStop(1, "rgba(248, 252, 255, 0)")
    ctx.beginPath()
    ctx.arc(x, y, radius * 2.3, 0, TAU)
    ctx.fillStyle = glow
    ctx.fill()
    ctx.beginPath()
    ctx.arc(x, y, radius, 0, TAU)
    ctx.fillStyle = `rgba(240, 248, 255, ${0.42 + depth * 0.48})`
    ctx.fill()
    if (depth > 0.72) {
      ctx.strokeStyle = `rgba(240, 248, 255, ${0.22 + depth * 0.32})`
      ctx.lineWidth = Math.max(0.45, radius * 0.18)
      for (let arm = 0; arm < 3; arm++) {
        const angle = (arm / 3) * TAU + t * 0.00012
        const dx = Math.cos(angle) * radius * 2.1
        const dy = Math.sin(angle) * radius * 2.1
        ctx.beginPath()
        ctx.moveTo(x - dx, y - dy)
        ctx.lineTo(x + dx, y + dy)
        ctx.stroke()
      }
    }
  }
}

/** Rising glowing motes — fireflies and drifting pollen. */
function drawGlowMotes(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number,
  count: number
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  const isMeadow = k.backgroundKind === "meadow"
  // Fireflies glow warm amber regardless of how green the accent runs.
  const fr = isMeadow ? r : Math.min(255, Math.round(r * 0.6 + 150))
  const fg = isMeadow ? g : Math.min(255, Math.round(g * 0.6 + 110))
  const fb = isMeadow ? b : Math.round(b * 0.4)
  const speed = (isMeadow ? 0.028 : 0.05) * driftSpeed(k)
  const range = height + 40
  for (let i = 0; i < count; i++) {
    const phase = srand(i + 23) * TAU
    const pulse = isMeadow
      ? 0.55 + 0.25 * Math.sin(t * 0.0012 + phase)
      : Math.pow(0.5 + 0.5 * Math.sin(t * 0.003 + phase), 3)
    const raw =
      (srand(i + 3) * range + t * speed * (0.45 + srand(i + 5) * 0.8)) % range
    const yBase = height - raw
    // Fireflies wander on Lissajous-like paths (two incommensurate sines per
    // axis) rather than rising in straight lanes.
    const x =
      srand(i) * width +
      Math.sin(t * 0.0009 + phase) * (isMeadow ? 36 : 30) +
      Math.sin(t * 0.00047 + phase * 2.3) * (isMeadow ? 10 : 18)
    const y = yBase + (isMeadow ? 0 : Math.sin(t * 0.0011 + phase * 1.3) * 12)
    const radius = isMeadow
      ? 0.8 + srand(i + 13) * 1.7
      : 1.3 + srand(i + 13) * 2.2
    const halo = ctx.createRadialGradient(
      x,
      y,
      0,
      x,
      y,
      radius * (isMeadow ? 4.2 : 5.6)
    )
    halo.addColorStop(0, `rgba(${fr}, ${fg}, ${fb}, ${0.2 * pulse})`)
    halo.addColorStop(1, `rgba(${fr}, ${fg}, ${fb}, 0)`)
    ctx.beginPath()
    ctx.arc(x, y, radius * (isMeadow ? 4.2 : 5.6), 0, TAU)
    ctx.fillStyle = halo
    ctx.fill()
    ctx.beginPath()
    if (isMeadow) {
      ctx.ellipse(x, y, radius * 1.45, radius * 0.72, phase, 0, TAU)
      ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${0.34 * pulse})`
    } else {
      ctx.arc(x, y, radius, 0, TAU)
      ctx.fillStyle = `rgba(${fr}, ${fg}, ${fb}, ${0.25 + 0.7 * pulse})`
    }
    ctx.fill()
  }
}

function drawMeadowGrass(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  ctx.save()
  ctx.strokeStyle = `rgba(${Math.round(r * 0.52)}, ${Math.round(g * 0.62)}, ${Math.round(b * 0.36)}, 0.28)`
  for (let i = 0; i < 42; i++) {
    const x = srand(i + 101) * width
    const bladeH = height * (0.035 + srand(i + 103) * 0.06)
    // A traveling sine sends coherent wind waves across the field instead of
    // each blade wiggling independently.
    const bend =
      (srand(i + 107) - 0.5) * 22 +
      Math.sin(t * 0.0009 - x * 0.012) * 7 +
      Math.sin(t * 0.0007 + i) * 2
    ctx.lineWidth = 0.45 + srand(i + 109) * 1.1
    ctx.beginPath()
    ctx.moveTo(x, height + 2)
    ctx.quadraticCurveTo(
      x + bend * 0.35,
      height - bladeH * 0.55,
      x + bend,
      height - bladeH
    )
    ctx.stroke()
  }
  ctx.restore()
}

function drawStars(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  for (let i = 0; i < 90; i++) {
    const x = srand(i) * width
    const y = srand(i + 3) * height * 0.85
    const phase = srand(i + 7) * TAU
    const twinkle = 0.35 + 0.65 * (0.5 + 0.5 * Math.sin(t * 0.002 + phase))
    const depth = srand(i + 15)
    const radius = 0.45 + depth * 1.6
    if (depth > 0.7) {
      const glow = ctx.createRadialGradient(x, y, 0, x, y, radius * 4.2)
      glow.addColorStop(0, `rgba(${r}, ${g}, ${b}, ${0.28 * twinkle})`)
      glow.addColorStop(1, `rgba(${r}, ${g}, ${b}, 0)`)
      ctx.beginPath()
      ctx.arc(x, y, radius * 4.2, 0, TAU)
      ctx.fillStyle = glow
      ctx.fill()
    }
    ctx.beginPath()
    ctx.arc(x, y, radius, 0, TAU)
    ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${twinkle})`
    ctx.fill()
    if (depth > 0.9) {
      const flare = radius * 3
      ctx.strokeStyle = `rgba(${r}, ${g}, ${b}, ${0.45 * twinkle})`
      ctx.lineWidth = 0.55
      ctx.beginPath()
      ctx.moveTo(x - flare, y)
      ctx.lineTo(x + flare, y)
      ctx.moveTo(x, y - flare)
      ctx.lineTo(x, y + flare)
      ctx.stroke()
    }
  }
}

/** Flowing translucent light bands over a starfield. */
function drawAurora(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  drawStars(ctx, width, height, k, t)
  const [r, g, b] = hexToRgb(k.accentColor)
  const bands = 5
  const steps = 7
  ctx.save()
  ctx.globalCompositeOperation = "screen"
  for (let band = 0; band < bands; band++) {
    // Real aurorae shift hue with altitude: green low, teal/violet high.
    const hueF = band / (bands - 1)
    const br = Math.round(r * (1 - hueF * 0.5) + 130 * hueF)
    const bg = Math.round(g * (1 - hueF * 0.35))
    const bb = Math.min(255, Math.round(b * (1 - hueF) + 235 * hueF))
    const baseY = height * (0.14 + band * 0.105)
    const amp = 34 + band * 16
    const bandH = height * (0.12 + band * 0.018)
    const phase = band * 1.7
    // Start the fade well above the wandering top edge so the curtain has no
    // hard contour and reads as glow, not a painted hill.
    const grad = ctx.createLinearGradient(
      0,
      baseY - amp * 1.9,
      0,
      baseY + bandH
    )
    grad.addColorStop(0, `rgba(${br}, ${bg}, ${bb}, 0)`)
    grad.addColorStop(0.55, `rgba(${br}, ${bg}, ${bb}, ${0.05 + band * 0.012})`)
    grad.addColorStop(
      1,
      `rgba(${Math.round(br * 0.45)}, ${bg}, ${Math.min(255, Math.round(bb * 1.2))}, 0)`
    )
    // Feather the curtain: three offset passes at low alpha blur the top edge
    // so it reads as glow, not a hard contour.
    for (let pass = 0; pass < 3; pass++) {
      const dy = (pass - 1) * bandH * 0.2
      ctx.save()
      ctx.globalAlpha = 0.4
      ctx.beginPath()
      let px = 0
      let py = baseY + dy + Math.sin(t * 0.0006 + phase) * amp
      ctx.moveTo(px, py)
      for (let step = 1; step <= steps; step++) {
        const x = (step / steps) * width
        const y = baseY + dy + Math.sin(x * 0.004 + t * 0.0006 + phase) * amp
        ctx.quadraticCurveTo((px + x) / 2, (py + y) / 2 - amp * 0.18, x, y)
        px = x
        py = y
      }
      ctx.lineTo(width, baseY + dy + bandH)
      ctx.lineTo(0, baseY + dy + bandH * 0.86)
      ctx.closePath()
      ctx.fillStyle = grad
      ctx.fill()
      ctx.restore()
    }

    ctx.strokeStyle = `rgba(${br}, ${bg}, ${bb}, ${0.02 + band * 0.008})`
    ctx.lineWidth = Math.max(0.5, width * 0.001)
    for (let fold = 0; fold < 7; fold++) {
      const x = ((fold + 0.35 + srand(fold + band * 13) * 0.3) / 7) * width
      const top = baseY + Math.sin(x * 0.004 + t * 0.0006 + phase) * amp
      ctx.beginPath()
      ctx.moveTo(x, top)
      ctx.quadraticCurveTo(
        x + Math.sin(t * 0.0004 + fold) * 18,
        top + bandH * 0.45,
        x + Math.cos(t * 0.0005 + fold) * 10,
        top + bandH
      )
      ctx.stroke()
    }
  }
  ctx.restore()
}

/** Expanding elliptical ripple rings where rain meets the wet ground. */
function drawRainRipples(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  const [r, g, b] = hexToRgb(k.accentColor)
  for (let i = 0; i < 16; i++) {
    const cycle = 900 + srand(i + 51) * 700
    const p = ((t + srand(i + 53) * cycle * 7) % cycle) / cycle
    const x = srand(i + 55) * width
    const y = height * (0.9 + srand(i + 57) * 0.08)
    const radius = (2 + p * 15) * (0.6 + srand(i + 59) * 0.8)
    ctx.strokeStyle = rgba(
      r * 0.7 + 60,
      g * 0.7 + 64,
      b * 0.7 + 74,
      0.2 * (1 - p)
    )
    ctx.lineWidth = 0.8
    ctx.beginPath()
    ctx.ellipse(x, y, radius, radius * 0.3, 0, 0, TAU)
    ctx.stroke()
  }
}

/** Large soft out-of-focus foreground flakes for depth-of-field. */
function drawSnowBokeh(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  const speed = 0.09 * driftSpeed(k)
  const range = height + 60
  for (let i = 0; i < 6; i++) {
    const y =
      ((srand(i + 81) * range + t * speed * (0.8 + srand(i + 83) * 0.5)) %
        range) -
      30
    const x =
      srand(i + 85) * width + Math.sin(t * 0.0004 + srand(i + 87) * TAU) * 40
    const radius = 7 + srand(i + 89) * 8
    const soft = ctx.createRadialGradient(x, y, 0, x, y, radius)
    soft.addColorStop(0, "rgba(240, 246, 255, 0.16)")
    soft.addColorStop(0.7, "rgba(240, 246, 255, 0.08)")
    soft.addColorStop(1, "rgba(240, 246, 255, 0)")
    ctx.beginPath()
    ctx.arc(x, y, radius, 0, TAU)
    ctx.fillStyle = soft
    ctx.fill()
  }
}

/** Rare deterministic shooting star streaking across the upper sky. */
function drawShootingStar(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  t: number
): void {
  const period = 9000
  const p = (t % period) / period
  if (p >= 0.14) return
  const q = p / 0.14
  const pass = Math.floor(t / period)
  const sx = width * (0.12 + srand(pass + 61) * 0.6) + q * width * 0.2
  const sy = height * (0.06 + srand(pass + 67) * 0.2) + q * height * 0.1
  const len = width * 0.05 * (1 - q * 0.4)
  const fade = Math.sin(Math.min(1, q) * Math.PI)
  ctx.strokeStyle = `rgba(235, 240, 255, ${0.7 * fade})`
  ctx.lineWidth = 1.1
  ctx.beginPath()
  ctx.moveTo(sx, sy)
  ctx.lineTo(sx - len, sy - len * 0.5)
  ctx.stroke()
}

function drawNatureScene(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  t: number
): void {
  const layer = getNatureEnvironmentLayer(k, width, height)
  if (layer) {
    ctx.drawImage(layer, 0, 0)
  } else {
    paintNatureEnvironment(ctx, width, height, k)
  }
  switch (k.backgroundKind) {
    case "foliage":
      drawFallingLeaves(ctx, width, height, k, t, {
        count: 30,
        size: 15,
        petal: false,
      })
      break
    case "forest":
      drawFallingLeaves(ctx, width, height, k, t, {
        count: 22,
        size: 20,
        petal: false,
      })
      break
    case "autumn":
      drawFallingLeaves(ctx, width, height, k, t, {
        count: 40,
        size: 16,
        petal: false,
      })
      break
    case "blossom":
      drawFallingLeaves(ctx, width, height, k, t, {
        count: 40,
        size: 12,
        petal: true,
      })
      break
    case "rain":
      drawRain(ctx, width, height, k, t)
      drawRainRipples(ctx, width, height, k, t)
      break
    case "snow":
      drawSnow(ctx, width, height, k, t)
      drawSnowBokeh(ctx, width, height, k, t)
      break
    case "fireflies":
      drawGlowMotes(ctx, width, height, k, t, 36)
      break
    case "meadow":
      drawMeadowGrass(ctx, width, height, k, t)
      drawGlowMotes(ctx, width, height, k, t, 40)
      break
    case "stars":
      drawStars(ctx, width, height, k, t)
      drawShootingStar(ctx, width, height, t)
      break
    case "aurora":
      drawAurora(ctx, width, height, k, t)
      break
    default:
      break
  }
}

// ---------------------------------------------------------------------------
// Desert Cloth worship scene — 1:1 canvas port of worship_background HTML
// (sand cloth folds + sheen + weave + vignette + glowing cross + portrait).
// All literal values are transcribed from the HTML's CSS; see the plan's
// DESIGN FIDELITY SPEC before changing any of them.
// ---------------------------------------------------------------------------

const CLOTH = {
  base: ["#cbab7f", "#b8956a", "#8a6a45"],
  sandGlow: "#f3e3c2",
  sandLight: "#e3cba4",
  sandDeep: "#8a6a45",
  darkMid: "#6f5334",
} as const

const CLOTH_DESIGN_WIDTH = 1920
const CLOTH_DESIGN_HEIGHT = 1080

function clothScale(width: number, height: number): number {
  return Math.min(width / CLOTH_DESIGN_WIDTH, height / CLOTH_DESIGN_HEIGHT)
}

function clothPx(scale: number, value: number): number {
  return value * scale
}

/** [stopPosition, translateY%, rotateDeg, scaleY, skewXDeg] */
type GustFrame = [number, number, number, number, number]
type CubicBezier = [number, number, number, number]

const GUSTS: GustFrame[][] = [
  [
    [0, 1, -11, 0.85, 0],
    [0.22, -4, -6, 1.35, -7],
    [0.38, -1, -9, 0.95, 3],
    [0.55, -5, -5, 1.28, -9],
    [0.72, 2, -10, 0.9, 2],
    [1, 1, -11, 0.85, 0],
  ],
  [
    [0, -2, -6, 1.2, -6],
    [0.18, 3, -11, 0.8, 5],
    [0.4, -5, -5, 1.4, -10],
    [0.58, 0, -8, 1, 0],
    [0.78, -3, -5, 1.25, -8],
    [1, -2, -6, 1.2, -6],
  ],
  [
    [0, 2, -12, 0.82, 4],
    [0.25, -6, -5, 1.38, -11],
    [0.45, 1, -9, 0.92, 2],
    [0.65, -4, -4, 1.3, -8],
    [0.85, 2, -10, 0.88, 3],
    [1, 2, -12, 0.82, 4],
  ],
]

interface ClothFold {
  top: number // fraction of frame height
  light: boolean
  durationMs: number
  delayMs: number
  gust: 0 | 1 | 2
  easing: CubicBezier
}

const CLOTH_FOLDS: ClothFold[] = [
  {
    top: 0.02,
    light: true,
    durationMs: 6500,
    delayMs: 0,
    gust: 0,
    easing: [0.45, 0.05, 0.35, 1],
  },
  {
    top: 0.2,
    light: false,
    durationMs: 7800,
    delayMs: -2200,
    gust: 1,
    easing: [0.5, 0.1, 0.3, 1],
  },
  {
    top: 0.38,
    light: true,
    durationMs: 5900,
    delayMs: -1100,
    gust: 2,
    easing: [0.45, 0.05, 0.35, 1],
  },
  {
    top: 0.55,
    light: false,
    durationMs: 8400,
    delayMs: -3600,
    gust: 0,
    easing: [0.5, 0.1, 0.3, 1],
  },
  {
    top: 0.72,
    light: true,
    durationMs: 6200,
    delayMs: -4400,
    gust: 1,
    easing: [0.45, 0.05, 0.35, 1],
  },
  {
    top: 0.88,
    light: false,
    durationMs: 7100,
    delayMs: -800,
    gust: 2,
    easing: [0.5, 0.1, 0.3, 1],
  },
]

/** y of a CSS cubic-bezier timing function at progress t (Newton refinement). */
function cubicBezierEase(
  p1x: number,
  p1y: number,
  p2x: number,
  p2y: number,
  t: number
): number {
  if (t <= 0) return 0
  if (t >= 1) return 1
  const cx = 3 * p1x
  const bx = 3 * (p2x - p1x) - cx
  const ax = 1 - cx - bx
  const cy = 3 * p1y
  const by = 3 * (p2y - p1y) - cy
  const ay = 1 - cy - by
  let u = t
  for (let i = 0; i < 8; i++) {
    const x = ((ax * u + bx) * u + cx) * u - t
    const d = (3 * ax * u + 2 * bx) * u + cx
    if (Math.abs(x) < 1e-5 || d === 0) break
    u -= x / d
  }
  if (u < 0) u = 0
  if (u > 1) u = 1
  return ((ay * u + by) * u + cy) * u
}

function gustTransform(
  gust: 0 | 1 | 2,
  easing: CubicBezier,
  phase: number
): { ty: number; rot: number; sy: number; skx: number } {
  const frames = GUSTS[gust]
  const [x1, y1, x2, y2] = easing
  let a = frames[0]
  let b = frames[frames.length - 1]
  for (let i = 0; i < frames.length - 1; i++) {
    if (phase >= frames[i][0] && phase <= frames[i + 1][0]) {
      a = frames[i]
      b = frames[i + 1]
      break
    }
  }
  const span = Math.max(1e-6, b[0] - a[0])
  const t = cubicBezierEase(x1, y1, x2, y2, (phase - a[0]) / span)
  return {
    ty: a[1] + (b[1] - a[1]) * t,
    rot: a[2] + (b[2] - a[2]) * t,
    sy: a[3] + (b[3] - a[3]) * t,
    skx: a[4] + (b[4] - a[4]) * t,
  }
}

function drawClothFolds(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  t: number,
  scale: number
): void {
  const bandW = width * 1.6
  const bandH = height * 0.34
  for (const fold of CLOTH_FOLDS) {
    const phase = kineticLoopPhase(t - fold.delayMs, fold.durationMs)
    const { ty, rot, sy, skx } = gustTransform(fold.gust, fold.easing, phase)
    const cx = -0.3 * width + bandW / 2
    const cy = fold.top * height + bandH / 2 + (ty / 100) * bandH
    ctx.save()
    ctx.globalAlpha = fold.light ? 0.6 : 0.42
    ctx.filter = `blur(${clothPx(scale, 34)}px)`
    ctx.translate(cx, cy)
    ctx.rotate((rot * Math.PI) / 180)
    ctx.transform(1, 0, Math.tan((skx * Math.PI) / 180), 1, 0, 0)
    ctx.scale(1, sy)
    const grad = ctx.createLinearGradient(-bandW / 2, 0, bandW / 2, 0)
    if (fold.light) {
      grad.addColorStop(0, "rgba(243,227,194,0)")
      grad.addColorStop(0.35, CLOTH.sandGlow)
      grad.addColorStop(0.55, CLOTH.sandLight)
      grad.addColorStop(1, "rgba(227,203,164,0)")
    } else {
      grad.addColorStop(0, "rgba(138,106,69,0)")
      grad.addColorStop(0.4, CLOTH.sandDeep)
      grad.addColorStop(0.6, CLOTH.darkMid)
      grad.addColorStop(1, "rgba(111,83,52,0)")
    }
    ctx.fillStyle = grad
    ctx.beginPath()
    ctx.ellipse(0, 0, bandW / 2, bandH / 2, 0, 0, TAU)
    ctx.fill()
    ctx.restore()
  }
}

function drawClothSheen(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number
): void {
  ctx.save()
  ctx.globalCompositeOperation = "soft-light"
  const angle = (115 * Math.PI) / 180
  const cx = width / 2
  const cy = height / 2
  const len = Math.sqrt(width * width + height * height) / 2
  const grad = ctx.createLinearGradient(
    cx - Math.cos(angle) * len,
    cy - Math.sin(angle) * len,
    cx + Math.cos(angle) * len,
    cy + Math.sin(angle) * len
  )
  grad.addColorStop(0.3, "rgba(255,244,220,0)")
  grad.addColorStop(0.5, "rgba(255,244,220,0.18)")
  grad.addColorStop(0.7, "rgba(255,244,220,0)")
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)
  ctx.restore()
}

let weaveCanvas: HTMLCanvasElement | null = null
let weaveSizeKey = ""

function drawClothWeave(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  scale: number
): void {
  try {
    const key = `${width}x${height}`
    if (!weaveCanvas || weaveSizeKey !== key) {
      const c = document.createElement("canvas")
      c.width = width
      c.height = height
      const wctx = c.getContext("2d")
      if (!wctx) return
      const spacing = Math.max(1, clothPx(scale, 3))
      const line = Math.max(0.5, clothPx(scale, 1))
      wctx.fillStyle = "rgba(61,43,23,0.7)"
      for (let y = 0; y < height; y += spacing) wctx.fillRect(0, y, width, line)
      for (let x = 0; x < width; x += spacing) wctx.fillRect(x, 0, line, height)
      weaveCanvas = c
      weaveSizeKey = key
    }
    ctx.save()
    ctx.globalAlpha = 0.05
    ctx.drawImage(weaveCanvas, 0, 0)
    ctx.restore()
  } catch {
    // Non-DOM environment (tests): the weave is a subtle texture, safe to skip.
  }
}

function drawClothVignette(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number
): void {
  const grad = ctx.createRadialGradient(
    width * 0.42,
    height * 0.46,
    0,
    width * 0.42,
    height * 0.46,
    Math.max(width, height) * 0.72
  )
  grad.addColorStop(0.55, "rgba(61,43,23,0)")
  grad.addColorStop(1, "rgba(61,43,23,0.28)")
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)
}

function drawClothCross(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  t: number,
  scale: number
): void {
  void width
  const phase = kineticLoopPhase(t, 6000)
  const wave = 0.5 - 0.5 * Math.cos(phase * TAU) // 0→1→0 ease-in-out loop
  const w = clothPx(scale, 96)
  const h = clothPx(scale, 140)
  const bar = clothPx(scale, 18)
  const x = clothPx(scale, 52)
  const y = height - clothPx(scale, 44) - h - clothPx(scale, 4) * wave
  const cx = x + w / 2
  const cy = y + h / 2

  // Halo: pulses opacity .55→.9 and scale 1→1.1 on the same 6s loop.
  const haloR = (Math.max(w, h) / 2 + clothPx(scale, 36)) * (1 + 0.1 * wave)
  ctx.save()
  ctx.globalAlpha = 0.55 + 0.35 * wave
  const halo = ctx.createRadialGradient(cx, cy, 0, cx, cy, haloR)
  halo.addColorStop(0, "rgba(255,240,205,0.5)")
  halo.addColorStop(0.68, "rgba(255,240,205,0)")
  ctx.fillStyle = halo
  ctx.fillRect(cx - haloR, cy - haloR, haloR * 2, haloR * 2)
  ctx.restore()

  // Bars: 160deg gold gradient, outer shadow, soft inner highlight stroke.
  ctx.save()
  ctx.shadowColor = "rgba(61,43,23,0.4)"
  ctx.shadowBlur = clothPx(scale, 14)
  ctx.shadowOffsetY = clothPx(scale, 3)
  const angle = (160 * Math.PI) / 180
  const len = Math.sqrt(w * w + h * h) / 2
  const grad = ctx.createLinearGradient(
    cx - Math.sin(angle) * len,
    cy + Math.cos(angle) * len,
    cx + Math.sin(angle) * len,
    cy - Math.cos(angle) * len
  )
  grad.addColorStop(0, "#f3e3c2")
  grad.addColorStop(0.7, "#caa76e")
  grad.addColorStop(1, "#a8834e")
  ctx.fillStyle = grad
  const bars: [number, number, number, number][] = [
    [cx - bar / 2, y, bar, h], // vertical
    [x, y + clothPx(scale, 32), w, bar], // horizontal, top at 32px
  ]
  for (const [bx, by, bw, bh] of bars) {
    ctx.beginPath()
    if (typeof ctx.roundRect === "function")
      ctx.roundRect(bx, by, bw, bh, clothPx(scale, 3))
    else ctx.rect(bx, by, bw, bh)
    ctx.fill()
  }
  ctx.restore()

  ctx.save()
  ctx.strokeStyle = "rgba(255,248,230,0.55)"
  ctx.lineWidth = Math.max(0.5, clothPx(scale, 2))
  for (const [bx, by, bw, bh] of bars) {
    const inset = Math.max(0.5, clothPx(scale, 1))
    ctx.strokeRect(bx + inset, by + inset, bw - inset * 2, bh - inset * 2)
  }
  ctx.restore()
}

// Portrait asset: decoded once per session. Until it loads, the scene simply
// draws without it; static thumbnail hosts subscribe below for one redraw.
let portraitImage: HTMLImageElement | null = null
let portraitLoaded = false
let portraitRequested = false
let portraitMasked: HTMLCanvasElement | null = null
const portraitLoadListeners = new Set<() => void>()

export function onClothPortraitLoaded(callback: () => void): () => void {
  if (portraitLoaded) {
    callback()
    return () => {}
  }
  portraitLoadListeners.add(callback)
  return () => {
    portraitLoadListeners.delete(callback)
  }
}

function requestPortrait(): HTMLImageElement | null {
  if (!portraitRequested) {
    portraitRequested = true
    try {
      const img = new Image()
      img.onload = () => {
        portraitLoaded = true
        for (const listener of portraitLoadListeners) listener()
      }
      img.src = worshipPortraitUrl
      portraitImage = img
    } catch {
      // Non-DOM environment (tests): scene renders without the portrait.
    }
  }
  return portraitLoaded && portraitImage ? portraitImage : null
}

/** Left-edge alpha fade (transparent → .85 @22% → 1 @40%), cached once. */
function maskedPortrait(
  img: HTMLImageElement
): HTMLCanvasElement | HTMLImageElement {
  if (portraitMasked) return portraitMasked
  try {
    const c = document.createElement("canvas")
    c.width = img.naturalWidth
    c.height = img.naturalHeight
    const mctx = c.getContext("2d")
    if (!mctx) return img
    mctx.drawImage(img, 0, 0)
    const grad = mctx.createLinearGradient(0, 0, c.width, 0)
    grad.addColorStop(0, "rgba(0,0,0,0)")
    grad.addColorStop(0.22, "rgba(0,0,0,0.85)")
    grad.addColorStop(0.4, "rgba(0,0,0,1)")
    grad.addColorStop(1, "rgba(0,0,0,1)")
    mctx.globalCompositeOperation = "destination-in"
    mctx.fillStyle = grad
    mctx.fillRect(0, 0, c.width, c.height)
    portraitMasked = c
    return c
  } catch {
    return img
  }
}

function drawClothPortrait(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  scale: number
): void {
  const img = requestPortrait()
  if (!img || img.naturalHeight === 0) return
  const source = maskedPortrait(img)
  const drawH = height
  const drawW = height * (img.naturalWidth / img.naturalHeight)
  const drawX = width - drawW
  ctx.save()
  ctx.shadowColor = "rgba(61,43,23,0.35)"
  ctx.shadowBlur = clothPx(scale, 34)
  ctx.shadowOffsetX = -clothPx(scale, 14)
  ctx.drawImage(source, drawX, 0, drawW, drawH)
  ctx.restore()
}

function drawClothScene(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  t: number
): void {
  const scale = clothScale(width, height)
  // Base: linear-gradient(135deg, sand-mid 0%, sand 45%, sand-deep 100%).
  const angle = (135 * Math.PI) / 180
  const cx = width / 2
  const cy = height / 2
  const len = Math.sqrt(width * width + height * height) / 2
  const grad = ctx.createLinearGradient(
    cx - Math.cos(angle) * len,
    cy - Math.sin(angle) * len,
    cx + Math.cos(angle) * len,
    cy + Math.sin(angle) * len
  )
  grad.addColorStop(0, CLOTH.base[0])
  grad.addColorStop(0.45, CLOTH.base[1])
  grad.addColorStop(1, CLOTH.base[2])
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)

  drawClothFolds(ctx, width, height, t, scale)
  drawClothSheen(ctx, width, height)
  drawClothWeave(ctx, width, height, scale)
  drawClothVignette(ctx, width, height)
  drawClothPortrait(ctx, width, height, scale)
  drawClothCross(ctx, width, height, t, scale)
}

// KNFC verse stage: gradient, bottom glow, rotating conic shimmer, and a faint
// star field masked toward the vertical edges.
function hexWithAlpha(hex: string, alpha: number): string {
  let value = hex.replace("#", "")
  if (value.length === 3) {
    value = value
      .split("")
      .map((c) => c + c)
      .join("")
  }
  const n = Number.parseInt(value, 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`
}

function drawStageDots(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  spacing: number,
  offsetX: number,
  offsetY: number,
  radius: number,
  alpha: number
): void {
  for (let y = offsetY; y < height; y += spacing) {
    const fade = Math.sin((y / height) * Math.PI)
    if (fade <= 0.01) continue
    ctx.fillStyle = `rgba(255, 255, 255, ${(alpha * fade).toFixed(4)})`
    for (let x = offsetX; x < width; x += spacing) {
      ctx.beginPath()
      ctx.arc(x, y, radius, 0, TAU)
      ctx.fill()
    }
  }
}

function drawStageScene(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  k: BroadcastKineticTheme,
  timeMs: number
): void {
  const colors = colorsOf(k)
  const angle = (135 * Math.PI) / 180
  const cx = width / 2
  const cy = height / 2
  const len = Math.sqrt(width * width + height * height) / 2
  const grad = ctx.createLinearGradient(
    cx - Math.cos(angle) * len,
    cy - Math.sin(angle) * len,
    cx + Math.cos(angle) * len,
    cy + Math.sin(angle) * len
  )
  grad.addColorStop(0, colors[0])
  grad.addColorStop(1, colors[1] ?? colors[0])
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, width, height)

  const glow = colors[2] ?? k.accentColor
  const gx = width * 0.52
  const gy = height * 1.3
  const gr = Math.max(width, height) * 1.04
  const glowGrad = ctx.createRadialGradient(gx, gy, 0, gx, gy, gr)
  glowGrad.addColorStop(0, hexWithAlpha(glow, 0.36))
  glowGrad.addColorStop(1, hexWithAlpha(glow, 0))
  ctx.fillStyle = glowGrad
  ctx.fillRect(0, 0, width, height)

  const phase = kineticLoopPhase(timeMs, k.motion.durationMs)
  if (typeof ctx.createConicGradient === "function") {
    const conic = ctx.createConicGradient(phase * TAU, cx, cy)
    conic.addColorStop(0, "rgba(255, 255, 255, 0)")
    conic.addColorStop(0.14, "rgba(255, 255, 255, 0.075)")
    conic.addColorStop(0.28, "rgba(255, 255, 255, 0)")
    conic.addColorStop(0.68, "rgba(255, 255, 255, 0)")
    conic.addColorStop(0.8, "rgba(255, 255, 255, 0.05)")
    conic.addColorStop(1, "rgba(255, 255, 255, 0)")
    ctx.fillStyle = conic
    ctx.fillRect(0, 0, width, height)
  }

  const scale = Math.min(width, height) / 540
  drawStageDots(
    ctx,
    width,
    height,
    40 * scale,
    8 * scale,
    8 * scale,
    1.2 * scale,
    0.05
  )
  drawStageDots(
    ctx,
    width,
    height,
    56 * scale,
    39 * scale,
    22 * scale,
    1.2 * scale,
    0.038
  )
}

/**
 * Draws the kinetic moving background for `theme` at `timeMs`. Returns `true`
 * when it handled the background (caller should skip the static background) and
 * `false` for non-kinetic themes or when drawing failed (caller falls back to
 * the theme's static `background`).
 */
export function drawKineticBackground(
  ctx: CanvasRenderingContext2D,
  theme: BroadcastTheme,
  timeMs = 0,
  sectionKind?: HymnPresentationSectionKind
): boolean {
  const k = theme.kinetic
  if (!k) return false

  try {
    const { width, height } = theme.resolution
    const sceneTime = k.animate === false ? 0 : timeMs

    if (isHymnThemeScene(k)) {
      return drawHymnThemeScene(ctx, width, height, k, sceneTime, sectionKind)
    }

    if (k.backgroundKind === "cloth") {
      drawClothScene(ctx, width, height, sceneTime)
      return true
    }

    if (k.backgroundKind === "stage") {
      drawStageScene(ctx, width, height, k, sceneTime)
      return true
    }

    if (NATURE_KINDS.has(k.backgroundKind)) {
      drawNatureScene(ctx, width, height, k, sceneTime)
      return true
    }

    const phase = kineticLoopPhase(sceneTime, k.motion.durationMs)

    drawMeshBase(ctx, width, height, k, phase)
    drawDriftBlobs(ctx, width, height, k, phase)

    if (k.backgroundKind === "grid" || k.pattern === "dot-grid") {
      drawDotGrid(ctx, width, height, k, phase)
    }
    if (k.backgroundKind === "stripes" || k.pattern === "diagonal-stripes") {
      drawDiagonalStripes(ctx, width, height, k, phase)
    }
    return true
  } catch (e) {
    console.error("[kinetic-theme-renderer] draw error:", e)
    return false
  }
}
