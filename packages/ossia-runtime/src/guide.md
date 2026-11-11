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
For example, `ossia_list` becomes `list`, and `ossia_job` becomes
`job '{"action":"get","jobId":"…"}'`. Run `guide` first for this manual;
`--help` lists commands and options. CLI results are JSON; `guide` returns the
manual as a JSON string. Score operations start the resident runtime on demand.

An operation error does not mean MCP is unavailable. If a write loses its
response, inspect current state before retrying through another transport.

## Create and edit

1. Call `ossia_status` to inspect the connected runtime and capabilities, then
   `ossia_list` to inspect the library.
2. To create a score, establish the requested music and instruments, write a
   self-contained `.ly` file in the host workspace, then call `ossia_import`
   with its absolute path and format `ly`. For an existing file, import its
   absolute path. Poll `ossia_job` until it reaches a terminal state: successful
   import creates the first version and renders it.
3. Call `ossia_open` with the selected `documentId`. Open its one-time URL in the
   host's browser when available; otherwise give the URL to the user. Keep the
   returned `viewId` for this task.
4. Call `ossia_context` with that exact `viewId` to read the document, selection,
   source hash, and source path. Do not use another session's recent selection.
5. Call `ossia_edit` with action `begin`, the current `baseVersionId`, and the
   exact `expectedSourceHash`. Keep `editId` private.
6. Use `ossia_write_source` with `documentId`, `viewId`, `editId`, the current
   `expectedSourceHash` and complete `text`. Keep the returned new `sourceHash`.
   Direct source-file editing remains supported; never bypass an explicit host
   permission refusal by changing transports.
7. Call `ossia_check` with the returned source hash and the same `editId`. Add
   `expect` when musical intent is known. Poll `ossia_job`, inspect the music
   report and verification, then inspect a preview. Repeat write/check within the
   same edit; do not re-import drafts. A done job means the check ran: declared
   expectations must also have verification status `passed` before commit.
8. Call `ossia_commit` when ready with `editId`, `baseVersionId`,
   `expectedSourceHash` and `summary`. If it conflicts, reread context and do not
   overwrite another task. Auto-preview does not authorize commit.
9. Report the committed version, what was actually checked, and any export path.

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

Use `ossia_history` to inspect versions, `ossia_restore` to restore, and
`ossia_export` for output. History and export address `versionId`; restore appends
history and requires a clean workspace. Export supports PDF, MIDI and LilyPond.
SVG previews are available in the score page; PNG export is unsupported. Use Ossia
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
{"expect":{"noteCount":8,"tracks":[{"track":1,"channel":0,"pitches":[60,62,64,65,67,69,71,72]}]}}
```

Read actual track/channel identifiers from the report. Track expectations also
accept `notes` with `pitch`, `start` and `duration` fractions. Simultaneous notes
sort by pitch. Missing timing, SMPTE and format-2 cannot pass timing conditions.
Empty expectations and zero denominators are invalid.

A check result contains `render`, `music`, and `verification`. Compilation success
and intent verification are independent. A failed/unavailable expectation keeps
preview evidence but authorizes no commit. The latest explicit check supersedes
older checks; automatic previews never authorize a commit. Do not drop an
expectation just to make a check pass. Legacy jobs have no recorded music report;
check again instead of treating missing evidence as success.

## Draft artifacts

Call `ossia_artifact` with a completed import/check `jobId` and `format` (`svg`,
`png`, `midi`, `pdf`), then poll its returned job. The result contains a verified
absolute `path`, media type, hash and source provenance. Read that file with the
host's file/image tool. No external conversion tools or artifact-directory search
are needed. This does not create a version or export to a user-selected path.

SVG/PNG accept one-based `page` (default 1). PNG defaults to a white 1600-pixel-wide
page; optional `width` is 1..4096 and output is limited to 16 million pixels.
Artifacts always refer to the source job's captured snapshot, even after edits or
commit. Generate missing artifacts with the same engine build or re-check first.
