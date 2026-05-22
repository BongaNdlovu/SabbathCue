import type { BroadcastTheme } from "@/types/broadcast"

const baseTheme: Omit<
  BroadcastTheme,
  | "id"
  | "name"
  | "background"
  | "verseText"
  | "reference"
  | "layout"
  | "transition"
  | "textBox"
> = {
  builtin: true,
  pinned: false,
  createdAt: 0,
  updatedAt: 0,
  resolution: { width: 1920, height: 1080 },
  verseNumbers: {
    visible: true,
    fontSize: 14,
    color: "#ffffff",
    superscript: true,
  },
}

const CLASSIC_DARK: BroadcastTheme = {
  ...baseTheme,
  id: "builtin-classic-dark",
  name: "Classic Dark",
  background: {
    type: "gradient",
    color: "#1a1a3e",
    gradient: {
      type: "radial",
      angle: 0,
      stops: [
        { color: "#1a1a3e", position: 0 },
        { color: "#0a0a1a", position: 100 },
      ],
    },
    image: null,
  },
  textBox: {
    enabled: false,
    color: "#000000",
    opacity: 0,
    borderRadius: 0,
    padding: 0,
  },
  verseText: {
    fontFamily: "Source Serif 4 Variable",
    fontSize: 72,
    fontWeight: 400,
    color: "#ffffff",
    horizontalAlign: "center",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    lineHeight: 1.5,
    letterSpacing: 0,
    shadow: null,
    outline: null,
  },
  verseNumbers: {
    visible: true,
    fontSize: 20,
    color: "#d4a574",
    superscript: true,
  },
  reference: {
    fontFamily: "Geist Variable",
    fontSize: 48,
    fontWeight: 500,
    color: "#d4a574",
    horizontalAlign: "center",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    uppercase: true,
    letterSpacing: 2,
    position: "above",
  },
  layout: {
    anchor: "center",
    offsetX: 0,
    offsetY: 0,
    padding: { top: 60, right: 80, bottom: 60, left: 80 },
    textAlign: "center",
    backgroundWidth: 100,
    backgroundHeight: 100,
    textAreaWidth: 80,
    textAreaHeight: 80,
    referenceGap: 32,
  },
  transition: {
    type: "fade",
    duration: 500,
    easing: "ease-in-out",
    direction: "up",
  },
}

const MODERN_LIGHT: BroadcastTheme = {
  ...baseTheme,
  id: "builtin-modern-light",
  name: "Modern Light",
  background: {
    type: "solid",
    color: "#f5f5f0",
    gradient: null,
    image: null,
  },
  textBox: {
    enabled: false,
    color: "#000000",
    opacity: 0,
    borderRadius: 0,
    padding: 0,
  },
  verseText: {
    fontFamily: "Geist Variable",
    fontSize: 68,
    fontWeight: 400,
    color: "#1a1a1a",
    horizontalAlign: "left",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    lineHeight: 1.6,
    letterSpacing: 0,
    shadow: null,
    outline: null,
  },
  verseNumbers: {
    visible: true,
    fontSize: 18,
    color: "#666666",
    superscript: true,
  },
  reference: {
    fontFamily: "Geist Variable",
    fontSize: 45,
    fontWeight: 500,
    color: "#666666",
    horizontalAlign: "left",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    uppercase: false,
    letterSpacing: 0,
    position: "above",
  },
  layout: {
    anchor: "center",
    offsetX: 0,
    offsetY: 0,
    padding: { top: 60, right: 80, bottom: 60, left: 80 },
    textAlign: "left",
    backgroundWidth: 100,
    backgroundHeight: 100,
    textAreaWidth: 80,
    textAreaHeight: 80,
    referenceGap: 30,
  },
  transition: {
    type: "slide",
    duration: 400,
    easing: "ease-out",
    direction: "up",
  },
}

