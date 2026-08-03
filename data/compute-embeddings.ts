/// <reference types="bun-types" />
/**
 * Prepares canonical verse text for semantic embeddings.
 *
 * The semantic index stores vectors keyed by the KJV verse row id. KJV remains
 * the primary English vector; each public-domain translation is exported as a
 * separate vector for the same reference so language-specific wording can
 * match without cross-language truncation.
 *
 * Usage:
 * 1. Run: bun run data/download-model.ts
 * 2. Run: bun run data/compute-embeddings.ts
 * 3. Run: cargo run -p rhema-detection --features onnx,vector-search --bin precompute -- \
 *        --model models/minilm-l6-v2-int8/onnx/model_quantized.onnx \
 *        --tokenizer models/minilm-l6-v2/tokenizer.json \
 *        --verses data/verses-for-embedding.json \
 *        --output-embeddings embeddings/public-minilm-l6-v2.bin \
 *        --output-ids embeddings/public-minilm-l6-v2-ids.bin
 */

import { Database } from "bun:sqlite"
import { mkdir } from "node:fs/promises"
import { join } from "node:path"

const DATA_DIR = import.meta.dir
const DB_PATH = join(DATA_DIR, "rhema.db")
const OUTPUT_PATH = join(DATA_DIR, "verses-for-embedding.json")
const BLENDED_TRANSLATIONS = ["KJV"] as const
const SEPARATE_VECTOR_TRANSLATIONS = [
  "WEB",
  "SpaRV",
  "FreJND",
  "PorBLivre",
] as const
const EMBEDDING_TRANSLATIONS = [
  ...BLENDED_TRANSLATIONS,
  ...SEPARATE_VECTOR_TRANSLATIONS,
] as const

type TranslationRow = {
  id: number
  abbreviation: string
}

type VerseRow = {
  id: number
  translation_id: number
  book_number: number
  book_name: string
  chapter: number
  verse: number
  text: string
}

export type EmbeddingEntry = {
  id: number
  text: string
  ref: string
}

function verseKey(bookNumber: number, chapter: number, verse: number): string {
  return `${bookNumber}:${chapter}:${verse}`
}

export function buildEmbeddingEntries(
  verse: Pick<VerseRow, "id" | "book_name" | "chapter" | "verse" | "text">,
  texts: ReadonlyMap<string, string> | undefined,
): EmbeddingEntry[] {
  const ref = `${verse.book_name} ${verse.chapter}:${verse.verse}`
  const primaryText =
    texts?.get(BLENDED_TRANSLATIONS[0])?.trim() || verse.text
  const entries: EmbeddingEntry[] = [{
    id: verse.id,
    text: primaryText,
    ref,
  }]

  for (const translation of SEPARATE_VECTOR_TRANSLATIONS) {
    const text = texts?.get(translation)?.trim()
    if (text) entries.push({ id: verse.id, text, ref })
  }

  return entries
}

async function main() {
  await mkdir(join(DATA_DIR, "..", "embeddings"), { recursive: true })

  console.log("\nExporting redistributable verses for embedding...\n")

  const db = new Database(DB_PATH, { readonly: true })

  const placeholders = EMBEDDING_TRANSLATIONS.map(() => "?").join(", ")
  const translationRows = db
    .query(
      `SELECT id, abbreviation FROM translations WHERE abbreviation IN (${placeholders})`,
    )
    .all(...EMBEDDING_TRANSLATIONS) as TranslationRow[]

  const translations = EMBEDDING_TRANSLATIONS.map((abbreviation) =>
    translationRows.find((row) => row.abbreviation === abbreviation),
  ).filter((row): row is TranslationRow => Boolean(row))

  const kjvRow = translations.find((row) => row.abbreviation === "KJV")
  if (!kjvRow) {
    throw new Error(
      "KJV translation not found in rhema.db - run build:bible first",
    )
  }

  const missing = EMBEDDING_TRANSLATIONS.filter(
    (abbreviation) =>
      !translations.some((row) => row.abbreviation === abbreviation),
  )
  if (missing.length > 0) {
    console.log(`  Missing optional translations: ${missing.join(", ")}`)
  }
  console.log(
    `  Using translations: ${translations
      .map((row) => row.abbreviation)
      .join(", ")}`,
  )

  const kjvVerses = db
    .query(
      "SELECT id, translation_id, book_number, book_name, chapter, verse, text FROM verses WHERE translation_id = ? ORDER BY book_number, chapter, verse",
    )
    .all(kjvRow.id) as VerseRow[]

  const translationIds = translations.map((row) => row.id)
  const idPlaceholders = translationIds.map(() => "?").join(", ")
  const translationNamesById = new Map(
    translations.map((row) => [row.id, row.abbreviation]),
  )
  const verseRows = db
    .query(
      `SELECT id, translation_id, book_number, book_name, chapter, verse, text FROM verses WHERE translation_id IN (${idPlaceholders}) ORDER BY translation_id, book_number, chapter, verse`,
    )
    .all(...translationIds) as VerseRow[]

  const textByReference = new Map<string, Map<string, string>>()
  for (const row of verseRows) {
    const abbreviation = translationNamesById.get(row.translation_id)
    if (!abbreviation) continue

    const key = verseKey(row.book_number, row.chapter, row.verse)
    const texts = textByReference.get(key) ?? new Map<string, string>()
    texts.set(abbreviation, row.text)
    textByReference.set(key, texts)
  }

  console.log(`  Found ${kjvVerses.length} canonical KJV verse references`)

  const exportedCountByTranslation = new Map<string, number>([
    ["KJV", 0],
    ...SEPARATE_VECTOR_TRANSLATIONS.map((translation) => [translation, 0] as const),
  ])
  const output = kjvVerses.flatMap((verse) => {
    const texts = textByReference.get(
      verseKey(verse.book_number, verse.chapter, verse.verse),
    )
    exportedCountByTranslation.set(
      "KJV",
      (exportedCountByTranslation.get("KJV") ?? 0) + 1,
    )
    for (const translation of SEPARATE_VECTOR_TRANSLATIONS) {
      if (texts?.get(translation)?.trim()) {
        exportedCountByTranslation.set(
          translation,
          (exportedCountByTranslation.get(translation) ?? 0) + 1,
        )
      }
    }
    return buildEmbeddingEntries(verse, texts)
  })

  await Bun.write(OUTPUT_PATH, JSON.stringify(output))
  console.log(`  Exported ${output.length} embedding records`)
  console.log(
    `  Records by translation: ${Array.from(exportedCountByTranslation)
      .map(([translation, count]) => `${translation}=${count}`)
      .join(", ")}`,
  )
  console.log(`  Exported to ${OUTPUT_PATH}`)
  console.log(
    "\n  Next: run the embedding precompute step to generate the binary index.\n",
  )

  db.close()
}

if (import.meta.main) {
  main().catch((err) => {
    console.error("Export failed:", err)
    process.exit(1)
  })
}
