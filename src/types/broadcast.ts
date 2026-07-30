export type BroadcastOutputId = "main" | "alt"
export type BroadcastIssueOutputId = BroadcastOutputId | "global"

export type BroadcastOutputIssueKind =
  | "broadcast-sync"
  | "preview-open"
  | "ndi-config"
  | "ndi-frame"
  | "detection-settings"
  | "manual-detection"
  | "auto-detection"
  | "verse-lookup"
  | "video-audio"
  | "persistence"

export interface BroadcastOutputIssue {
  id: string
  outputId: BroadcastIssueOutputId
  kind: BroadcastOutputIssueKind
  title: string
  description: string
  firstSeenAt: number
  lastSeenAt: number
  count: number
}

export interface BroadcastOutputErrorEvent {
  outputId: "main" | "alt"
  kind: BroadcastOutputIssueKind
  title: string
  description: string
}

export interface VerseSegment {
  verseNumber?: number
  text: string
}

export interface VerseRenderData {
  reference: string
  segments: VerseSegment[]
}

export interface RenderOptions {
  opacity?: number
  offsetX?: number
  offsetY?: number
  scale?: number // Scale factor for rendering at display size (e.g., 0.42 for 400px panel)
  imageCache?: Map<string, HTMLImageElement>
  /**
   * Animation clock in milliseconds for kinetic themes. `0` (or omitted)
   * produces a deterministic static frame, which keeps tests stable and lets
   * static thumbnails render the same image every time. Only kinetic themes
   * read this value; static themes ignore it entirely.
   */
  timeMs?: number
}

// --- Kinetic (moving-background) theme support ---------------------------------
// Kinetic themes are an additive, preset-based path layered on top of the
// existing static `BroadcastTheme`. The metadata below is the canvas-native
// description of the HTML prototype's CSS motion (liquidMesh / drift / dot-grid
// / diagonal stripes). It carries no DOM/CSS — the kinetic renderer turns it
// into deterministic canvas draw calls so the same motion works for NDI.

export type KineticBackgroundKind =
  | "mesh"
  | "grid"
  | "stripes"
  // Nature scenes (deterministic canvas particle systems).
  | "foliage"
  | "forest"
  | "rain"
  | "autumn"
  | "blossom"
  | "snow"
  | "fireflies"
  | "stars"
  | "meadow"
  | "aurora"
  // Worship scenes (deterministic canvas ports of standalone HTML designs).
  | "cloth"
  // KNFC premium-light verse stage: gradient, glow, conic shimmer, and stars.
  | "stage"
  // Full-fidelity ports of the seven premium hymn HTML scenes.
  | "hymn-midnight"
  | "hymn-dawn"
  | "hymn-minimal"
  | "hymn-glass"
  | "hymn-water"
  | "hymn-heritage"
  | "hymn-upper-room"

export type KineticPattern = "dot-grid" | "diagonal-stripes"

export interface KineticMotion {
  /** Full loop duration in ms (mirrors the CSS animation duration). */
  durationMs: number
  /** Relative blob/mesh travel distance, roughly 0..1. */
  driftAmount: number
  /** Peak hue rotation across the loop, in degrees. */
  hueShiftDegrees: number
  /** Peak saturation multiplier boost across the loop (e.g. 0.3 = +30%). */
  saturationBoost: number
}

export interface BroadcastKineticTheme {
  source: "html-prototype-v2"
  presetId: string
  group: "classical" | "modern" | "nature"
  backgroundKind: KineticBackgroundKind
  /** Mesh gradient stop colors (the four corner colors from the prototype). */
  colors: string[]
  /** Accent color used for dot-grid / stripe overlays and glow. */
  accentColor: string
  motion: KineticMotion
  /** False renders the procedural scene at time zero without scheduling frames. */
  animate?: boolean
  /** Optional overlay pattern drawn on top of the moving base. */
  pattern?: KineticPattern
}

