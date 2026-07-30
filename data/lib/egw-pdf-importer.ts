import { existsSync, mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs"
import { cleanEgwParagraphs } from "./egw-text-cleanup"
import {
  reconstructPageParagraphs,
  type ParagraphLayoutOptions,
  type PdfTextItemLike,
} from "./egw-paragraph-layout"

export interface EgwChapterConfig {
  chapter: number
  title: string
}

export interface EgwBookConfig {
  title: string
  abbreviation: string
  book_number: number
  chapterAnchorTemplate: string
  expectedChapterCount: number
  pdfPath: string
  outputJsonPath: string
  debugSlug: string
  requiredTokens: string[]
  appendixMarker?: string
  /** Per-book overrides for layout-aware paragraph reconstruction. */
  layout?: ParagraphLayoutOptions
  /**
   * Where printed page numbers come from. "brackets": the PDF embeds
   * standard-edition page breaks as inline [n] markers (PP/Ed/GC) — these
   * are the citation pages, and the PDF's own folio/header numbers are a
   * different sequence that must be ignored. "legacy" (default): no reliable
   * bracket markers (SC); standalone printed-page lines are promoted to [n]
   * markers and the table of contents seeds chapter start pages. "folios"
   * keeps bracket markers for paragraph boundaries, then remaps page labels to
   * visible running-header folios and TOC chapter starts.
   */
  pageSource?: "brackets" | "legacy" | "folios"
  splitReadableParagraphs?: boolean
  countContinuedPagesForPageParagraphs?: boolean
  postprocessChapters?: (chapters: EgwDraftChapter[]) => EgwDraftChapter[]
  chapters: readonly EgwChapterConfig[]
}

type OutputChapter = {
  chapter: number
  title: string
  paragraphs: Array<{
    paragraph: number
    page: number
    page_paragraph: number
    text: string
  }>
}

type DraftChapter = Omit<OutputChapter, "paragraphs"> & {
  paragraphs: Array<{
    paragraph: number
    page?: number
    continued_pages?: number[]
    text: string
  }>
}

export type EgwDraftChapter = DraftChapter

const PAGE_MARKER_PATTERN_SOURCE = String.raw`\[([ivxlcdm\d]{1,8})\]`
const FOLIO_MARKER_PATTERN_SOURCE = String.raw`\{\{folio:(\d{1,4})\}\}`

function pageMarkerPattern(): RegExp {
  return new RegExp(PAGE_MARKER_PATTERN_SOURCE, "gi")
}

function folioMarkerPattern(): RegExp {
  return new RegExp(FOLIO_MARKER_PATTERN_SOURCE, "g")
}

function trackingMarkerPattern(): RegExp {
  return new RegExp(
    `${PAGE_MARKER_PATTERN_SOURCE}|${FOLIO_MARKER_PATTERN_SOURCE}`,
    "gi"
  )
}

function repoRoot(): string {
  return import.meta.dir === join(process.cwd(), "data", "lib")
    ? process.cwd()
    : dirname(dirname(import.meta.dir))
}

function normalizePageText(raw: string): string {
  return raw
    .replace(/\r\n/g, "\n")
    .replace(/\u00ad/g, "")
    .replace(/\u2019/g, "'")
    .replace(/\u2018/g, "'")
    .replace(/\u201c/g, '"')
    .replace(/\u201d/g, '"')
    .replace(/\u2014/g, "-")
    .replace(/\u2013/g, "-")
    .replace(/-\n([a-z])/g, "$1")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/[ \t]{2,}/g, " ")
}

function normalizeFullText(text: string): string {
  return text
    .replace(/\r\n/g, "\n")
    .replace(/\u00ad/g, "")
    .replace(/\u2019/g, "'")
    .replace(/\u2018/g, "'")
    .replace(/\u201c/g, '"')
    .replace(/\u201d/g, '"')
    .replace(/\u2014/g, "-")
    .replace(/\u2013/g, "-")
    .replace(/-\n([a-z])/g, "$1")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/(?<!\n)\n(?!\n)/g, " ")
    .replace(/[ \t]{2,}/g, " ")
    .trim()
}

