# Ossia guide

Ossia edits one self-contained LilyPond file locally. The browser shows score
previews and history. Host integration is managed from the desktop window or
`ossia integrate`; installed Codex and DSH entries launch this same local runtime.

## CLI transport

Use MCP when available. If its tools are unavailable or the MCP server cannot
connect, use the CLI command bound to this instance in the installed Skill.
Ossia publishes `<data-root>/ossia` on macOS/Linux and `<data-root>\ossia.exe` on
Windows at startup. Keep `--home` and any resource options from that command so
both transports address the same library.

CLI subcommands use the MCP tool name without the `ossia_` prefix. Operations with
parameters accept a single JSON object argument with the same fields as MCP.
For example, `ossia_list` becomes `list '{"limit":50}'`, `ossia_job` becomes
`job '{"action":"get","jobId":"…"}'`, and `ossia_output` becomes `output`.
CLI `doctor` provides offline diagnostics. Run `guide` first for
this manual; `--help` lists commands and options. CLI results are JSON; `guide`
returns the manual as a JSON string. Score operations start the resident runtime
on demand.

An operation error does not mean MCP is unavailable. If a write loses its
response, inspect current state before retrying through another transport.

## Before external lookup

For questions about pitch, octave, duration or measure placement, inspect Ossia
evidence before opening a PDF, image or exported MIDI:

1. Call `ossia_evidence` with `section:"summary"`.
2. Call `ossia_evidence` with `section:"source"` when source tokens, written
   spelling or source spans matter.
3. Call `ossia_evidence` with `section:"performance"` when sounding pitch or
   timing matters.

Use `measureRange` on source or performance evidence to keep the answer small:

```json
{
  "source": { "kind": "version", "versionId": "…" },
  "section": "performance",
  "measureRange": { "start": 17, "end": 17 },
  "limit": 64
}
```

Do not infer pitch from preview pixels. Do not parse exported MIDI or PNG files
when `ossia_evidence` can answer the question.

## Immutable evidence and reference checks

Use `ossia_evidence` when you need to inspect an existing version or completed
job without editing it. It accepts `{"kind":"version","versionId":"…"}` or
`{"kind":"job","jobId":"…"}`, plus `section` (`summary`, `performance` or
`source`), `offset`, `limit`, `waitMs` and optional music filters. The first
request may create a durable evidence job; later requests reuse the report for
the same source, engine build and analysis version. Evidence never creates a
version or changes the current source.

`measureRange` is an inclusive one-based range. It filters both source and
performance notes by their own measure number. `voice` filters source events;
`track` and `channel` filter performance notes. Invalid ranges and incompatible
filters fail explicitly. Simultaneous source branches use stable names such as
`left[1]` and `left[2]` in source order.

Source measure numbers come from explicit bar boundaries (`|` or `\bar`) and
the end of a simultaneous branch. Performance measure numbers come from MIDI
time-signature events. A source score with repeats, a pickup, an unsupported
meter map or ambiguous timing may expose `measure: null`; never treat a missing
number as measure one.

`pitchTokens` preserve the literal source note tokens, including duration and
articulation text. `pitches` are the sounding MIDI pitches predicted from the
source. Performance notes use `pitchName`, which is a canonical sharp spelling;
it cannot distinguish enharmonic spellings such as F♯ and G♭. Use source
`pitchTokens` when spelling matters.

Source evidence can be partial when the LilyPond input uses constructs outside
the verified analysis subset. `\transpose`, `\relative`, `\tempo` and
simultaneous `<< … \\ … >>` voices are handled; common non-pitch `\set`
assignments are ignored. Chord mode, repeats with unresolved expansion, semantic
includes and opaque Scheme remain conservative. A partial source report does
not make performed-note evidence unavailable.

Import and check accept either `expect` or `reference`, never both:

```json
{
  "reference": {
    "source": { "kind": "version", "versionId": "…" },
    "comparison": "performance"
  }
}
```

