import { describe, expect, test } from "bun:test"

import { buildEmbeddingEntries } from "./compute-embeddings"

describe("buildEmbeddingEntries", () => {
  const verse = {
    id: 316,
    book_name: "John",
    chapter: 3,
    verse: 16,
    text: "For God so loved the world.",
  }

  test("emits one independent vector per available translation", () => {
    const entries = buildEmbeddingEntries(
      verse,
      new Map([
        ["KJV", "For God so loved the world."],
        ["WEB", "For God so loved the world, that he gave his only Son."],
        ["SpaRV", "Porque de tal manera amó Dios al mundo."],
        ["FreJND", "Car Dieu a tant aimé le monde."],
        ["PorBLivre", "Porque Deus amou o mundo de tal maneira."],
      ]),
    )

    expect(entries).toHaveLength(5)
    expect(entries.map((entry) => entry.id)).toEqual([316, 316, 316, 316, 316])
    expect(entries.map((entry) => entry.text)).toEqual([
      "For God so loved the world.",
      "For God so loved the world, that he gave his only Son.",
      "Porque de tal manera amó Dios al mundo.",
      "Car Dieu a tant aimé le monde.",
      "Porque Deus amou o mundo de tal maneira.",
    ])
  })

  test("falls back to the canonical verse when KJV text is absent", () => {
    const entries = buildEmbeddingEntries(verse, new Map([["WEB", "Modern wording."]]))

    expect(entries[0]).toEqual({
      id: 316,
      text: verse.text,
      ref: "John 3:16",
    })
    expect(entries[1]).toEqual({
      id: 316,
      text: "Modern wording.",
      ref: "John 3:16",
    })
  })
})
