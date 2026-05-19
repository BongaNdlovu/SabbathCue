import type { BroadcastTheme, VerseRenderData } from "@/types"

export function getBroadcastRenderKey(
  theme: BroadcastTheme,
  verse: VerseRenderData | null,
): string {
  return JSON.stringify({
    theme: {
      id: theme.id,
      updatedAt: theme.updatedAt,
      resolution: theme.resolution,
      background: theme.background,
      textBox: theme.textBox,
      verseText: theme.verseText,
      verseNumbers: theme.verseNumbers,
      reference: theme.reference,
      layout: theme.layout,
    },
    verse,
  })
}