const BROADCAST_OVERLAY: BroadcastTheme = {
  ...baseTheme,
  id: "builtin-broadcast-overlay",
  name: "Broadcast Overlay",
  background: {
    type: "transparent",
    color: "transparent",
    gradient: null,
    image: null,
  },
  textBox: {
    enabled: true,
    color: "#000000",
    opacity: 0.7,
    borderRadius: 12,
    padding: 24,
  },
  verseText: {
    fontFamily: "Geist Variable",
    fontSize: 64,
    fontWeight: 500,
    color: "#ffffff",
    horizontalAlign: "center",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    lineHeight: 1.5,
    letterSpacing: 0,
    shadow: { color: "rgba(0,0,0,0.8)", blur: 8, x: 0, y: 2 },
    outline: null,
  },
  verseNumbers: {
    visible: true,
    fontSize: 18,
    color: "#fbbf24",
    superscript: true,
  },
  reference: {
    fontFamily: "Geist Variable",
    fontSize: 43,
    fontWeight: 600,
    color: "#fbbf24",
    horizontalAlign: "center",
    verticalAlign: "top",
    textTransform: "none",
    textDecoration: "none",
    uppercase: false,
    letterSpacing: 1,
    position: "below",
  },
  layout: {
    anchor: "bottom-center",
    offsetX: 0,
    offsetY: 0,
    padding: { top: 40, right: 60, bottom: 40, left: 60 },
    textAlign: "center",
    backgroundWidth: 100,
    backgroundHeight: 100,
    textAreaWidth: 90,
    textAreaHeight: 40,
    referenceGap: 24,
  },
  transition: {
    type: "fade",
    duration: 300,
    easing: "ease-in-out",
    direction: "up",
  },
}

const FOREST_GLASS: BroadcastTheme = {
  ...MODERN_LIGHT,
  id: "builtin-forest-glass",
  name: "Forest Glass",
  background: {
    type: "gradient",
    color: "#13342f",
    gradient: {
      type: "linear",
      angle: 135,
      stops: [
        { color: "#13342f", position: 0 },
        { color: "#d7c27a", position: 100 },
      ],
    },
    image: null,
  },
  textBox: {
    enabled: true,
    color: "#071512",
    opacity: 0.55,
    borderRadius: 8,
    padding: 28,
  },
  verseText: {
    ...MODERN_LIGHT.verseText,
    fontSize: 66,
    color: "#f9faf7",
    lineHeight: 1.55,
  },
  verseNumbers: {
    visible: true,
    fontSize: 18,
    color: "#f1d98a",
    superscript: true,
  },
  reference: {
    ...MODERN_LIGHT.reference,
    color: "#f1d98a",
    uppercase: true,
    letterSpacing: 1,
  },
}

const STAINED_WARMTH: BroadcastTheme = {
  ...CLASSIC_DARK,
  id: "builtin-stained-warmth",
  name: "Stained Warmth",
  background: {
    type: "gradient",
    color: "#35142a",
    gradient: {
      type: "radial",
      angle: 0,
      stops: [
        { color: "#7c2d12", position: 0 },
        { color: "#35142a", position: 58 },
        { color: "#111827", position: 100 },
      ],
    },
    image: null,
  },
  verseText: { ...CLASSIC_DARK.verseText, fontSize: 70, color: "#fff7ed" },
  verseNumbers: {
    visible: true,
    fontSize: 20,
    color: "#f59e0b",
    superscript: true,
  },
  reference: { ...CLASSIC_DARK.reference, color: "#fbbf24", letterSpacing: 1 },
}

const CLEAN_LOWER_THIRD: BroadcastTheme = {
  ...BROADCAST_OVERLAY,
  id: "builtin-clean-lower-third",
  name: "Clean Lower Third",
  textBox: {
    enabled: true,
    color: "#0f172a",
    opacity: 0.82,
    borderRadius: 6,
    padding: 22,
  },
  verseText: {
    ...BROADCAST_OVERLAY.verseText,
    fontSize: 54,
    fontWeight: 500,
    lineHeight: 1.38,
  },
  verseNumbers: {
    visible: false,
    fontSize: 16,
    color: "#38bdf8",
    superscript: true,
  },
  reference: {
    ...BROADCAST_OVERLAY.reference,
    fontSize: 30,
    color: "#38bdf8",
    horizontalAlign: "right",
    position: "below",
  },
  layout: {
    ...BROADCAST_OVERLAY.layout,
    anchor: "bottom-center",
    textAreaWidth: 88,
    textAreaHeight: 32,
    padding: { top: 40, right: 60, bottom: 56, left: 60 },
  },
}