`performance` compares the multiset of performed notes by pitch, quarter-note
start and duration; track order, channel and MIDI file order are ignored.
`tracks` also includes track and channel. Differences report missing and extra
notes without index-based cascades. Missing start or duration makes the result
`unavailable`; it never silently passes. A reference comparison proves performed
equivalence, not identical notation, staff assignment or engraving.

`reference` is an optional oracle, not a default verification step. Use it only
when the user names a reference edition, supplies a reference file, or an
existing library version is the intended base. Do not search for or import a
public-domain reference merely to validate an ordinary new score. If the user
does provide a reference file, import it once as a normal document and use its
`versionId`; there is no direct external-file reference API.

## LilyPond reference

The runtime installs the official English LilyPond 2.26.0 manuals into its data
directory. Use `ossia_docs` with action `search` before guessing an unfamiliar
syntax, octave rule, engraving command or layout property. Read promising results
with action `read` and the returned `chapterId`; `offset` and `nextOffset` page
long chapters. Search asks for `query`; read asks for `chapterId`. Local search
accepts a few Chinese terms such as 八度, 时值 and 谱号.

Search and chapter results include the authoritative LilyPond URL, a local
chapter ID and the offline source archive path. The archive retains English
manual HTML, examples, images and the upstream notices. Search text is indexed
from the same 2.26.0 package used by the engine. A missing or damaged package is
reported by `ossia_status.documentation`; it does not disable score editing.

Official examples may use includes, multiple files or LilyPond features outside
Ossia's current verified analysis subset. Follow the Ossia workflow for ownership,
snapshots and commit authorization, and use a check to verify the actual result.

## Create and edit

1. Call `ossia_status` to inspect the connected runtime, documentation and active
   jobs, then `ossia_list` to inspect compact document metadata.
2. To create a score, establish the requested music and instruments, write a
   self-contained `.ly` file in the host workspace, then call `ossia_import`
   with its absolute path. `format` is inferred from the extension; `title`
   defaults from the filename and `author` from `"Ossia"`. Pass `expect` or
   `reference` only when the user supplied the musical intent or reference:
   an import that fails the check returns a job error with
   `verification.differences` and actual pitches, and creates no document or
   version. Pass `waitMs` to wait for completion, or poll
   `ossia_job`. The top-level import outcome also reports `verification`.
3. Call `ossia_open` with the selected `documentId`. The result is a stable
   `score.html?documentId=…` URL plus the initial compact context. Open it in an
   in-app browser when available; otherwise launch the system browser (`open
   <url>` on macOS, `xdg-open <url>` on Linux, or `Start-Process <url>` on
   Windows). The URL contains no credential and can be reused after restarts.
4. Call `ossia_context` with that `documentId` when you need to refresh the
   document, shared selection, source spans and edit state. Selections belong to
   the document and are shared by its browser tabs. The compact response has no
   source filesystem path.
5. Call `ossia_edit` with action `begin`, the current `baseVersionId`, and the
   exact `expectedSourceHash`. Keep `editId` private.
6. Use `ossia_read_source` with that source hash. Start with
   `{"kind":"start"}` or `{"kind":"line","line":N}`. Continue with the returned
   `next` span while `complete` is false. Exact ranges use
   `{"kind":"span","span":...}`. Read through this tool even when you authored the
   host file: its spans are authoritative. Do not compute byte offsets yourself.
7. Use `ossia_write_source` with `change.kind = "edit"` and up to 64 non-overlapping
   edits. Every edit supplies an exact `span`, its `expectedText`, and
   `replacement`; empty expected text inserts and empty replacement deletes.
   Reserve `change.kind = "replace"` for a deliberate whole-file rewrite. Do not
   edit the source file directly. Keep the returned new `sourceHash`. Set
   `"preview": false` when you will call `ossia_check` immediately; otherwise the
   file watcher also compiles an SVG-only preview.
