import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';

export default tseslint.config(
  { ignores: ['dist'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  // The top-level `recommended-latest` is still the legacy eslintrc shape;
  // the flat-config variant lives under `configs.flat`.
  reactHooks.configs.flat['recommended-latest'],
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    rules: {
      // docs/conventions.md: no `any`. Use `unknown` with an explicit check.
      '@typescript-eslint/no-explicit-any': 'error',
    },
  },
);
