import { SDA_HYMNAL_INDEX } from "@/data/sda-hymnal-index"
import { selectPreviewItem } from "@/lib/presentation-workflow"
import { generateHymnScreens } from "@/services/hymnal/generate-hymn-screens"
import {
  createHymnPresentationItem,
  defaultSelectedSectionIds,
} from "@/services/hymnal/hymn-presentation"
import { addRecentHymn } from "@/services/hymnal/hymnal-history"
import { getHymnByNumber } from "@/services/hymnal/hymnal-repository"
import { useHymnSlideStore } from "@/stores/hymn-slide-store"

const VALID_HYMN_NUMBERS: Set<number> = new Set(SDA_HYMNAL_INDEX.map((hymn) => hymn.number))
const DEDUPE_WINDOW_MS = 5000

const HYMN_CUE_PATTERN =
  /\b(?:sda\s+(?:hymn|song)|(?:hymn|song))(?:\s+number)?\s+([a-z0-9][a-z0-9\s-]*)/i

const ONES: Record<string, number> = {
  zero: 0,
  one: 1,
  two: 2,
  three: 3,
  four: 4,
  five: 5,
  six: 6,
  seven: 7,
  eight: 8,
  nine: 9,
  ten: 10,
  eleven: 11,
  twelve: 12,
  thirteen: 13,
  fourteen: 14,
  fifteen: 15,
  sixteen: 16,
  seventeen: 17,
  eighteen: 18,
  nineteen: 19,
}

const TENS: Record<string, number> = {
  twenty: 20,
  thirty: 30,
  forty: 40,
  fifty: 50,
  sixty: 60,
  seventy: 70,
  eighty: 80,
  ninety: 90,
}

let lastHandled: { hymnNumber: number; at: number } | null = null

function normalizeTranscript(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
}

function isValidHymnNumber(number: number): boolean {
  return Number.isInteger(number) && number > 0 && VALID_HYMN_NUMBERS.has(number)
}

function parseSpokenNumber(words: string[]): number | null {
  let total = 0
  let current = 0

  for (const word of words) {
    if (word === "and") continue

    if (word === "hundred") {
      current = (current === 0 ? 1 : current) * 100
      continue
    }

    if (word in TENS) {
      current += TENS[word]
      continue
    }

    if (word in ONES) {
      current += ONES[word]
      continue
    }

    return null
  }

  total += current
  return total > 0 ? total : null
}

function parseNumberPhrase(phrase: string): number | null {
  const trimmed = phrase.trim().toLowerCase()
  if (!trimmed) return null

  if (/^\d+$/.test(trimmed)) {
    const digits = Number.parseInt(trimmed, 10)
    return Number.isFinite(digits) && digits > 0 ? digits : null
  }

  const words = trimmed.replace(/-/g, " ").split(/\s+/).filter(Boolean)
  return parseSpokenNumber(words)
}

export function parseHymnCommand(text: string): number | null {
  const normalized = normalizeTranscript(text)
  if (!normalized) return null

  const match = normalized.match(HYMN_CUE_PATTERN)
  if (!match) return null

  const numberPhrase = match[1].split(/[,.!?;]/)[0]?.trim() ?? ""
  const number = parseNumberPhrase(numberPhrase)
  if (number === null || !isValidHymnNumber(number)) return null

  return number
}

export function shouldSuppressDuplicateHymnCommand(hymnNumber: number, now = Date.now()): boolean {
  if (!lastHandled) return false
  if (now - lastHandled.at > DEDUPE_WINDOW_MS) return false
  return lastHandled.hymnNumber === hymnNumber
}

export function resetHymnVoiceControlState(): void {
  lastHandled = null
}

export async function handleHymnVoiceControl(text: string): Promise<boolean> {
  const hymnNumber = parseHymnCommand(text)
  if (hymnNumber === null) return false

  if (shouldSuppressDuplicateHymnCommand(hymnNumber)) return false

  const hymn = await getHymnByNumber(hymnNumber)
  if (!hymn) return false

  const screens = generateHymnScreens({
    hymn,
    selectedSectionIds: defaultSelectedSectionIds(hymn),
    maxLinesPerScreen: 4,
  })
  const firstScreen = screens[0]
  if (!firstScreen) return false

  const deck = screens.map((screen) => createHymnPresentationItem(screen))
  useHymnSlideStore.getState().setDeck(deck, 0)
  selectPreviewItem(deck[0])
  addRecentHymn(hymn.id)
  lastHandled = { hymnNumber, at: Date.now() }
  return true
}
