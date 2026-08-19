import { ESLint } from 'eslint'
import { describe, expect, it } from 'vitest'

const V10_RECOMMENDED_ADDITIONS = [
  'no-unassigned-vars',
  'no-useless-assignment',
  'preserve-caught-error',
] as const

// Resolving the flat config takes ~8s on its own, which leaves almost no room
// under the 10s global testTimeout — the test timed out intermittently when the
// full suite ran its files in parallel. Give this one case its own budget.
const CONFIG_RESOLVE_TIMEOUT_MS = 60_000

describe('ESLint migration configuration', () => {
  it(
    'preserves the v9 recommended baseline for rules added in v10',
    async () => {
      const eslint = new ESLint()
      const config = await eslint.calculateConfigForFile('src/main.tsx')

      expect(config).not.toBeUndefined()
      for (const rule of V10_RECOMMENDED_ADDITIONS) {
        expect(config?.rules?.[rule]?.[0], rule).toBe(0)
      }
    },
    CONFIG_RESOLVE_TIMEOUT_MS
  )
})
