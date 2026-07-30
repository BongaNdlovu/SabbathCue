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
  nul: 0,
  een: 1,
  twee: 2,
  drie: 3,
  vier: 4,
  vyf: 5,
  ses: 6,
  sewe: 7,
  agt: 8,
  nege: 9,
  tien: 10,
  elf: 11,
  twaalf: 12,
  dertien: 13,
  veertien: 14,
  vyftien: 15,
  sestien: 16,
  sewentien: 17,
  agtien: 18,
  negentien: 19,
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
  twintig: 20,
  dertig: 30,
  veertig: 40,
  vyftig: 50,
  sestig: 60,
  sewentig: 70,
  tagtig: 80,
  negentig: 90,
}

function parseUnderHundred(words: string[]): number | null {
  if (words.length === 1) {
    return ONES[words[0]] ?? TENS[words[0]] ?? null
  }

  if (words.length === 2 && words[0] in TENS && words[1] in ONES) {
    return TENS[words[0]] + ONES[words[1]]
  }

  if (
    words.length === 3 &&
    words[0] in ONES &&
    (words[1] === "and" || words[1] === "en") &&
    words[2] in TENS
  ) {
    return TENS[words[2]] + ONES[words[0]]
  }

  return null
}

export function parsePositiveSpokenNumber(value: string): number | null {
  const normalized = value.trim().toLowerCase()
  if (!normalized) return null

  if (/^\d+$/.test(normalized)) {
    const number = Number.parseInt(normalized, 10)
    return Number.isSafeInteger(number) && number > 0 ? number : null
  }

  const words = normalized.replace(/-/g, " ").split(/\s+/).filter(Boolean)
  const hundredIndex = words.findIndex(
    (word) => word === "hundred" || word === "honderd"
  )

  if (hundredIndex === -1) {
    const number = parseUnderHundred(words)
    return number !== null && number > 0 ? number : null
  }

  if (hundredIndex > 1) return null
  const multiplier =
    hundredIndex === 0 ? 1 : (ONES[words[0]] ?? Number.NaN)
  if (!Number.isInteger(multiplier) || multiplier <= 0 || multiplier > 9) {
    return null
  }

  const remainderWords = words
    .slice(hundredIndex + 1)
    .filter((word, index) => index !== 0 || (word !== "and" && word !== "en"))
  if (remainderWords.some((word) => word === "hundred" || word === "honderd")) {
    return null
  }

  const remainder =
    remainderWords.length === 0 ? 0 : parseUnderHundred(remainderWords)
  if (remainder === null) return null

  return multiplier * 100 + remainder
}