export type TextHorizontalAlign = "left" | "center" | "right" | "justify"
export type TextVerticalAlign = "top" | "middle" | "bottom"
export type TextTransform = "none" | "uppercase" | "lowercase" | "capitalize"
export type TextDecoration = "none" | "underline" | "line-through"
export type BroadcastTransitionType = "fade" | "slide" | "scale" | "none"
export type BroadcastTransitionEasing =
  "linear" | "ease-in" | "ease-out" | "ease-in-out"
export type BroadcastTransitionDirection = "up" | "down" | "left" | "right"

export interface BroadcastTransition {
  type: BroadcastTransitionType
  duration: number
  easing: BroadcastTransitionEasing
  direction: BroadcastTransitionDirection
}

/** Optional hymn slide layout overrides any theme can adopt. */
export interface HymnPresentationStyle {
  /** Show the hymn title instead of the full reference string. */
  titleOnly?: boolean
  slideCounter?: {
    position?: "top-right" | "bottom-right"
    format?: "of" | "slash"
    style?: "badge" | "plain"
  }
  /** Render the authored section label above the lyrics. */
  showSectionLabel?: boolean
  /** Text shown for refrain/chorus pages in place of their source label. */
  refrainLabel?: string
  /** Per-section typography layered over the base theme without mutating it. */
  sectionStyles?: Partial<
    Record<
      "verse" | "refrain" | "chorus" | "bridge" | "ending" | "custom",
      {
        verseText?: {
          fontStyle?: "normal" | "italic"
          fontSize?: number
          fontWeight?: number
          color?: string
          lineHeight?: number
          letterSpacing?: number
        }
      }
    >
  >
}

export interface BroadcastTheme {
  id: string
  name: string
  builtin: boolean
  pinned: boolean
  createdAt: number
  updatedAt: number
  resolution: { width: number; height: number }
  background: {
    type: "solid" | "gradient" | "image" | "transparent"
    color: string
    gradient: {
      type: "linear" | "radial"
      angle: number
      stops: { color: string; position: number }[]
    } | null
    image: {
      url: string
      fit: "cover" | "contain" | "stretch"
      blur: number
      brightness: number
      tint: string | null
    } | null
  }
  textBox: {
    enabled: boolean
    color: string
    opacity: number
    borderRadius: number
    padding: number
  }
  verseText: {
    fontFamily: string
    /** Optional; absent on all pre-existing themes (renders as "normal"). */
    fontStyle?: "normal" | "italic"
    fontSize: number
    fontWeight: number
    color: string
    horizontalAlign?: TextHorizontalAlign
    verticalAlign?: TextVerticalAlign
    textTransform?: TextTransform
    textDecoration?: TextDecoration
    lineHeight: number
    letterSpacing: number
    shadow: { color: string; blur: number; x: number; y: number } | null
    outline: { color: string; width: number } | null
  }
  verseNumbers: {
    visible: boolean
    fontSize: number
    color: string
    superscript: boolean
  }
  reference: {
    fontFamily: string
    fontSize: number
    fontWeight: number
    color: string
    horizontalAlign?: TextHorizontalAlign
    verticalAlign?: TextVerticalAlign
    textTransform?: TextTransform
    textDecoration?: TextDecoration
    uppercase: boolean
    letterSpacing: number
    position: "above" | "below" | "inline"
  }
  layout: {
    anchor:
      | "center"
      | "top-left"
      | "top-center"
      | "top-right"
      | "bottom-left"
      | "bottom-center"
      | "bottom-right"
    offsetX: number
    offsetY: number
    padding: { top: number; right: number; bottom: number; left: number }
    textAlign: "left" | "center" | "right"
    backgroundWidth: number
    backgroundHeight: number
    textAreaWidth: number
    textAreaHeight: number
    referenceGap?: number
  }
  transition: BroadcastTransition
  /**
   * Optional kinetic (moving-background) metadata. Present only on kinetic
   * presets; `undefined` on every existing static/custom theme so persisted
   * themes remain fully backward-compatible.
   */
  kinetic?: BroadcastKineticTheme
  /** Optional hymn-specific layout; omitted on themes that keep the default. */
  hymnPresentation?: HymnPresentationStyle
}
