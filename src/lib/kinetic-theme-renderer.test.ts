import { describe, expect, it, vi } from "vitest"
import {
  drawKineticBackground,
  getNatureEnvironmentLayer,
  isKineticTheme,
  kineticLoopPhase,
} from "./kinetic-theme-renderer"
import {
  buildKineticBroadcastTheme,
  KINETIC_THEME_PRESETS,
} from "./kinetic-themes"
import { BUILTIN_THEMES } from "./builtin-themes"
import type { BroadcastTheme } from "@/types"

function preset(id: string) {
  const p = KINETIC_THEME_PRESETS.find((x) => x.presetId === id)
  if (!p) throw new Error(`missing preset ${id}`)
  return buildKineticBroadcastTheme(p)
}

interface Recorder {
  ctx: CanvasRenderingContext2D
  radial: unknown[][]
  linear: unknown[][]
  arcArgs: unknown[][]
  transforms: unknown[][]
  filters: string[]
  styles: string[]
  arcs: number
  paths: number
  fillRects: number
}

function createRecorder(): Recorder {
  const radial: unknown[][] = []
  const linear: unknown[][] = []
  const arcArgs: unknown[][] = []
  const transforms: unknown[][] = []
  const filters: string[] = []
  const styles: string[] = []
  let currentFilter = "none"
  let currentFillStyle = ""
  const rec = { arcs: 0, paths: 0, fillRects: 0 }
  const gradient = {
    addColorStop: vi.fn((offset: number, color: string) => {
      styles.push(`${offset}:${color}`)
    }),
  }
  const ctx = {
    save: vi.fn(),
    restore: vi.fn(),
    fillRect: vi.fn(() => {
      rec.fillRects += 1
    }),
    clearRect: vi.fn(),
    beginPath: vi.fn(() => {
      rec.paths += 1
    }),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    quadraticCurveTo: vi.fn(),
    arc: vi.fn((...args: unknown[]) => {
      rec.arcs += 1
      arcArgs.push(args)
    }),
    ellipse: vi.fn((...args: unknown[]) => {
      rec.arcs += 1
      arcArgs.push(args)
    }),
    rect: vi.fn(),
    strokeRect: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    translate: vi.fn((...args: unknown[]) => {
      transforms.push(["translate", ...args])
    }),
    rotate: vi.fn((...args: unknown[]) => {
      transforms.push(["rotate", ...args])
    }),
    scale: vi.fn((...args: unknown[]) => {
      transforms.push(["scale", ...args])
    }),
    transform: vi.fn((...args: unknown[]) => {
      transforms.push(["transform", ...args])
    }),
    drawImage: vi.fn(),
    createLinearGradient: vi.fn((...args: unknown[]) => {
      linear.push(args)
      return gradient
    }),
    createRadialGradient: vi.fn((...args: unknown[]) => {
      radial.push(args)
      return gradient
    }),
    get fillStyle() {
      return currentFillStyle
    },
    set fillStyle(value: string | CanvasGradient | CanvasPattern) {
      currentFillStyle = String(value)
      styles.push(currentFillStyle)
    },
    strokeStyle: "",
    globalAlpha: 1,
    globalCompositeOperation: "source-over",
    get filter() {
      return currentFilter
    },
    set filter(value: string) {
      currentFilter = value
      filters.push(value)
    },
    lineWidth: 1,
    shadowColor: "",
    shadowBlur: 0,
    shadowOffsetX: 0,
    shadowOffsetY: 0,
  } as unknown as CanvasRenderingContext2D
  return {
    ctx,
    radial,
    linear,
    arcArgs,
    transforms,
    filters,
    styles,
    get arcs() {
      return rec.arcs
    },
    get paths() {
      return rec.paths
    },
    get fillRects() {
      return rec.fillRects
    },
  } as Recorder
}

describe("isKineticTheme", () => {
  it("is false for static built-ins and true for kinetic presets", () => {
    expect(isKineticTheme(BUILTIN_THEMES[0])).toBe(false)
    expect(isKineticTheme(preset("ocean"))).toBe(true)
    expect(isKineticTheme(preset("hymn-minimal-static"))).toBe(false)
  })
})

describe("kineticLoopPhase", () => {
  it("wraps into 0..1 and is stable for timeMs=0", () => {
    expect(kineticLoopPhase(0, 6000)).toBe(0)
    expect(kineticLoopPhase(6000, 6000)).toBe(0)
    expect(kineticLoopPhase(3000, 6000)).toBeCloseTo(0.5)
    expect(kineticLoopPhase(-1, 6000)).toBeGreaterThanOrEqual(0)
    expect(kineticLoopPhase(1000, 0)).toBe(0)
  })
})

