# Agent workflow implementation

The implementation follows six independently verified stages: source analysis,
performance reports, expectation checks, source/artifact operations, compact Agent
responses, and bundled reference documentation. Runtime operations remain shared
by CLI and MCP; browser state remains complete.

## Source analysis

Text-command state ends at brace boundaries. Regression tests
cover header/markup, layout, paper, named music, inline Staff, explicit Voice,
unused variables, Unicode, BOM and CRLF. `cargo test -p ossia-score` verifies
source indexing, pitch prediction and byte-preserving tokenization.

Music analysis availability is separate from import conversion status. Unsupported
source must not be presented as a verified empty score.

## Music evidence

`MusicReport` separates written-source coverage from performed MIDI events.
`QuarterTime` uses exact nonnegative fractions of a quarter note. MIDI key
releases exclude pedal acoustics; ambiguous overlapping keys retain pitches but
have no claimed duration. SMPTE and format-2 retain pitches without shared
quarter-note time. `cargo test -p ossia-score -p ossia-types` passes.

## Checks and commit authorization

Explicit checks request SVG and MIDI and persist `CheckResult`; automatic previews
remain SVG-only. The latest explicit check owns the edit's receipt. Declared
expectations must pass before commit, including unchanged-source commits following
a failed check. Preview and late older checks cannot authorize a commit.

Application, protocol and database version numbers remain unchanged during MVP.
Existing databases receive additive columns transactionally; old receipts are
invalidated, while documents, versions and job history are preserved.

Library/runtime/type tests, Clippy, frontend checks and generated contract checks
pass. A native LilyPond 2.26.0 macOS ARM64 test confirms the C3–C4 mismatch blocks
commit and corrected C4–C5 with quarter-note durations passes.

## Source and draft artifacts

`write_source` uses the existing durable worktree journal and ownership/hash
checks. It returns the computed hash without creating a version. Recovery tests
cover failed writes after replacement and preserve the original version history.

`artifact` queues outputs from completed import/check snapshots. PNG rendering
uses resvg 0.49.0, bounds allocations, disables external image reads, and keys its
cache by SVG, dimensions, renderer and font metadata. Library/runtime/engine tests
and Clippy pass. Native macOS ARM64 evidence covers SVG/MIDI/PDF/PNG before commit,
1600px PNG output, old-snapshot isolation and oversized preview rejection.
