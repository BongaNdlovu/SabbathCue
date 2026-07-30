import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, test } from "bun:test"
import { alignStepsToChristCanonicalParagraphs } from "./convert-egw-sc-pdf"

type EgwSource = {
  title: string
  abbreviation: string
  chapters: Array<{
    chapter: number
    title: string
    paragraphs: Array<{
      paragraph: number
      page: number
      page_paragraph: number
      text: string
    }>
  }>
}

function loadStepsToChrist(): EgwSource {
  return JSON.parse(
    readFileSync(
      join(import.meta.dir, "sources", "egw", "steps-to-christ.json"),
      "utf-8"
    )
  ) as EgwSource
}

function requireChapter(source: EgwSource, chapterNumber: number) {
  const chapter = source.chapters.find(
    (entry) => entry.chapter === chapterNumber
  )
  if (!chapter) throw new Error(`Missing chapter ${chapterNumber}`)
  return chapter
}

describe("Steps to Christ source", () => {
  test("chapters follow the canonical EGW Writings SC paragraph labels", () => {
    // Sources: https://m.egwwritings.org/en/book/108.21#21 and following
    // chapter links. The visible SC labels are the canonical boundaries.
    const expectedLabels: Record<number, string[]> = {
      1: [
        "9.1",
        "9.2",
        "9.3",
        "10.1",
        "10.2",
        "10.3",
        "11.1",
        "11.2",
        "12.1",
        "12.2",
        "13.1",
        "13.2",
        "14.1",
        "14.2",
        "14.3",
        "15.1",
        "15.2",
      ],
      2: [
        "17.1",
        "17.2",
        "18.1",
        "18.2",
        "19.1",
        "19.2",
        "20.1",
        "21.1",
        "21.2",
        "21.3",
        "21.4",
        "22.1",
      ],
      3: [
        "23.1",
        "23.2",
        "23.3",
        "24.1",
        "24.2",
        "24.3",
        "25.1",
        "25.2",
        "25.3",
        "26.1",
        "26.2",
        "26.3",
        "26.4",
        "27.1",
        "27.2",
        "28.1",
        "28.2",
        "28.3",
        "29.1",
        "29.2",
        "29.3",
        "30.1",
        "30.2",
        "31.1",
        "31.2",
        "32.1",
        "32.2",
        "33.1",
        "33.2",
        "33.3",
        "34.1",
        "34.2",
        "34.3",
        "35.1",
        "35.2",
        "35.3",
        "35.4",
      ],
      4: [
        "37.1",
        "37.2",
        "37.3",
        "37.4",
        "38.1",
        "38.2",
        "39.1",
        "40.1",
        "40.2",
        "41.1",
        "41.2",
      ],
      5: [
        "43.1",
        "43.2",
        "43.3",
        "43.4",
        "44.1",
        "44.2",
        "45.1",
        "45.2",
        "46.1",
        "46.2",
        "46.3",
        "47.1",
        "47.2",
        "48.1",
      ],
      6: [
        "49.1",
        "49.2",
        "49.3",
        "50.1",
        "51.1",
        "51.2",
        "51.3",
        "51.4",
        "52.1",
        "52.2",
        "52.3",
        "53.1",
        "53.2",
        "54.1",
        "54.2",
        "54.3",
        "55.1",
      ],
      7: [
        "57.1",
        "57.2",
        "58.1",
        "58.2",
        "58.3",
        "59.1",
        "59.2",
        "59.3",
        "59.4",
        "60.1",
        "60.2",
        "61.1",
        "61.2",
        "62.1",
        "62.2",
        "62.3",
        "63.1",
        "63.2",
        "64.1",
        "64.2",
        "65.1",
        "65.2",
      ],
      8: [
        "67.1",
        "67.2",
        "67.3",
        "68.1",
        "68.2",
        "68.3",
        "69.1",
        "69.2",
        "70.1",
        "70.2",
        "71.1",
        "71.2",
        "72.1",
        "72.2",
        "73.1",
        "73.2",
        "74.1",
        "75.1",
        "75.2",
      ],
      9: [
        "77.1",
        "77.2",
        "77.3",
        "77.4",
        "78.1",
        "78.2",
        "79.1",
        "79.2",
        "79.3",
        "80.1",
        "80.2",
        "80.3",
        "81.1",
        "81.2",
        "81.3",
        "81.4",
        "82.1",
        "82.2",
        "82.3",
        "83.1",
        "83.2",
      ],
      10: [
        "85.1",
        "85.2",
        "85.3",
        "85.4",
        "86.1",
        "86.2",
        "87.1",
        "87.2",
        "87.3",
        "88.1",
        "88.2",
        "88.3",
        "89.1",
        "89.2",
        "89.3",
        "90.1",
        "90.2",
        "90.3",
        "91.1",
        "91.2",
      ],
      11: [
        "93.1",
        "93.2",
        "93.3",
        "93.4",
        "94.1",
        "94.2",
        "95.1",
        "95.2",
        "95.3",
        "96.1",
        "96.2",
        "96.3",
        "97.1",
        "97.2",
        "98.1",
        "98.2",
        "98.3",
        "99.1",
        "99.2",
        "99.3",
        "99.4",
        "100.1",
        "100.2",
        "101.1",
        "101.2",
        "101.3",
        "102.1",
        "102.2",
        "103.1",
        "103.2",
        "103.3",
        "104.1",
      ],
      12: [
        "105.1",
        "105.2",
        "105.3",
        "106.1",
        "106.2",
        "107.1",
        "107.2",
        "108.1",
        "108.2",
        "109.1",
        "109.2",
        "109.3",
        "110.1",
        "111.1",
        "111.2",
        "111.3",
        "112.1",
        "112.2",
        "112.3",
      ],
      13: [
        "115.1",
        "115.2",
        "116.1",
        "116.2",
        "116.3",
        "117.1",
        "117.2",
        "117.3",
        "118.1",
        "118.2",
        "119.1",
        "119.2",
        "119.3",
        "120.1",
        "120.2",
        "120.3",
        "120.4",
        "121.1",
        "121.2",
        "121.3",
        "122.1",
        "122.2",
        "122.3",
        "123.1",
        "123.2",
        "124.1",
        "124.2",
        "125.1",
        "125.2",
        "125.3",
        "126.1",
        "126.2",
      ],
    }

    const source = loadStepsToChrist()

    for (const chapter of source.chapters) {
      expect(
        chapter.paragraphs.map((p) => `${p.page}.${p.page_paragraph}`)
      ).toEqual(expectedLabels[chapter.chapter])
    }

    expect(
      source.chapters.reduce(
        (count, chapter) => count + chapter.paragraphs.length,
        0
      )
    ).toBe(273)
  })

  test("keeps poetry blocks on the EGW Writings paragraph boundaries", () => {
    const source = loadStepsToChrist()
    const chapter1 = requireChapter(source, 1)
    const chapter3 = requireChapter(source, 3)

    expect(chapter1.paragraphs).toHaveLength(17)
    expect(chapter1.paragraphs[1].text).toContain(
      "The eyes of all wait upon Thee"
    )
    expect(chapter1.paragraphs[1].text).toContain("Psalm 145:15, 16")
    expect(chapter1.paragraphs[3].text).toMatch(/^"God is love"/)

    expect(chapter3.paragraphs[5].text).toContain(
      "This was the language of his soul:"
    )
    expect(chapter3.paragraphs[6].page).toBe(25)
    expect(chapter3.paragraphs[6].page_paragraph).toBe(1)
    expect(chapter3.paragraphs[6].text).toContain("Psalm 32:1, 2")
    expect(chapter3.paragraphs[7].page_paragraph).toBe(2)
    expect(chapter3.paragraphs[7].text).toContain("Psalm 51:1-14")
    expect(chapter3.paragraphs[8].page_paragraph).toBe(3)
    expect(chapter3.paragraphs[8].text).toMatch(/^A repentance such as this/)
  })

  test("does not keep continued page metadata on the isolated chapter 3 prose fragment", () => {
    const [chapter] = alignStepsToChristCanonicalParagraphs([
      {
        chapter: 3,
        title: "Repentance",
        paragraphs: [
          {
            paragraph: 6,
            page: 24,
            continued_pages: [25],
            text: [
              "The prayer of David after his fall, illustrates the nature of true sorrow for sin. This was the language of his soul:",
              '"Blessed is he whose transgression is forgiven, whose sin is covered." Psalm 32:1, 2.',
              '"Have mercy upon me, O God, according to Thy loving-kindness." Psalm 51:1-14.',
              "A repentance such as this, is beyond the reach of our own power to accomplish.",
            ].join(" "),
          },
        ],
      },
    ])

    expect(chapter?.paragraphs[0]).toEqual({
      paragraph: 1,
      page: 24,
      text: "The prayer of David after his fall, illustrates the nature of true sorrow for sin. This was the language of his soul:",
    })
    expect(chapter?.paragraphs[0]).not.toHaveProperty("continued_pages")
    expect(chapter?.paragraphs[1]?.page).toBe(25)
    expect(chapter?.paragraphs[2]?.page).toBe(25)
  })

  test("tracks the pages of merged chapter 1 psalm block fragments", () => {
    const [chapter] = alignStepsToChristCanonicalParagraphs([
      {
        chapter: 1,
        title: "God's Love for Man",
        paragraphs: [
          {
            paragraph: 1,
            page: 9,
            text: '"The eyes of all wait upon Thee; and Thou givest them their meat in due season.',
          },
          {
            paragraph: 2,
            page: 9,
            text: 'Thou openest Thine hand, and satisfiest the desire of every living thing."',
          },
          {
            paragraph: 3,
            page: 10,
            text: "Psalm 145:15, 16.",
          },
        ],
      },
    ])

    expect(chapter?.paragraphs).toHaveLength(1)
    expect(chapter?.paragraphs[0]?.page).toBe(9)
    expect(chapter?.paragraphs[0]?.continued_pages).toEqual([10])
  })
})