describe("drawKineticBackground", () => {
  it("returns false and draws nothing for non-kinetic themes", () => {
    const r = createRecorder()
    const drew = drawKineticBackground(
      r.ctx,
      BUILTIN_THEMES[0] as BroadcastTheme,
      0
    )
    expect(drew).toBe(false)
    expect(r.fillRects).toBe(0)
  })

  it("draws a deterministic frame: same timeMs => identical gradient calls", () => {
    const a = createRecorder()
    const b = createRecorder()
    drawKineticBackground(a.ctx, preset("ocean"), 0)
    drawKineticBackground(b.ctx, preset("ocean"), 0)
    expect(a.radial).toEqual(b.radial)
    expect(a.linear).toEqual(b.linear)
  })

  it("changes the frame when timeMs advances", () => {
    const a = createRecorder()
    const b = createRecorder()
    drawKineticBackground(a.ctx, preset("ocean"), 0)
    drawKineticBackground(b.ctx, preset("ocean"), 1500)
    expect(a.radial).not.toEqual(b.radial)
  })

  it("draws dot-grid arcs for cyberpunk", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("cyberpunk"), 0)
    expect(r.arcs).toBeGreaterThan(0)
  })

  it("draws diagonal stripe paths for brutalist", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("brutalist"), 0)
    expect(r.paths).toBeGreaterThan(0)
  })

  it("does not draw dot-grid for a plain mesh theme", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("ocean"), 0)
    expect(r.arcs).toBe(0)
  })

  it("draws the KNFC verse stage and reports handled", () => {
    const r = createRecorder()
    const drew = drawKineticBackground(r.ctx, preset("stage-navy"), 0)
    expect(drew).toBe(true)
    expect(r.linear.length).toBeGreaterThan(0)
    expect(r.radial.length).toBeGreaterThan(0)
    expect(r.arcs).toBeGreaterThan(50)
  })

  it("verse stage is deterministic and works without createConicGradient", () => {
    const a = createRecorder()
    const b = createRecorder()
    expect(drawKineticBackground(a.ctx, preset("stage-teal"), 4000)).toBe(true)
    expect(drawKineticBackground(b.ctx, preset("stage-teal"), 4000)).toBe(true)
    expect(a.radial).toEqual(b.radial)
    expect(a.linear).toEqual(b.linear)
    expect(a.arcArgs).toEqual(b.arcArgs)
  })

  it("draws the desert cloth scene and reports handled", () => {
    const r = createRecorder()
    const drew = drawKineticBackground(r.ctx, preset("desert-cloth"), 0)
    expect(drew).toBe(true)
    // Base gradient + fold fills + vignette at minimum.
    expect(r.fillRects).toBeGreaterThan(0)
    expect(r.arcs).toBeGreaterThanOrEqual(6) // six fold ellipses
  })

  it("desert cloth is deterministic at a fixed timeMs", () => {
    const a = createRecorder()
    const b = createRecorder()
    drawKineticBackground(a.ctx, preset("desert-cloth"), 0)
    drawKineticBackground(b.ctx, preset("desert-cloth"), 0)
    expect(a.transforms).toEqual(b.transforms)
    expect(a.linear).toEqual(b.linear)
    expect(a.radial).toEqual(b.radial)
  })

  it("desert cloth animates: fold transforms differ across timeMs", () => {
    const a = createRecorder()
    const b = createRecorder()
    drawKineticBackground(a.ctx, preset("desert-cloth"), 0)
    drawKineticBackground(b.ctx, preset("desert-cloth"), 1500)
    expect(a.transforms).not.toEqual(b.transforms)
  })

  it("desert cloth scales CSS pixel effects for thumbnails", () => {
    const r = createRecorder()
    const theme = {
      ...preset("desert-cloth"),
      resolution: { width: 192, height: 108 },
    }
    drawKineticBackground(r.ctx, theme, 0)
    expect(r.filters.some((f) => f.includes("blur(3.4"))).toBe(true)
    expect(r.filters).not.toContain("blur(34px)")
  })

  it("notifies static hosts when the desert cloth portrait loads", async () => {
    vi.resetModules()
    const created: Array<{ onload: (() => void) | null }> = []
    class MockImage {
      onload: (() => void) | null = null
      naturalWidth = 500
      naturalHeight = 900
      constructor() {
        created.push(this)
      }
      set src(_value: string) {}
    }
    vi.stubGlobal("Image", MockImage)

    const renderer = await import("./kinetic-theme-renderer")
    const themes = await import("./kinetic-themes")
    const p = themes.KINETIC_THEME_PRESETS.find(
      (x) => x.presetId === "desert-cloth"
    )
    if (!p) throw new Error("missing desert-cloth preset")
    const onLoad = vi.fn()
    renderer.onClothPortraitLoaded(onLoad)
    renderer.drawKineticBackground(
      createRecorder().ctx,
      themes.buildKineticBroadcastTheme(p),
      0
    )
    created[0]?.onload?.()

    expect(onLoad).toHaveBeenCalledTimes(1)
    vi.unstubAllGlobals()
  })
})

