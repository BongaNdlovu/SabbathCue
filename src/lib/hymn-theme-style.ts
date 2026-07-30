import type {
  BroadcastTheme,
  HymnPresentationSectionKind,
  PresentationRenderData,
} from "@/types"

function sectionStyleKey(
  data: PresentationRenderData | null
): HymnPresentationSectionKind | null {
  if (data?.kind !== "hymn") return null
  const kind = data.hymnSlide?.sectionKind
  if (!kind) return null
  return kind
}

export function resolveHymnSectionTheme(
  theme: BroadcastTheme,
  data: PresentationRenderData | null
): BroadcastTheme {
  const sectionStyles = theme.hymnPresentation?.sectionStyles
  const key = sectionStyleKey(data)
  if (!sectionStyles || !key) return theme

  const direct = sectionStyles[key]
  const override =
    direct ?? (key === "chorus" ? sectionStyles.refrain : undefined)
  if (!override?.verseText) return theme

  return {
    ...theme,
    verseText: {
      ...theme.verseText,
      ...override.verseText,
    },
  }
}
