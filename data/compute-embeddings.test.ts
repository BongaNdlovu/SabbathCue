import { describe, expect, test } from "bun:test"

import {
  buildEmbeddingCorpusManifest,
  buildEmbeddingEntries,
} from "./compute-embeddings"

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

describe("buildEmbeddingCorpusManifest", () => {
  test("records split composition and counts", () => {
    const manifest = buildEmbeddingCorpusManifest({
      recordCount: 155345,
      uniqueVerseIds: 31102,
      generatedAt: "2026-08-03T00:00:00.000Z",
    })

    expect(manifest).toEqual({
      schema_version: 1,
      blended_translations: ["KJV"],
      separate_translations: ["WEB", "SpaRV", "FreJND", "PorBLivre"],
      record_count: 155345,
      unique_verse_ids: 31102,
      model_family: "minilm-l6-v2",
      padding: "batch_longest",
      max_tokens: 128,
      generated_at: "2026-08-03T00:00:00.000Z",
    })
  })
})
