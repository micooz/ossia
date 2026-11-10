# TypeScript migration reference

These pure modules preserve the P00 source/layout join and import-warning classifier after removal of the Next.js backend. They run only through the local baseline and engine probes, use no database or application runtime, and import the retained `packages/ly` comparison library directly.

`pnpm baseline:local --check` verifies that relocation did not change `observed.json`. Keep this reference until the remaining Rust import/analysis comparisons are covered; do not use it as a production fallback. Removed engine, storage and HTTP implementations remain available from Git commit `1f9974c` for the P02–P04 port.
