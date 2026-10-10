import type { NextConfig } from 'next';

// `@ossia/ly` ships TypeScript source on purpose: the L0/L1/L2/pitch modules are the product's
// core invariants, and a separate build step would be one more thing that can drift from them.
// Next transpiles the sources in place. They must *not* also be listed in
// `serverExternalPackages` — Turbopack rejects a package appearing in both.
const nextConfig: NextConfig = {
  transpilePackages: ['@ossia/ly'],
  // The API surface is the product: no client-side pages live here (packages/ui is the SPA).
  output: process.env.OSSIA_OUTPUT_STANDALONE ? 'standalone' : undefined,
};

export default nextConfig;