function cleanParagraph(text: string): string {
  return text
    .replace(/\s+/g, " ")
    .replace(/\s+([,.;:!?])/g, "$1")
    .trim()
}

function parseNumericPageMarker(value: string): number | null {
  if (!/^\d{1,4}$/.test(value)) return null
  const page = Number(value)
  return Number.isInteger(page) && page > 0 ? page : null
}

function stripPageMarkers(text: string): string {
  return text.replace(pageMarkerPattern(), " ")
}

function folioMarker(page: number): string {
  return `{{folio:${page}}}`
}

function stripFolioMarkers(text: string): string {
  return text.replace(folioMarkerPattern(), " ")
}

function stripTrackingMarkers(text: string): string {
  return stripFolioMarkers(stripPageMarkers(text))
}

function hasNumericBracketPageMarker(text: string): boolean {
  const pattern = pageMarkerPattern()
  let match: RegExpExecArray | null
  while ((match = pattern.exec(text)) !== null) {
    if (parseNumericPageMarker(match[1] ?? "") != null) {
      return true
    }
  }
  return false
}

function standalonePrintedPageLine(
  lines: string[]
): { index: number; page: number } | null {
  const candidates = [0, lines.length - 1]
  for (const index of candidates) {
    const page = parseNumericPageMarker(lines[index]?.trim() ?? "")
    if (page != null) {
      return { index, page }
    }
  }
  return null
}

function preserveStandalonePrintedPageMarker(text: string): string {
  if (hasNumericBracketPageMarker(text)) return text

  const lines = text.split("\n")
  const nonEmpty = lines
    .map((line, index) => ({ index, text: line.trim() }))
    .filter((line) => line.text.length > 0)
  const marker = standalonePrintedPageLine(nonEmpty.map((line) => line.text))
  if (!marker) return text

  const originalIndex = nonEmpty[marker.index]?.index
  if (originalIndex == null) return text

  lines.splice(originalIndex, 1)
  const body = lines.join("\n").trim()
  return body ? `[${marker.page}]\n${body}` : `[${marker.page}]`
}

function printedFolioFromRunningHeader(text: string): number | null {
  const leading = text.match(/^(\d{1,4})\s+\D/)
  if (leading?.[1]) {
    return parseNumericPageMarker(leading[1])
  }

  const trailing = text.match(/\D\s+(\d{1,4})$/)
  return trailing?.[1] ? parseNumericPageMarker(trailing[1]) : null
}

function visibleFolioFromPageText(text: string): number | undefined {
  const lines = stripPageMarkers(text)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
  if (lines.length === 0) return undefined

  const headerPage = printedFolioFromRunningHeader(lines[0] ?? "")
  if (headerPage != null) return headerPage

  return standalonePrintedPageLine(lines)?.page
}

function splitParagraphsWithPages(
  chapterText: string,
  chapterStartPage?: number,
  pageSource: "page-markers" | "folio-markers" = "page-markers"
): Array<{ page?: number; continued_pages?: number[]; text: string }> {
  let currentPage = chapterStartPage
  const paragraphs: Array<{
    page?: number
    continued_pages?: number[]
    text: string
  }> = []

  for (const rawChunk of chapterText.split(/\n\s*\n/g)) {
    const chunk = rawChunk.trim()
    if (!chunk) continue

    let pageForParagraph = currentPage
    let sawTextBeforeMarker = false
    let lastIndex = 0
    const continuedPages: number[] = []
    const markerPattern =
      pageSource === "folio-markers"
        ? trackingMarkerPattern()
        : pageMarkerPattern()

    let match: RegExpExecArray | null
    while ((match = markerPattern.exec(chunk)) !== null) {
      const before = chunk.slice(lastIndex, match.index)
      if (stripTrackingMarkers(before).trim()) {
        sawTextBeforeMarker = true
      }

      const markerPage =
        pageSource === "folio-markers"
          ? parseNumericPageMarker(match[2] ?? "")
          : parseNumericPageMarker(match[1] ?? "")
      if (markerPage != null) {
        if (!sawTextBeforeMarker) {
          pageForParagraph = markerPage
        } else if (pageForParagraph == null) {
          pageForParagraph = markerPage
        } else if (markerPage !== pageForParagraph) {
          continuedPages.push(markerPage)
        }
        currentPage = markerPage
      }
      lastIndex = match.index + match[0].length
    }

    const text = cleanParagraph(stripTrackingMarkers(chunk))
    if (text.length > 0) {
      paragraphs.push({
        page: pageForParagraph,
        continued_pages: continuedPages.length > 0 ? continuedPages : undefined,
        text,
      })
    }
  }

  return paragraphs
}

