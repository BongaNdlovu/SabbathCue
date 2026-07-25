import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores([
    'build',
    'coverage',
    'dist',
    'node_modules',
    'src-tauri/target',
    'test-results',
    'web',
  ]),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      // Preserve the v9 recommended baseline during the dependency migration.
      'no-useless-assignment': 'off',
      'react-refresh/only-export-components': 'off',
      // Surface high-cyclomatic-complexity functions as warnings (non-blocking).
      // 2026-06-18 baseline: max 58, 11 functions > 20. Ratchet the threshold
      // down as the R4/R13 refactors land. See docs/reports/CODE_REFACTORING_PLAN.md.
      complexity: ['warn', 20],
    },
  },
])
