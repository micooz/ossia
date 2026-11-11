// Vite+ owns workspace formatting, lint and development checks.
import { defineConfig } from 'vite-plus';

export default defineConfig({
  test: {
    // Rust tests run through cargo; the P00 TypeScript comparison probes were retired in P07.
    exclude: ['**/node_modules/**', '**/.next/**', '**/target/**', 'work/**'],
  },
  staged: {
    '*': 'vp check --fix',
  },
  fmt: {
    singleQuote: true,
    semi: true,
    sortImports: {},
    // Not ours to reformat: derived trees (vendored engine, dist/.next/target),
    // `.docs` (hand-maintained Chinese specs whose markdown tables would churn), and `work/`
    // (pre-MVP spike scripts kept as evidence).
    ignorePatterns: [
      '.docs/',
      'packages/ui/src/generated/',
      'tests/fixtures/',
      'work/',
      'vendor/',
      'packages/desktop/src-tauri/{target,gen,icons}/',
      'packages/*/{dist,.next}/',
      'next-env.d.ts',
      'pnpm-lock.yaml',
    ],
  },
  lint: {
    ignorePatterns: [
      '.docs/',
      'packages/ui/src/generated/',
      'tests/fixtures/',
      'work/',
      'vendor/',
      'packages/desktop/src-tauri/{target,gen}/',
      'packages/*/{dist,.next}/',
    ],
    options: { typeAware: true, typeCheck: true },
  },
});