8. Call `ossia_check` with the returned source hash and the same `editId`. Add
   `expect` or `reference` when musical intent is known. Pass `waitMs`, or poll `ossia_job`.
   Inspect the check result and verification, then inspect a preview. Repeat
   write/check within the same edit; do not re-import drafts. A done job means the
   check ran: declared expectations must also have verification status `passed`
   before commit. The direct CLI/MCP response is a bounded job summary; use
   `ossia_job` when full source, event or MIDI evidence is needed. Use
   `ossia_layout` with the completed `jobId` to inspect page, system and staff
   structure without image inspection.
9. Call `ossia_commit` when ready with `editId`, `expectedSourceHash` and
   `summary`; document and base version are derived from the edit. If it
   conflicts, reread context and do not overwrite another task. Auto-preview does
   not authorize commit.
10. Report the committed version, what was actually checked, and any output path.

End preserves files; discard is explicit and hash-guarded. Resume requires the
original interrupted `editId`. Runtime restarts preserve document URLs and
workspaces; reopen the document, then explicitly resume interrupted work.

## New score example

An empty library does not require downloading an existing piece. A minimal piano
skeleton is:

```lilypond
\version "2.26.0"
\header { title = "Untitled" }
\score {
  \new PianoStaff <<
    \new Staff { \clef treble \time 4/4 R1 | }
    \new Staff { \clef bass \time 4/4 R1 | }
  >>
  \layout { }
  \midi { }
}
```

These skeletons are examples, not defaults: take clef, key, octave, time,
instruments and paper size from the request. A short single-staff example is:

```lilypond
\version "2.26.0"
\header { title = "Example" }
\paper { #(set-paper-size "a6" 'landscape) }

music = \relative c' {
  \clef treble
  \key c \major
  \time 4/4
  c4 d e f |
  g4 a b c |
  \bar "|."
}

\score {
  \new Staff \music
  \layout { ragged-right = ##f }
  \midi { }
}
```

`a6` landscape and `ragged-right = ##f` are presentation choices for this short
example; use the requested page size and layout for real scores. Outside
`\relative`, a plain `c` is C3 and `c'` is middle C. Do not infer an octave from
note names; verify the performed pitches. The snippet above performs 60–72,
while `c4 d e f | g4 a b c'` performs 48–60.

Rendering hides LilyPond's default tagline. A source that explicitly sets
`tagline` keeps that value.

## Import reports and validation

Lossy imports need the returned confirmation hash; refused inputs cannot be forced.
Clean import means conversion succeeded, not musical or engraving correctness.
A null report `measures` count means unavailable; read `measureCountReason`.
`noteEvents` counts written note tokens, not performed MIDI notes. MIDI/token
differences are diagnostics, not proof of musical intent; an unavailable
difference result is not proof of no change.

Inspect the rendered score for beams, clefs, spacing, and collisions. Successful
compilation and MIDI checks alone cannot establish readability or fidelity to a
reference.

## History and output

Use `ossia_history` to page through versions, `ossia_restore` to restore, and
`ossia_output` for files. History and restore address `versionId`; restore appends
history and requires a clean workspace. Output accepts a completed job snapshot
or a version snapshot and supports SVG, PNG, PDF, MIDI and LilyPond. Use Ossia
operations instead of private engine binaries.

## Source locations

Locations use UTF-8 byte spans, one-based pages/measures, zero-based staves and
SVG user units.

## Musical checks

Explicit import/check produces SVG, source evidence and MIDI evidence when
available. Automatic previews produce SVG only. Written note tokens and performed
MIDI note-ons are different counts: repeats, ties and unused definitions matter.
`music.source` and `music.performance` report coverage separately. Unknown counts
are null, not a verified zero. MIDI time uses `{numerator, denominator}` fractions
of a quarter note; duration is key-release time and excludes pedal acoustics.

For the single-staff example above, an exact expectation could be:

```json
{
  "expect": {
    "noteCount": 8,
    "tracks": [{ "track": 1, "channel": 0, "pitches": [60, 62, 64, 65, 67, 69, 71, 72] }]
  }
}
```

