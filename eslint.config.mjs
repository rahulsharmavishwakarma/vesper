import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import prettier from 'eslint-config-prettier';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';
import svelteConfig from './apps/desktop/svelte.config.js';

export default defineConfig(
  { ignores: ['**/node_modules/**', '**/dist/**', 'apps/desktop/src-tauri/**'] },
  js.configs.recommended,
  ts.configs.recommended,
  svelte.configs.recommended,
  {
    rules: {
      // Framework callbacks may require a parameter whose value is unused.
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
    },
  },
  { languageOptions: { globals: globals.node } },
  {
    files: ['apps/desktop/src/**/*.{ts,svelte}', 'apps/desktop/tests/**/*.{mjs,svelte}'],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { parser: ts.parser, svelteConfig } },
    // TypeScript/svelte-check resolves DOM types inside TypeScript script blocks.
    rules: { 'no-undef': 'off' },
  },
  prettier,
  svelte.configs.prettier,
);