describe("full-fidelity hymn scenes", () => {
  const kineticIds = [
    "hymn-midnight",
    "hymn-dawn",
    "hymn-minimal",
    "hymn-glass",
    "hymn-water",
    "hymn-heritage",
    "hymn-upper-room",
  ]

  function signature(recorder: Recorder): string {
    return JSON.stringify({
      radial: recorder.radial,
      linear: recorder.linear,
      arcs: recorder.arcArgs,
      transforms: recorder.transforms,
      styles: recorder.styles,
    })
  }

  it("renders all seven designs deterministically at a fixed time", () => {
    for (const id of kineticIds) {
      const a = createRecorder()
      const b = createRecorder()
      expect(drawKineticBackground(a.ctx, preset(id), 3750, "verse")).toBe(true)
      expect(drawKineticBackground(b.ctx, preset(id), 3750, "verse")).toBe(true)
      expect(signature(a)).toBe(signature(b))
      expect(a.fillRects + a.paths + a.arcs).toBeGreaterThan(0)
    }
  })

  it("animates every kinetic hymn design", () => {
    for (const id of kineticIds) {
      const a = createRecorder()
      const b = createRecorder()
      drawKineticBackground(a.ctx, preset(id), 0, "verse")
      drawKineticBackground(b.ctx, preset(id), 3500, "verse")
      expect(signature(a), id).not.toBe(signature(b))
    }
  })

  it("renders authored refrain pages differently from verse pages", () => {
    for (const id of kineticIds) {
      const verse = createRecorder()
      const refrain = createRecorder()
      drawKineticBackground(verse.ctx, preset(id), 0, "verse")
      drawKineticBackground(refrain.ctx, preset(id), 0, "refrain")
      expect(
        (verse.ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls.length
      ).toBeGreaterThan(0)
      expect(
        (refrain.ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls.length
      ).toBeGreaterThan(0)
      expect(verse.styles, id).not.toEqual(refrain.styles)
    }
  })

  it("freezes the two static candidates at the deterministic time-zero frame", () => {
    for (const id of ["hymn-minimal-static", "hymn-heritage-static"]) {
      const a = createRecorder()
      const b = createRecorder()
      drawKineticBackground(a.ctx, preset(id), 0, "verse")
      drawKineticBackground(b.ctx, preset(id), 9000, "verse")
      expect(signature(a)).toBe(signature(b))
      expect(isKineticTheme(preset(id))).toBe(false)
    }
  })
})

describe("nature scenes", () => {
  it("draws stroked streaks for rain", () => {
    const r = createRecorder()
    const drew = drawKineticBackground(r.ctx, preset("nature-rain"), 0)
    expect(drew).toBe(true)
    expect(r.paths).toBeGreaterThan(0)
    expect(r.linear.length).toBeGreaterThan(1)
  })

  it("draws flake arcs for snow", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-snow"), 0)
    expect(r.arcs).toBeGreaterThan(0)
    expect(r.radial.length).toBeGreaterThan(0)
    expect(
      (r.ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws glow arcs for fireflies", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-fireflies"), 0)
    expect(r.arcs).toBeGreaterThan(0)
    expect(r.radial.length).toBeGreaterThan(0)
  })

  it("draws meadow grass and drifting pollen", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-meadow"), 0)
    expect(r.radial.length).toBeGreaterThan(0)
    expect(
      (r.ctx.quadraticCurveTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
    expect(
      (r.ctx.ellipse as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws star halos and glints", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-stars"), 0)
    expect(r.radial.length).toBeGreaterThan(0)
    expect(
      (r.ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws curved aurora curtains", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-aurora"), 0)
    expect(r.linear.length).toBeGreaterThan(1)
    expect(
      (r.ctx.quadraticCurveTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws curved leaf blades for foliage", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-foliage"), 0)
    expect(r.paths).toBeGreaterThan(0)
    expect(r.linear.length).toBeGreaterThan(1)
    expect(
      (r.ctx.quadraticCurveTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
    expect(
      (r.ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws ground ripples for rain", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-rain"), 2000)
    expect(
      (r.ctx.ellipse as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws a horizon silhouette behind the stars", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-stars"), 0)
    expect(
      (r.ctx.quadraticCurveTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws hill silhouettes behind the snow", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-snow"), 0)
    expect(
      (r.ctx.quadraticCurveTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("draws a dusk treeline behind the fireflies", () => {
    const r = createRecorder()
    drawKineticBackground(r.ctx, preset("nature-fireflies"), 0)
    expect(
      (r.ctx.quadraticCurveTo as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBeGreaterThan(0)
  })

  it("renders every nature kind without throwing at zero and nonzero times", () => {
    const ids = [
      "nature-foliage",
      "nature-forest",
      "nature-rain",
      "nature-autumn",
      "nature-blossom",
      "nature-snow",
      "nature-fireflies",
      "nature-stars",
      "nature-meadow",
      "nature-aurora",
    ]
    for (const id of ids) {
      expect(drawKineticBackground(createRecorder().ctx, preset(id), 0)).toBe(
        true
      )
      expect(
        drawKineticBackground(createRecorder().ctx, preset(id), 40000)
      ).toBe(true)
    }
  })

  it("is deterministic at a fixed timeMs and animates as time advances", () => {
    const a = createRecorder()
    const b = createRecorder()
    const c = createRecorder()
    drawKineticBackground(a.ctx, preset("nature-snow"), 0)
    drawKineticBackground(b.ctx, preset("nature-snow"), 0)
    drawKineticBackground(c.ctx, preset("nature-snow"), 1500)
    expect(a.arcArgs).toEqual(b.arcArgs)
    expect(a.arcArgs).not.toEqual(c.arcArgs)
  })
})

describe("nature environment layer cache", () => {
  function stubCanvasDocument() {
    const createElement = vi.fn((tag: string) => {
      if (tag !== "canvas") throw new Error(`unexpected tag ${tag}`)
      return {
        width: 0,
        height: 0,
        getContext: () => createRecorder().ctx,
      }
    })
    vi.stubGlobal("document", { createElement })
    return createElement
  }

  it("returns the same canvas for identical args and repaints on palette change", () => {
    stubCanvasDocument()
    try {
      const theme = preset("nature-forest")
      if (!isKineticTheme(theme)) throw new Error("expected kinetic theme")
      const kinetic = theme.kinetic
      const a = getNatureEnvironmentLayer(kinetic, 640, 360)
      const b = getNatureEnvironmentLayer(kinetic, 640, 360)
      expect(a).not.toBeNull()
      expect(b).toBe(a)
      const recolored = { ...kinetic, accentColor: "#ff0000" }
      const c = getNatureEnvironmentLayer(recolored, 640, 360)
      expect(c).not.toBe(a)
      const resized = getNatureEnvironmentLayer(kinetic, 320, 180)
      expect(resized).not.toBe(a)
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it("returns null without a DOM and the scene still renders inline", () => {
    const theme = preset("nature-forest")
    if (!isKineticTheme(theme)) throw new Error("expected kinetic theme")
    expect(getNatureEnvironmentLayer(theme.kinetic, 641, 361)).toBeNull()
    const r = createRecorder()
    expect(drawKineticBackground(r.ctx, theme, 0)).toBe(true)
    expect(r.fillRects).toBeGreaterThan(0)
    expect(
      (r.ctx.drawImage as ReturnType<typeof vi.fn>).mock.calls.length
    ).toBe(0)
  })

  it("blits the cached environment layer when a DOM is available", () => {
    stubCanvasDocument()
    try {
      const r = createRecorder()
      drawKineticBackground(r.ctx, preset("nature-forest"), 0)
      expect(
        (r.ctx.drawImage as ReturnType<typeof vi.fn>).mock.calls.length
      ).toBeGreaterThan(0)
    } finally {
      vi.unstubAllGlobals()
    }
  })
})