These values belong to the example; use the requested pitches, timing, track and
channel for the actual score. Read actual track/channel identifiers from the
report. Track expectations also
accept `notes` with `pitch`, `start` and `duration` fractions. Simultaneous notes
sort by pitch. Missing timing, SMPTE and format-2 cannot pass timing conditions.
Empty expectations and zero denominators are invalid.

A check result contains `render`, `music`, and `verification`. Compilation success
and intent verification are independent. A failed/unavailable expectation keeps
preview evidence but authorizes no commit. The latest explicit check supersedes
older checks; automatic previews never authorize a commit. Do not drop an
expectation just to make a check pass. For an older job without recorded evidence,
call `ossia_evidence` with its `jobId`; do not treat missing evidence as success.

## Reviewing an existing version

1. Use `ossia_history` to choose an immutable `versionId`.
2. Call `ossia_evidence` with `section:"summary"` to inspect source and performance
   coverage and counts.
3. Read `source` or `performance` pages when pitches, spelling, timing or measure
   placement matter. Use `measureRange` and page with `offset` and `nextOffset`.
4. If the user explicitly wants to match this version, compare a draft or import
   by passing it as `reference` to `ossia_check` or `ossia_import`. Do not compare
   to it merely because it exists.
5. If the version is the desired base, open it and use the normal edit/check/commit
   workflow. If it already matches the request, do not duplicate it merely to
   prove that it exists.

Use image inspection for readability, collisions, spacing and visual structure.
It is not authoritative evidence for pitch or octave.

## Rendered structure

`ossia_layout` accepts a completed import or check `jobId` and returns
`pageCount`, `systemCount`, `staffCount`, and per-page `systems[].staves` with
bounds in SVG user units. Each system also reports `bars`, `measureStart` and
`measureEnd` when the source measure grid is reliable. Use these fields instead
of counting measures in a PNG. `measureMapping` is `complete`, `partial` or
`none`; inspect the page reason when it is not complete. `ossia_layout` does not
identify clefs, keys or time signatures; read the source and docs for those.

## Output files

Call `ossia_output` with a `source` of `{"kind":"job","jobId":…}` or
`{"kind":"version","versionId":…}` and a `format` of `ly`, `svg`, `png`, `midi`
or `pdf`, then poll its returned job. The result contains a verified absolute
`path`, media type, hash and source provenance. Read that file with the host's
file/image tool. No external conversion tools or artifact-directory search are
needed. Job outputs use the captured import/check snapshot; version outputs use
the immutable version snapshot.

SVG/PNG accept one-based `page` (default 1). PNG defaults to a white 1600-pixel-wide
page; optional `width` is 1..4096 and output is limited to 16 million pixels.
`outputPath` must be absolute and must not exist; otherwise output goes to the
system download directory under `Ossia/`. Generate missing formats with the same
engine build or re-check first.

## Compact Agent responses

CLI/MCP `status` and `context` always return compact Agent responses. `status`
lists active jobs only and includes documentation health; `context` includes one
render identity rather than duplicated manifests. `import` and `check` return a
bounded job summary: the full import source, repeated render/manifest and all
evidence are omitted or capped. Use `ossia_job` for full evidence. `list` and
`history` are paged with `limit`/`offset`.

`ossia_job` returns `{job, analysis}`. Without `section`, it returns a summary by
default; `detail:"full"` is available for diagnostics. Use `section`
(`performance`, `source`, `differences`), zero-based `offset` and optional `limit`
(1..512, default 64) to read bounded evidence from a completed import/check;
`job.result` is then null and evidence is in `analysis`. Follow `nextOffset` until
null. Pass `waitMs` (0..30000) to wait for a running job. For a version or a
legacy job, use `ossia_evidence` instead.

`ossia_context` returns `lastCheck` as a compact summary with analysis status,
counts, verification status and reference provenance. Use `ossia_job` or
`ossia_evidence` for the event lists.
