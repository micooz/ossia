# Test fixtures

Committed synthetic inputs and reference results shared by the Rust crates and
browser tests. These files contain no user scores or library data.

| Directory | Purpose |
| --- | --- |
| `inputs/` | LilyPond, MusicXML and byte-level MIDI inputs; `manifest.json` documents coverage and expectations. |
| `inputs/pitch-layout/` | Additional sources for native pitch-state and compressed-rest cross-checks. |
| `rendered/` | LilyPond 2.26.0 source/SVG pairs, a page manifest and the MIDI files used by pitch assertions. |
| `analysis/` | Normalized token and two-staff geometry expectations captured from the independent TypeScript reference. |
| `contracts/` | Shared DTO examples and generated MCP tool schemas. |

Paths in the input and render manifests are relative to `tests/fixtures/`.

## Updating references

Input bytes matter: preserve BOM, CRLF and Unicode. `raw-emoji.ly` is deliberately
invalid LilyPond; `xml-negative-skip.musicxml` deliberately overlaps events in one
voice. Converted render sources enable anchors/MIDI and have their own byte
positions; direct LilyPond render sources preserve the input bytes. SVG links
normalize scratch paths while retaining LilyPond's source positions.

The analysis baselines retain only values asserted by the tests. Token spans use
UTF-8 bytes and Unicode scalar columns; the reference already joins adjacent
same-mode RAW surrogate pairs. Rust behavior corrections are checked separately
with explicit assertions. Do not regenerate these expectations from the Rust
implementation under test.

MCP schemas come from `ossia_runtime::mcp::tools()`. Run
`pnpm generate:contracts` to update `contracts/mcp-tools.json` and
`pnpm check:contracts` to check for drift; do not edit it by hand.

## Checks

```sh
cargo test -p ossia-score -p ossia-types
pnpm check:contracts
OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" \
OSSIA_NATIVE_EVIDENCE="$PWD/target/native-score.json" \
  cargo test -p ossia-engine --test score_native_crosscheck -- --ignored
```

Native tests compile in temporary directories. Optional `OSSIA_NATIVE_EVIDENCE`
reports belong in ignored `target/`, separate from committed reference data.
Historical migration logs, host screenshots and unconsumed outputs are available
in Git history. Passing on one native target does not validate other platforms
or installation packages.