function alignFolioParagraphs(
  chapter: number,
  citationParagraphs: Array<{ text: string }>,
  folioParagraphs: Array<{
    page?: number
    continued_pages?: number[]
    text: string
  }>
): Array<{ page?: number; continued_pages?: number[]; text: string }> {
  const aligned: Array<{
    page?: number
    continued_pages?: number[]
    text: string
  }> = []
  let folioIndex = 0

  for (
    let citationIndex = 0;
    citationIndex < citationParagraphs.length;
    citationIndex += 1
  ) {
    const citation = citationParagraphs[citationIndex]
    while (
      folioIndex < folioParagraphs.length &&
      folioParagraphs[folioIndex]?.text !== citation.text
    ) {
      if (/^\d{1,4}$/.test(folioParagraphs[folioIndex]?.text ?? "")) {
        folioIndex += 1
        continue
      }
      const folioText = folioParagraphs[folioIndex]?.text ?? ""
      let diffAt = 0
      while (
        diffAt < citation.text.length &&
        citation.text[diffAt] === folioText[diffAt]
      ) {
        diffAt += 1
      }
      throw new Error(
        `Folio paragraph text alignment failed in chapter ${chapter} at ${citationIndex + 1}, ` +
          `first difference at ${diffAt}: ` +
          `citation "…${citation.text.slice(Math.max(0, diffAt - 40), diffAt + 60)}" ` +
          `vs folio "…${folioText.slice(Math.max(0, diffAt - 40), diffAt + 60)}"`
      )
    }

    const folio = folioParagraphs[folioIndex]
    if (!folio) {
      throw new Error(
        `Folio paragraph alignment failed in chapter ${chapter}: missing output paragraph ${citationIndex + 1}`
      )
    }
    aligned.push(folio)
    folioIndex += 1
  }

  return aligned
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

function chapterHeaderTitlePatterns(title: string): string[] {
  const variants = [title]
  const withoutLeadingThe = title.replace(/^The\s+/i, "")
  if (withoutLeadingThe !== title) variants.push(withoutLeadingThe)

  return variants.map(escapeRegExp)
}

export function stripChapterFurniture(
  raw: string,
  bookTitle: string,
  currentTitle: string
): string {
  const escBook = escapeRegExp(bookTitle)
  const chapterHeaderTitles = chapterHeaderTitlePatterns(currentTitle)
  return (
    chapterHeaderTitles
      .reduce(
        (text, escTitle) =>
          // The trailing (?![:\d]) keeps scripture references intact: a
          // chapter titled "The Exodus" must not eat "Exodus 12" out of
          // "Exodus 12:34" — running headers end in a bare page number.
          text.replace(
            new RegExp(`(?<!\\w)${escTitle}\\s+\\d{1,3}\\b(?![:\\d])`, "gi"),
            " "
          ),
        raw
      )
      // Running headers only — never a bare title word. Even pages print
      // "<page> <Book Title>", odd pages "<Chapter Title> <page>"; both are
      // removed number-and-all. The header number is the PDF file's own print
      // pagination, a different sequence from the standard-edition [n] bracket
      // markers that carry citation pages, so it must not leak into the page
      // stream. The former `\b<Book Title>\b` strip also deleted every ordinary
      // occurrence of the title word — erasing "education" from Education and
      // "the great controversy" from GC — which is why only full headers match.
      .replace(new RegExp(`\\b\\d{1,3}\\s+${escBook}\\b`, "gi"), " ")
      // Appendix pages carry "Appendix <page>" running headers that belong to
      // no chapter title; body references use "Note …. Page …" wording, never
      // "Appendix <number>".
      .replace(/(?<!\w)Appendix\s+\d{1,4}\b(?![:\d])/gi, " ")
      .replace(/^\s*\d+\s*$/gm, "")
      .replace(/^\s*Contents\s*$/gim, "")
      .replace(/^\s*Appendix\s*$/gim, "")
      .replace(/^\s*Foreword\s*$/gim, "")
      .replace(/^\s*Preface\s*$/gim, "")
      .trim()
  )
}

/**
 * Locate a chapter anchor, tolerating a printed-page marker interleaved between
 * the anchor's words (a wrapped chapter title can have "[698]" land between its
 * lines, e.g. DA ch. 75 "…the Court of [698] Caiaphas"). Returns the LAST
 * occurrence — mirroring the previous lastIndexOf behavior, which skips the
 * table-of-contents entry — with the matched length so callers can slice past
 * whatever variant actually matched.
 */
export function findChapterAnchor(
  text: string,
  anchor: string
): { pos: number; length: number } | null {
  const markerBetweenWords = `(?:${PAGE_MARKER_PATTERN_SOURCE}|${FOLIO_MARKER_PATTERN_SOURCE})`
  const pattern = new RegExp(
    anchor
      .split(/\s+/)
      .filter(Boolean)
      .map(escapeRegExp)
      .join(String.raw`\s+` + `(?:${markerBetweenWords}\\s+)?`),
    "g"
  )

  let last: { pos: number; length: number } | null = null
  let match: RegExpExecArray | null
  while ((match = pattern.exec(text)) !== null) {
    last = { pos: match.index, length: match[0].length }
  }
  return last
}

function chapterAnchor(
  template: string,
  chapter: number,
  title: string
): string {
  return template
    .replaceAll("{chapter}", String(chapter))
    .replaceAll("{title}", title)
}

function ensureUsableText(
  fullText: string,
  requiredTokens: readonly string[]
): void {
  const normalizedText = normalizeFullText(fullText)
  for (const token of requiredTokens) {
    const normalizedToken = normalizeFullText(token)
    if (!normalizedText.includes(normalizedToken)) {
      throw new Error(
        `PDF text layer is not usable for deterministic conversion: missing "${token}".`
      )
    }
  }
}

function titlePattern(title: string): string {
  return title
    .replace(/["']/g, "")
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map(escapeRegExp)
    .map((part) => part.replace(/["']/g, `["']?`))
    .join("\\s+")
}

function parseTocPageNumber(
  tocText: string,
  chapter: EgwChapterConfig
): number | undefined {
  const title = titlePattern(chapter.title)
  const chapterPattern = new RegExp(
    `\\b(?:Chapter|Chap\\.)\\s+${chapter.chapter}\\s*-\\s*["']*${title}["']*\\s*(?:\\.\\s*)+(\\d{1,4})\\b`,
    "i"
  )
  const chapterMatch = tocText.match(chapterPattern)
  if (chapterMatch?.[1]) return Number(chapterMatch[1])

  const titleOnlyPattern = new RegExp(
    `\\b["']*${title}["']*\\s*(?:\\.\\s*)+(\\d{1,4})\\b`,
    "i"
  )
  const titleMatch = tocText.match(titleOnlyPattern)
  return titleMatch?.[1] ? Number(titleMatch[1]) : undefined
}

function nearestPageMarkerBefore(
  text: string,
  pos: number
): number | undefined {
  const window = text.slice(Math.max(0, pos - 300), pos)
  let page: number | undefined
  const markerPattern = pageMarkerPattern()

  let match: RegExpExecArray | null
  while ((match = markerPattern.exec(window)) !== null) {
    const parsed = parseNumericPageMarker(match[1] ?? "")
    if (parsed != null) {
      page = parsed
    }
  }

  return page
}

function nearestFolioMarkerBefore(
  text: string,
  pos: number
): number | undefined {
  const window = text.slice(Math.max(0, pos - 500), pos)
  let page: number | undefined
  const markerPattern = folioMarkerPattern()

  let match: RegExpExecArray | null
  while ((match = markerPattern.exec(window)) !== null) {
    const parsed = parseNumericPageMarker(match[1] ?? "")
    if (parsed != null) {
      page = parsed
    }
  }

  return page
}

/**
 * Page open at `pos` when no numeric marker precedes it (e.g. a chapter that
 * opens right after roman-numeral front matter): the next numeric marker [N]
 * starts page N, so the text before it sits on page N - 1.
 */
function pageFromNextMarkerAfter(
  text: string,
  pos: number
): number | undefined {
  const markerPattern = pageMarkerPattern()
  markerPattern.lastIndex = pos
  let match: RegExpExecArray | null
  while ((match = markerPattern.exec(text)) !== null) {
    const parsed = parseNumericPageMarker(match[1] ?? "")
    if (parsed != null) {
      return parsed > 1 ? parsed - 1 : parsed
    }
  }
  return undefined
}

function pageFromNextFolioMarkerAfter(
  text: string,
  pos: number
): number | undefined {
  const markerPattern = folioMarkerPattern()
  markerPattern.lastIndex = pos
  const match = markerPattern.exec(text)
  return match?.[1]
    ? (parseNumericPageMarker(match[1]) ?? undefined)
    : undefined
}

type PageSource = NonNullable<EgwBookConfig["pageSource"]>
type ExtractedPage = {
  page: number
  text: string
  continuesFromPreviousPage: boolean
  folioPage?: number
}

type ChapterPosition = {
  chapter: number
  title: string
  anchor: string
  pos: number
  anchorLength: number
  folioPos?: number
  folioAnchorLength?: number
  tocPage?: number
  folioPage?: number
}

function assignPageParagraphNumbers(
  chapters: DraftChapter[],
  { countContinuedPages }: { countContinuedPages: boolean }
): OutputChapter[] {
  const countsByPage = new Map<number, number>()

  return chapters.map((chapter) => ({
    ...chapter,
    paragraphs: chapter.paragraphs.map((paragraph) => {
      if (paragraph.page == null) {
        throw new Error(
          `Missing printed page for ${chapter.title} paragraph ${paragraph.paragraph}`
        )
      }
      const pageParagraph = (countsByPage.get(paragraph.page) ?? 0) + 1
      countsByPage.set(paragraph.page, pageParagraph)
      if (countContinuedPages) {
        for (const continuedPage of paragraph.continued_pages ?? []) {
          countsByPage.set(
            continuedPage,
            (countsByPage.get(continuedPage) ?? 0) + 1
          )
        }
      }
      return {
        paragraph: paragraph.paragraph,
        page: paragraph.page,
        page_paragraph: pageParagraph,
        text: paragraph.text,
      }
    }),
  }))
}

async function extractPages(
  pdfPath: string,
  layout: ParagraphLayoutOptions | undefined,
  pageSource: PageSource
): Promise<ExtractedPage[]> {
  const loadingTask = getDocument(pdfPath)
  const pdf = await loadingTask.promise
  const pages: ExtractedPage[] = []

  for (let i = 1; i <= pdf.numPages; i += 1) {
    const page = await pdf.getPage(i)
    const textContent = await page.getTextContent()
    const items = textContent.items

    const reconstructed = reconstructPageParagraphs(
      items.filter(
        (item): item is typeof item & PdfTextItemLike => "str" in item
      ),
      layout
    )
    // In brackets mode a standalone number line is the PDF's own folio — a
    // different pagination from the [n] citation markers — so promoting it
    // would mix two page sequences.
    const normalizedText = normalizePageText(reconstructed.text)
    const folioPage =
      pageSource === "folios"
        ? visibleFolioFromPageText(normalizedText)
        : undefined
    const text =
      pageSource === "brackets" || pageSource === "folios"
        ? normalizedText
        : preserveStandalonePrintedPageMarker(normalizedText)

    pages.push({
      page: i,
      text,
      continuesFromPreviousPage: reconstructed.continuesFromPreviousPage,
      folioPage,
    })
  }

  return pages
}

function combineExtractedPages(
  pages: ExtractedPage[],
  pageSource: PageSource
): { rawFullText: string; rawFolioFullText: string } {
  let rawFullText = ""
  let rawFolioFullText = ""
  for (const page of pages) {
    const pageText = page.text
    const pageFolioText =
      pageSource === "folios" && page.folioPage != null
        ? `${folioMarker(page.folioPage)}\n${page.text}`
        : page.text
    if (rawFullText.length === 0) {
      rawFullText = pageText
    } else {
      rawFullText += page.continuesFromPreviousPage ? "\n" : "\n\n"
      rawFullText += pageText
    }
    if (pageSource === "folios") {
      if (rawFolioFullText.length === 0) {
        rawFolioFullText = pageFolioText
      } else {
        rawFolioFullText += page.continuesFromPreviousPage ? "\n" : "\n\n"
        rawFolioFullText += pageFolioText
      }
    }
  }

  return { rawFullText, rawFolioFullText }
}

function collectChapterPositions(
  config: EgwBookConfig,
  normalized: string,
  folioNormalized: string,
  pageSource: PageSource
): ChapterPosition[] {
  const chapterPositions: ChapterPosition[] = []
  for (const chapter of config.chapters) {
    const anchor = normalizeFullText(
      chapterAnchor(
        config.chapterAnchorTemplate,
        chapter.chapter,
        chapter.title
      )
    )
    const found = findChapterAnchor(normalized, anchor)
    if (!found) {
      throw new Error(`Missing chapter anchor: ${anchor}`)
    }
    const folioFound =
      pageSource === "folios"
        ? findChapterAnchor(folioNormalized, anchor)
        : null
    if (pageSource === "folios" && !folioFound) {
      throw new Error(`Missing folio chapter anchor: ${anchor}`)
    }
    chapterPositions.push({
      chapter: chapter.chapter,
      title: chapter.title,
      anchor,
      pos: found.pos,
      anchorLength: found.length,
      folioPos: folioFound?.pos,
      folioAnchorLength: folioFound?.length,
    })
  }
  return chapterPositions
}

function assertChapterOrder(
  chapterPositions: ChapterPosition[],
  pageSource: PageSource
): void {
  for (let i = 1; i < chapterPositions.length; i += 1) {
    if (chapterPositions[i].pos <= chapterPositions[i - 1].pos) {
      throw new Error(
        `Chapter order broken: ${chapterPositions[i].anchor} comes before ${chapterPositions[i - 1].anchor}`
      )
    }
    const currentFolioPos = chapterPositions[i].folioPos
    const previousFolioPos = chapterPositions[i - 1].folioPos
    if (
      pageSource === "folios" &&
      currentFolioPos != null &&
      previousFolioPos != null &&
      currentFolioPos <= previousFolioPos
    ) {
      throw new Error(
        `Folio chapter order broken: ${chapterPositions[i].anchor} comes before ${chapterPositions[i - 1].anchor}`
      )
    }
  }
}

function resolveMainTexts(
  config: EgwBookConfig,
  chapterPositions: ChapterPosition[],
  normalized: string,
  folioNormalized: string,
  pageSource: PageSource
): {
  mainText: string
  folioMainText: string
  tocText: string
  folioTocText: string
} {
  const lastChapter = chapterPositions[chapterPositions.length - 1]
  const firstChapter = chapterPositions[0]
  if (!lastChapter || !firstChapter) {
    throw new Error("At least one chapter is required")
  }
  const searchStart = lastChapter.pos + lastChapter.anchorLength
  const appendixIdx = config.appendixMarker
    ? normalized.indexOf(config.appendixMarker, searchStart)
    : -1

  const mainText =
    appendixIdx !== -1 ? normalized.slice(0, appendixIdx) : normalized
  const folioSearchStart =
    pageSource === "folios" && lastChapter.folioPos != null
      ? lastChapter.folioPos + (lastChapter.folioAnchorLength ?? 0)
      : searchStart
  const folioAppendixIdx =
    pageSource === "folios" && config.appendixMarker
      ? folioNormalized.indexOf(config.appendixMarker, folioSearchStart)
      : -1
  const folioMainText =
    pageSource === "folios" && folioAppendixIdx !== -1
      ? folioNormalized.slice(0, folioAppendixIdx)
      : folioNormalized

  const tocText = normalized.slice(0, firstChapter.pos)
  const folioTocText =
    pageSource === "folios" && firstChapter.folioPos != null
      ? folioNormalized.slice(0, firstChapter.folioPos)
      : tocText
  return { mainText, folioMainText, tocText, folioTocText }
}

function assignChapterStartPages(
  chapterPositions: ChapterPosition[],
  normalized: string,
  folioNormalized: string,
  tocText: string,
  folioTocText: string,
  pageSource: PageSource
): void {
  for (const chapter of chapterPositions) {
    if (pageSource === "brackets" || pageSource === "folios") {
      // TOC page numbers are the PDF's own pagination, not citation pages.
      // The chapter starts on the page of the [n] marker inside its heading
      // (a wrapped title can swallow one), else the page open at the anchor —
      // the nearest marker before it.
      const anchorSlice = normalized.slice(
        chapter.pos,
        chapter.pos + chapter.anchorLength
      )
      const markerInAnchor = pageMarkerPattern().exec(anchorSlice)
      chapter.tocPage =
        (markerInAnchor
          ? (parseNumericPageMarker(markerInAnchor[1] ?? "") ?? undefined)
          : undefined) ??
        nearestPageMarkerBefore(normalized, chapter.pos) ??
        pageFromNextMarkerAfter(normalized, chapter.pos + chapter.anchorLength)
      if (pageSource === "folios") {
        chapter.folioPage =
          parseTocPageNumber(folioTocText, chapter) ??
          nearestFolioMarkerBefore(
            folioNormalized,
            chapter.folioPos ?? chapter.pos
          ) ??
          pageFromNextFolioMarkerAfter(
            folioNormalized,
            (chapter.folioPos ?? chapter.pos) +
              (chapter.folioAnchorLength ?? chapter.anchorLength)
          )
      }
    } else {
      chapter.tocPage =
        parseTocPageNumber(tocText, chapter) ??
        nearestPageMarkerBefore(normalized, chapter.pos)
    }
  }
}

function extractDraftChapters(
  config: EgwBookConfig,
  chapterPositions: ChapterPosition[],
  mainText: string,
  folioMainText: string,
  pageSource: PageSource
): DraftChapter[] {
  const chapters: DraftChapter[] = []
  for (let i = 0; i < chapterPositions.length; i += 1) {
    const current = chapterPositions[i]
    const next =
      i + 1 < chapterPositions.length ? chapterPositions[i + 1] : null

    const start = current.pos + current.anchorLength
    const end = next ? next.pos : mainText.length
    if (end <= start) {
      throw new Error(`Invalid text range for chapter ${current.chapter}`)
    }

    const rawSlice = mainText.slice(start, end)
    const cleaned = stripChapterFurniture(rawSlice, config.title, current.title)
    const outputCleaned =
      pageSource === "folios" && current.folioPos != null
        ? stripChapterFurniture(
            folioMainText.slice(
              current.folioPos + (current.folioAnchorLength ?? 0),
              next?.folioPos ?? folioMainText.length
            ),
            config.title,
            current.title
          )
        : cleaned
    const paragraphTexts = splitParagraphsWithPages(cleaned, current.tocPage)
    const outputParagraphTexts =
      pageSource === "folios"
        ? alignFolioParagraphs(
            current.chapter,
            paragraphTexts,
            splitParagraphsWithPages(
              outputCleaned,
              current.folioPage,
              "folio-markers"
            )
          )
        : paragraphTexts

    if (paragraphTexts.length === 0) {
      throw new Error(`No paragraphs extracted for chapter ${current.chapter}`)
    }

    for (
      let paragraphIndex = 0;
      paragraphIndex < paragraphTexts.length;
      paragraphIndex += 1
    ) {
      if (!paragraphTexts[paragraphIndex]) {
        throw new Error(
          `Empty paragraph extracted in chapter ${current.chapter} at ${paragraphIndex + 1}`
        )
      }
    }

    chapters.push({
      chapter: current.chapter,
      title: current.title,
      paragraphs: cleanEgwParagraphs(
        paragraphTexts.map((paragraph, index) => ({
          paragraph: index + 1,
          page: paragraph.page,
          continued_pages: paragraph.continued_pages,
          output_page: outputParagraphTexts[index]?.page,
          output_continued_pages: outputParagraphTexts[index]?.continued_pages,
          text: paragraph.text,
        })),
        {
          bookTitle: config.title,
          chapterTitle: current.title,
          splitReadableParagraphs: config.splitReadableParagraphs,
        }
      ),
    })
  }
  return chapters
}

function assertOutputIntegrity(
  outputChapters: OutputChapter[],
  expectedChapterCount: number
): void {
  if (outputChapters.length !== expectedChapterCount) {
    throw new Error(
      `Expected ${expectedChapterCount} chapters, got ${outputChapters.length}`
    )
  }

  for (let i = 0; i < outputChapters.length; i += 1) {
    const chapter = outputChapters[i]
    if (chapter.chapter !== i + 1) {
      throw new Error(`Chapter sequence broken at ${i + 1}`)
    }
    for (let j = 0; j < chapter.paragraphs.length; j += 1) {
      if (chapter.paragraphs[j].paragraph !== j + 1) {
        throw new Error(
          `Paragraph sequence broken in chapter ${chapter.chapter} at ${j + 1}`
        )
      }
    }
  }
}

export async function importEgwPdf(config: EgwBookConfig): Promise<void> {
  if (!existsSync(config.pdfPath)) {
    throw new Error(`Source PDF not found: ${config.pdfPath}`)
  }

  const debugDir = join(repoRoot(), "tmp", "egw", config.debugSlug)
  const debugPagesJson = join(debugDir, "pages.json")
  const debugTextTxt = join(debugDir, "extracted.txt")
  mkdirSync(debugDir, { recursive: true })
  mkdirSync(dirname(config.outputJsonPath), { recursive: true })

  const pageSource = config.pageSource ?? "legacy"
  const pages = await extractPages(config.pdfPath, config.layout, pageSource)
  writeFileSync(debugPagesJson, `${JSON.stringify(pages, null, 2)}\n`)

  const { rawFullText, rawFolioFullText } = combineExtractedPages(
    pages,
    pageSource
  )
  writeFileSync(
    debugTextTxt,
    pageSource === "folios" ? rawFolioFullText : rawFullText
  )
  ensureUsableText(rawFullText, config.requiredTokens)

  const normalized = normalizeFullText(rawFullText)
  const folioNormalized =
    pageSource === "folios" ? normalizeFullText(rawFolioFullText) : normalized
  const chapterPositions = collectChapterPositions(
    config,
    normalized,
    folioNormalized,
    pageSource
  )
  assertChapterOrder(chapterPositions, pageSource)

  const { mainText, folioMainText, tocText, folioTocText } = resolveMainTexts(
    config,
    chapterPositions,
    normalized,
    folioNormalized,
    pageSource
  )
  assignChapterStartPages(
    chapterPositions,
    normalized,
    folioNormalized,
    tocText,
    folioTocText,
    pageSource
  )
  const chapters = extractDraftChapters(
    config,
    chapterPositions,
    mainText,
    folioMainText,
    pageSource
  )
  const processedChapters = config.postprocessChapters?.(chapters) ?? chapters
  const outputChapters = assignPageParagraphNumbers(processedChapters, {
    countContinuedPages: config.countContinuedPagesForPageParagraphs ?? true,
  })
  assertOutputIntegrity(outputChapters, config.expectedChapterCount)

  const output = {
    title: config.title,
    abbreviation: config.abbreviation,
    book_number: config.book_number,
    chapters: outputChapters,
  }

  writeFileSync(config.outputJsonPath, `${JSON.stringify(output, null, 2)}\n`)

  const totalParagraphs = outputChapters.reduce(
    (sum, chapter) => sum + chapter.paragraphs.length,
    0
  )
  console.log(
    `Imported ${config.title}: ${outputChapters.length} chapters, ${totalParagraphs} paragraphs.`
  )
  console.log(`Wrote JSON to ${config.outputJsonPath}`)
}
