import type {
  SlideDeck,
  SlideDeckPresentationItemData,
  SlideDeckSection,
  SlideDeckSectionKind,
  SlideDeckSlide,
} from "@/types"

const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "webp", "gif"])

export function inferDeckSourceType(path: string): SlideDeck["sourceType"] {
  const lower = path.toLowerCase()
  if (lower.endsWith(".pdf")) return "pdf"
  return "images"
}

export function createSlideDeckFromFiles(paths: string[], title = "Custom slide deck"): SlideDeck {
  const slides: SlideDeckSlide[] = paths.map((path, index) => ({
    id: crypto.randomUUID(),
    index,
    label: `Slide ${index + 1}`,
    path,
  }))

  return {
    id: crypto.randomUUID(),
    title,
    sourceType: paths.some((path) => path.toLowerCase().endsWith(".pdf"))
      ? "pdf"
      : "images",
    slides,
    sections: inferDefaultSections(slides),
  }
}

export function inferDefaultSections(slides: SlideDeckSlide[]): SlideDeckSection[] {
  return slides.map((slide) => ({
    id: crypto.randomUUID(),
    kind: inferSectionKind(slide.label),
    label: slide.label,
    slideIndexes: [slide.index],
  }))
}

export function inferSectionKind(label: string): SlideDeckSectionKind {
  const lower = label.toLowerCase()
  if (lower.includes("chorus") || lower.includes("refrain")) return "chorus"
  if (lower.includes("bridge")) return "bridge"
  if (lower.includes("ending") || lower.includes("close")) return "ending"
  if (lower.includes("intro")) return "intro"
  if (lower.includes("verse")) return "verse"
  return "custom"
}

export function createSlideDeckPresentationItem(
  deck: SlideDeck,
  slideIndex: number,
): SlideDeckPresentationItemData | null {
  const slide = deck.slides[slideIndex]
  if (!slide) return null
  const section = deck.sections.find((entry) => entry.slideIndexes.includes(slide.index))
  const label = section?.label ?? slide.label

  return {
    kind: "slideDeck",
    deckId: deck.id,
    deckTitle: deck.title,
    slideId: slide.id,
    slideIndex: slide.index,
    slideCount: deck.slides.length,
    slidePath: slide.path,
    sectionId: section?.id,
    sectionLabel: section?.label,
    reference: `${deck.title} - ${label}`,
    segments: [{ text: label }],
  }
}

export function isSupportedDeckInput(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? ""
  return IMAGE_EXTENSIONS.has(ext) || ext === "pdf"
}
