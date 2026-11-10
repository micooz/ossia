# Local migration fixtures

These inputs are original synthetic examples. They contain no user scores or library data. `manifest.json` lists each input's coverage and normative expectations; `known-differences.json` separates old TypeScript bugs and gaps from the P01 target behavior.

## Commands

Requires the repository's Node.js/pnpm toolchain. The native probe additionally requires LilyPond 2.26.0 at the vendored macOS ARM64 path, or an explicit `OSSIA_LILYPOND_ROOT` pointing to that version.

```sh
pnpm baseline:fixtures       # rebuild synthetic text and binary inputs
pnpm probe:engine            # regenerate native reference inputs and evidence
pnpm baseline:local          # regenerate normalized TS observations
pnpm baseline:local --check  # compare without changing references
pnpm test:local
pnpm probe:host              # print two single-use, five-minute launch URLs
```

Scripts locate the repository from their module URL. For a different cwd, invoke the baseline script by absolute path with `node --experimental-strip-types`, or use `pnpm --dir <repository>`.

## Reference data

- `inputs/`: editable `.ly` and MusicXML; MIDI binaries have byte-level recipes in `scripts/local/fixtures.mts`. `raw-emoji` is deliberately invalid LilyPond. `xml-negative-skip` deliberately overlaps events in one voice.
- `engine/`: 2.26.0 converter sources, normalized SVG and MIDI, plus a render manifest. Converter originals are kept separately from the derived `*-render/score.ly`; only derived sources enable anchors/MIDI. Direct `.ly` compiles use unchanged input bytes and the command-line point-and-click option.
- `observed.json`: current TS behavior with raw UTF-16 tokens, canonical UTF-8 spans, pitches, geometry, locators and MIDI. Raw `s/from/to` fields remain observations of old TS offsets; canonical locations are under `span`.
- `contract-examples.json`: illustrative shared DTO values. The bytes and SVG hash reference `compressed-rest`; these are not implemented production interfaces.
- `evidence/`: native commands, diagnostics, artifact hashes and host observations. PDF hashes may vary due to engine metadata and are excluded from baseline equality. Native raw scratch files are deleted after evidence is saved.

The adapter joins only adjacent same-mode RAW high/low surrogate tokens. It validates byte slices and never changes BOM, CRLF or source text. Normative columns count Unicode scalars, not grapheme clusters or JS UTF-16 code units.

Host URLs carry the launch credential in the fragment, clear it before exchanging it, and keep bearer tokens in page/server memory. Requests and the fetch-based event stream omit cookies. `host-session.json` is overwritten by a new probe session; snapshot historical evidence before rerunning. The development server is a P00 feasibility fixture, not the production runtime or a complete authentication implementation.

Linux ARM64, other native targets, plugin-driven page opening and installation-package behavior remain unverified. See the P00 report for exact boundaries.