const PAPER_READING: BroadcastTheme = {
  ...MODERN_LIGHT,
  id: "builtin-paper-reading",
  name: "Paper Reading",
  background: { type: "solid", color: "#fbfaf6", gradient: null, image: null },
  textBox: {
    enabled: false,
    color: "#000000",
    opacity: 0,
    borderRadius: 0,
    padding: 0,
  },
  verseText: {
    ...MODERN_LIGHT.verseText,
    fontFamily: "Source Serif 4 Variable",
    fontSize: 64,
    color: "#1f2933",
    lineHeight: 1.7,
  },
  verseNumbers: {
    visible: true,
    fontSize: 17,
    color: "#8a5a2b",
    superscript: true,
  },
  reference: {
    ...MODERN_LIGHT.reference,
    fontFamily: "Geist Variable",
    fontSize: 38,
    color: "#8a5a2b",
    uppercase: true,
    letterSpacing: 1,
  },
}

const MIDNIGHT_GOLD: BroadcastTheme = {
  ...CLASSIC_DARK,
  id: "builtin-midnight-gold",
  name: "Midnight Gold",
  background: {
    type: "gradient",
    color: "#050816",
    gradient: {
      type: "linear",
      angle: 120,
      stops: [
        { color: "#050816", position: 0 },
        { color: "#172554", position: 55 },
        { color: "#422006", position: 100 },
      ],
    },
    image: null,
  },
  verseText: {
    ...CLASSIC_DARK.verseText,
    fontSize: 74,
    fontWeight: 500,
    color: "#f8fafc",
  },
  verseNumbers: {
    visible: true,
    fontSize: 18,
    color: "#facc15",
    superscript: true,
  },
  reference: { ...CLASSIC_DARK.reference, color: "#facc15", fontSize: 42 },
}

const HYMNS_BIG_LYRICS: BroadcastTheme = {
  ...CLASSIC_DARK,
  id: "builtin-hymns-big-lyrics",
  name: "Hymns Big Lyrics",
  background: { type: "solid", color: "#111111", gradient: null, image: null },
  textBox: {
    enabled: false,
    color: "#000000",
    opacity: 0,
    borderRadius: 0,
    padding: 0,
  },
  verseText: {
    ...CLASSIC_DARK.verseText,
    fontFamily: "Geist Variable",
    fontSize: 86,
    fontWeight: 700,
    lineHeight: 1.25,
  },
  verseNumbers: {
    visible: false,
    fontSize: 16,
    color: "#ffffff",
    superscript: true,
  },
  reference: {
    ...CLASSIC_DARK.reference,
    fontSize: 34,
    color: "#22c55e",
    position: "below",
  },
  layout: {
    ...CLASSIC_DARK.layout,
    textAreaWidth: 88,
    textAreaHeight: 78,
    referenceGap: 24,
  },
}

const CALM_BLUE_WHITE: BroadcastTheme = {
  ...MODERN_LIGHT,
  id: "builtin-calm-blue-white",
  name: "Calm Blue White",
  background: {
    type: "gradient",
    color: "#eaf6ff",
    gradient: {
      type: "linear",
      angle: 180,
      stops: [
        { color: "#eaf6ff", position: 0 },
        { color: "#ffffff", position: 55 },
        { color: "#e8fff4", position: 100 },
      ],
    },
    image: null,
  },
  verseText: { ...MODERN_LIGHT.verseText, fontSize: 68, color: "#0f172a" },
  verseNumbers: {
    visible: true,
    fontSize: 17,
    color: "#0369a1",
    superscript: true,
  },
  reference: {
    ...MODERN_LIGHT.reference,
    color: "#047857",
    fontSize: 40,
    uppercase: true,
  },
}

export const BUILTIN_THEMES: BroadcastTheme[] = [
  CLASSIC_DARK,
  MODERN_LIGHT,
  BROADCAST_OVERLAY,
  FOREST_GLASS,
  STAINED_WARMTH,
  CLEAN_LOWER_THIRD,
  PAPER_READING,
  MIDNIGHT_GOLD,
  HYMNS_BIG_LYRICS,
  CALM_BLUE_WHITE,
]
