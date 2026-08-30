import { defineConfig } from 'eslint/config';
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import lit from 'eslint-plugin-lit';

export default defineConfig(
  { ignores: ['dist/**', 'node_modules/**', 'coverage/**', 'placeholder/**'] },
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  lit.configs['flat/recommended'],
  {
    languageOptions: { parserOptions: { projectService: { allowDefaultProject: ['eslint.config.js'] }, tsconfigRootDir: import.meta.dirname } },
    rules: {
      // effect-fp-skill hard rules, mechanical.
      'no-restricted-syntax': [
        'error',
        { selector: 'ThrowStatement', message: 'Return a tagged error via the Effect error channel' },
        { selector: 'TryStatement', message: 'Use Effect.try / Effect.tryPromise at the interop edge (src/core/interop.ts)' },
        { selector: 'CallExpression[callee.object.name="console"]', message: 'Use Effect.log*' },
      ],
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-non-null-assertion': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
    },
  },
  { files: ['eslint.config.js'], rules: { '@typescript-eslint/no-unsafe-assignment': 'off' } },
  {
    // The interop edge and the Lit lifecycle are allowed to touch promises/async.
    files: ['src/core/interop.ts', 'src/core/runtime.ts', 'src/**/*.test.ts', 'e2e/**'],
    rules: { 'no-restricted-syntax': 'off' },
  },
);
