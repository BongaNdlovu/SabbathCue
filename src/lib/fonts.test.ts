import { existsSync, readFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { describe, expect, it } from "vitest"
import { BUILTIN_THEMES } from "./builtin-themes"
import { buildKineticBroadcastThemes } from "./kinetic-themes"
import {
  FONT_FAMILIES,
  FONT_FAMILY_GROUPS,
  SYSTEM_FONT_FAMILIES,
} from "./fonts"

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..")
const INDEX_CSS = join(REPO_ROOT, "src", "index.css")
const NODE_MODULES = join(REPO_ROOT, "node_modules")

interface FontFaceRule {
  family: string
  /** Package-relative specifier, e.g. `@fontsource/marcellus/files/...woff2`. */
  src: string
}

/**
 * Parses the `@font-face` blocks out of index.css. This is deliberately derived
 * from the stylesheet rather than restated as a literal: a hand-maintained
 * allowlist passes even when the font it names was never installed, which is
 * exactly the failure this suite exists to catch.
 */
function parseFontFaces(css: string): FontFaceRule[] {
  const rules: FontFaceRule[] = []
  const blockPattern = /@font-face\s*\{([^}]*)\}/g

  for (const [, body] of css.matchAll(blockPattern)) {
    const family = body.match(/font-family:\s*"([^"]+)"/)?.[1]
    const src = body.match(/src:\s*url\(\s*"([^"]+)"/)?.[1]
    if (family && src) rules.push({ family, src })
  }

  return rules
}

const css = readFileSync(INDEX_CSS, "utf8")
const fontFaces = parseFontFaces(css)
const bundledFamilies = new Set(fontFaces.map((rule) => rule.family))
const systemFamilies = new Set<string>(SYSTEM_FONT_FAMILIES)

function isRenderable(family: string): boolean {
  return bundledFamilies.has(family) || systemFamilies.has(family)
}

describe("index.css @font-face declarations", () => {
  it("parses a non-trivial number of faces", () => {
    // Guards the parser itself: a regex that silently matched nothing would
    // make every downstream assertion vacuously true.
    expect(fontFaces.length).toBeGreaterThan(20)
    expect(bundledFamilies.size).toBeGreaterThan(10)
  })

  it("points every src at a woff2 that exists on disk", () => {
    const missing = fontFaces
      .filter((rule) => !existsSync(join(NODE_MODULES, rule.src)))
      .map((rule) => `${rule.family} -> ${rule.src}`)

    expect(missing).toEqual([])
  })

  it("declares every referenced @fontsource package as a dependency", () => {
    const manifest = JSON.parse(
      readFileSync(join(REPO_ROOT, "package.json"), "utf8")
    ) as { dependencies?: Record<string, string> }
    const dependencies = new Set(Object.keys(manifest.dependencies ?? {}))

    const referenced = new Set(
      fontFaces.map((rule) => rule.src.split("/").slice(0, 2).join("/"))
    )
    const undeclared = [...referenced].filter(
      (pkg) => !dependencies.has(pkg)
    )

    // A font that resolves locally but is absent from package.json breaks on a
    // clean install, and the failure only shows up in CI or on a new machine.
    expect(undeclared).toEqual([])
  })
})

describe("operator font picker", () => {
  it("offers only families that can actually render", () => {
    const unrenderable = FONT_FAMILIES.filter((f) => !isRenderable(f))
    expect(unrenderable).toEqual([])
  })

  it("lists each family once across all groups", () => {
    expect(new Set(FONT_FAMILIES).size).toBe(FONT_FAMILIES.length)
  })

  it("gives every group a label and at least one family", () => {
    for (const group of FONT_FAMILY_GROUPS) {
      expect(group.label).not.toBe("")
      expect(group.families.length).toBeGreaterThan(0)
    }
  })

  it("does not bundle the families it calls system fonts", () => {
    // A system family that is also bundled is a contradiction: one of the two
    // lists is wrong, and the picker would be shipping bytes it claims not to.
    const contradictory = [...systemFamilies].filter((f) =>
      bundledFamilies.has(f)
    )
    expect(contradictory).toEqual([])
  })
})

describe("theme font families", () => {
  const kineticThemes = buildKineticBroadcastThemes()
  const allThemes = [...BUILTIN_THEMES, ...kineticThemes]

  it("covers a meaningful number of themes", () => {
    expect(allThemes.length).toBeGreaterThan(20)
  })

  it("names only families that can actually render", () => {
    const offenders = allThemes.flatMap((theme) =>
      [theme.verseText.fontFamily, theme.reference.fontFamily]
        .filter((family) => !isRenderable(family))
        .map((family) => `${theme.id}: ${family}`)
    )

    expect(offenders).toEqual([])
  })
})
