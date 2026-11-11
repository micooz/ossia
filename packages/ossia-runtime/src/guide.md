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
   with its absolute path. `format` is inferred from the extension; `title` and
   `author` default from the filename and `"agent"`. Pass `waitMs` to wait for
   completion, or poll `ossia_job`. Successful import creates the first version
   and renders it.
3. Call `ossia_open` with the selected `documentId`. Open its one-time URL in the
   host's browser when available; otherwise give the URL to the user. The result
   already includes the initial compact context; keep `viewId` for this task.
4. Call `ossia_context` with that exact `viewId` when you need to refresh the
   document, selection, source spans and edit state. Do not use another session's
   recent selection. The compact response has no source filesystem path.
5. Call `ossia_edit` with action `begin`, the current `baseVersionId`, and the
   exact `expectedSourceHash`. Keep `editId` private.
6. Use `ossia_read_source` with that source hash. Start with
   `{"kind":"start"}` or `{"kind":"line","line":N}`. Continue with the returned
   `next` span while `complete` is false. Exact ranges use
   `{"kind":"span","span":...}`.
7. Use `ossia_write_source` with `change.kind = "edit"` and up to 64 non-overlapping
   edits. Every edit supplies an exact `span`, its `expectedText`, and
   `replacement`; empty expected text inserts and empty replacement deletes.
   Reserve `change.kind = "replace"` for a deliberate whole-file rewrite. Do not
   edit the source file directly. Keep the returned new `sourceHash`.
8. Call `ossia_check` with the returned source hash and the same `editId`. Add
   `expect` when musical intent is known. Pass `waitMs`, or poll `ossia_job`.
   Inspect the check result and verification, then inspect a preview. Repeat
   write/check within the same edit; do not re-import drafts. A done job means the
   check ran: declared expectations must also have verification status `passed`
   before commit.
9. Call `ossia_commit` when ready with `editId`, `expectedSourceHash` and
   `summary`; document and base version are derived from the edit. If it
   conflicts, reread context and do not overwrite another task. Auto-preview does
   not authorize commit.
10. Report the committed version, what was actually checked, and any output path.

End preserves files; discard is explicit and hash-guarded. Resume requires the
original interrupted `editId`. Runtime restarts invalidate views and page tokens;
re-open, then explicitly resume interrupted work.

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

For example, check an eight-note ascending scale with:

```json
{
  "expect": {
    "noteCount": 8,
    "tracks": [{ "track": 1, "channel": 0, "pitches": [60, 62, 64, 65, 67, 69, 71, 72] }]
  }
}
```

Read actual track/channel identifiers from the report. Track expectations also
accept `notes` with `pitch`, `start` and `duration` fractions. Simultaneous notes
sort by pitch. Missing timing, SMPTE and format-2 cannot pass timing conditions.
Empty expectations and zero denominators are invalid.

A check result contains `render`, `music`, and `verification`. Compilation success
and intent verification are independent. A failed/unavailable expectation keeps
preview evidence but authorizes no commit. The latest explicit check supersedes
older checks; automatic previews never authorize a commit. Do not drop an
expectation just to make a check pass. Jobs without a recorded music report
cannot provide evidence; check again instead of treating missing evidence as
success.

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
render identity rather than duplicated manifests. `list` and `history` are paged
with `limit`/`offset`.

`ossia_job` returns `{job, analysis}`. Without `section`, it returns a summary by
default; `detail:"full"` is available for diagnostics. Use `section`
(`performance`, `source`, `differences`), zero-based `offset` and optional `limit`
(1..512, default 64) to read bounded evidence from a completed import/check;
`job.result` is then null and evidence is in `analysis`. Follow `nextOffset` until
null. Pass `waitMs` (0..30000) to wait for a running job.
