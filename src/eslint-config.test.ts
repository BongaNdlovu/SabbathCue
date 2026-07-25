import { ESLint } from 'eslint'
import { describe, expect, it } from 'vitest'

const V10_RECOMMENDED_ADDITIONS = [
  'no-unassigned-vars',
  'no-useless-assignment',
  'preserve-caught-error',
] as const

describe('ESLint migration configuration', () => {
  it('preserves the v9 recommended baseline for rules added in v10', async () => {
    const eslint = new ESLint()
    const config = await eslint.calculateConfigForFile('src/main.tsx')

    expect(config).not.toBeUndefined()
    for (const rule of V10_RECOMMENDED_ADDITIONS) {
      expect(config?.rules?.[rule]?.[0], rule).toBe(0)
    }
  })
})
