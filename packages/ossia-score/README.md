# ossia-score

Pure, in-memory analysis of single-file LilyPond source, rendered SVG pages and Standard MIDI Files. The crate does not read files, run LilyPond or depend on a GUI, database or service.

```rust
use ossia_score::{build_index, build_layout, locate, LocateQuery, SvgPage};

let source = "Music = { c'1 | } \\score { \\new Staff \\Music }";
let index = build_index(source);
assert_eq!(index.max_measures, 1);
// The caller supplies the SVG compiled from exactly this source snapshot.
let svg = "<svg viewBox='0 0 100 100'/>";
let layout = build_layout(&index, &[SvgPage { page: 1, svg }])?;
let hit = locate(&index, &layout, &LocateQuery::Point { page: 1, x: 20.0, y: 20.0 })?;
assert!(hit.region.is_none());
# Ok::<(), ossia_score::AppError>(())
```

## Interface contracts

- `tokenize` / `serialize` preserve every input byte, including BOM, CRLF and unknown Unicode scalars. Tokens use byte offsets. `SourceMap` validates scalar boundaries and converts to one-based scalar line/column positions. Its tab-expanded display conversion follows the P00 2.26.0 probes, not terminal grapheme widths.
- `build_index` finds balanced music expressions and explicit measure boundaries in converter output, named variables and inline Staff music. `% N` comments supply inclusive range endpoints. `\bar` commands and the end of a simultaneous branch also close a measure; a multiplied rest can map several measures to the same source span.
- `resolve_locator` validates the explicit staff and optional voice. Missing identifiers return `not-found`, absent measures return `none`, opaque music returns `anchor-only` with a reason. It never substitutes an unrelated voice. A range covering part of a compressed rest returns the entire written rest span.
- `build_layout` accepts explicitly numbered pages and the source index. Coordinates are SVG user units. It supports affine translate/scale/matrix transforms, paths with M/L/H/V/C/S/Q/T/Z, lines, rectangles and polygons. Bounds include Bézier control points. Unsupported elements/transforms and malformed XML have distinct page errors; source anchors are retained when only staff geometry is unavailable.
- Global measure columns collapse parallel staves, recover a missing opening bar from the staff edge and source anchors, and use singleton source measures to eliminate full-height stems. Broken or missing pages break sequential numbering; subsequent columns need source evidence to recover it. `ScoreLayout.reason` and individual page/column capabilities describe partial results.
- `locate` accepts a point or a source locator. It rejects a source/layout hash mismatch with `stale-selection`; nonexistent pages, margins and gaps do not fall back. `anchor-only` exposes source evidence, without an inferred measure or note selection.
- `resolve_pitches` accepts a validated span of one complete music expression. It supports relative/absolute wrappers, `\transpose`, simultaneous `<< … \\ … >>` branches with independent pitch state, nested mode restoration, chord first-member references and unison deduplication; common non-pitch `\set` assignments are ignored. Unsupported music returns an empty event list and a reason. Document-level note-language changes and includes other than the verified english/nederlands note definitions are also unsupported. Events expose sounding MIDI pitches and literal source pitch tokens, but not performed repeats, tied-note merging or timing; out-of-MIDI-range pitches are flagged.
- `parse_midi` checks SMF formats 0/1/2, chunk and event boundaries, four-byte VLQs, channel running status, meta/SysEx lengths and end-of-track markers. It also retains time-signature meta events for one-based performance measure assignment. Zero-velocity note-ons are excluded. `diff_midi` compares track presence, note-on counts, pitches, channels and quarter-note start positions; it rejects SMPTE and independent format-2 comparisons. Note-offs, duration, velocity and tempo maps are outside the diff.

Composed music variables that cannot be expanded safely, repeats, executable music Scheme and missing measure boundaries still degrade rather than produce precise source ranges. This is not a complete LilyPond parser. Standard-library versus external include validation belongs to P02.

## Checks

```sh
cargo test -p ossia-score -p ossia-types
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Tests consume the shared inputs, rendered pages and normalized token/geometry baselines in `tests/fixtures/`. Normative assertions separately cover corrections to TypeScript behavior. See the fixture README for update rules.

The native test is explicitly ignored by default and fails without the requested engine when invoked:

```sh
OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" \
OSSIA_NATIVE_EVIDENCE="$PWD/target/native-score.json" \
cargo test -p ossia-engine --test score_native_crosscheck -- --ignored
```

It compiles `tests/fixtures/inputs/pitch-layout/` sources in temporary directories and verifies pitches, SVG columns and unchanged source bytes against native LilyPond 2.26.0. The evidence file records the actual target; passing it on macOS is not Linux or installation-package validation.
