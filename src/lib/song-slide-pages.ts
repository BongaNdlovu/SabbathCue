import { splitLyricLineForReadableSlides } from "@/lib/text-slide-chunking"

/**
 * Treats each blank-line or `---` separated block as an authored page.
 * Individual long lines can wrap for readability, but an authored page is
 * never silently divided into additional presentation pages.
 */
export function parseAuthoredSongPages(text: string): string[][] {
  const normalized = text
    .replace(/\r\n?/g, "\n")
    .replace(/\n[ \t]*---+[ \t]*\n/g, "\n\n")

  return normalized
    .split(/\n[ \t]*\n+/g)
    .map((block) =>
      block
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .flatMap((line) => splitLyricLineForReadableSlides(line))
    )
    .filter((lines) => lines.length > 0)
}
