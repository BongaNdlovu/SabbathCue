import { describe, expect, it } from "vitest"
import { parsePositiveSpokenNumber } from "./spoken-number"

describe("parsePositiveSpokenNumber", () => {
  it.each([
    ["1", 1],
    ["twelve", 12],
    ["twenty one", 21],
    ["twenty-one", 21],
    ["two hundred fifty one", 251],
    ["one hundred and one", 101],
    ["twaalf", 12],
    ["drie en twintig", 23],
    ["een honderd", 100],
  ])("parses %s", (value, expected) => {
    expect(parsePositiveSpokenNumber(value)).toBe(expected)
  })

  it.each(["", "0", "zero", "-1", "1.5", "one two", "one hundred one two"])(
    "rejects %s",
    (value) => {
      expect(parsePositiveSpokenNumber(value)).toBeNull()
    }
  )
})
