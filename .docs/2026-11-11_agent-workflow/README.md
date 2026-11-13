# Agent workflow

Ossia exposes one Agent contract over IPC, CLI and MCP. The browser keeps full
view snapshots; Agent transports return compact projections.

## Evidence

Source analysis distinguishes written pitch tokens from performed MIDI note-ons.
Unavailable coverage is not an empty score. `MusicReport` reports source and
performance separately, and `QuarterTime` uses exact fractions of a quarter note.

Explicit checks request SVG and MIDI; automatic previews request SVG only. The
latest explicit check owns the edit receipt. Declared expectations must pass
before commit. Preview and stale checks do not authorize a commit.

## Source editing

`write_source` uses the durable worktree journal and requires the active owned
edit, the expected source hash and exact edit text. It returns the new hash
without creating a version.

`read_source` pages from a line or exact `SourceSpan`, bounded by lines
and bytes. `includeLineSpans` returns editable spans for every returned line
fragment. `write_source` accepts a whole-file replacement or up to 64
non-overlapping edits. The compact context omits the source filesystem path.

## Output files

`output` writes SVG, PNG, MIDI, PDF or LilyPond from a completed import/check
snapshot or an immutable version. PNG uses bounded resvg rendering, external
image reads are disabled, and the cache key includes SVG, dimensions, renderer
and font metadata. Visual crops accept a measure range or system, use render
geometry and actual ink bounds, and return one atomically published file per
affected page. Output paths never overwrite existing files.

## Offline reference

The bundled LilyPond 2.26.0 reference contains filtered documentation, examples,
images and source notices with an FTS5 index. It installs by content identity and
does not block runtime startup. Missing documentation affects documentation
requests and status only.

## Agent projections

`status` omits terminal task history, `context` omits duplicate render manifests,
and `list`/`history` page compact records. `job` returns `{job, analysis}`;
`section` reads immutable evidence in pages of at most 512, while
`detail: "full"` is diagnostic-only. Browser view snapshots include full render
manifests. `evidence` exposes bounded music only through `page`; its job
envelope carries status without duplicating `job.result`.

Selections store a conservative music identity. A canvas rerun preserves them
across annotations or layout-only source changes after resolving locators
against the new snapshot. Music, voice or staff changes and partial analyses
clear the selection. `selection-changed` reports `source-music-preserved` or
`source-structure-changed`.

`PROTOCOL_VERSION` is 1 and the current library `SCHEMA_VERSION` is 2. An
incompatible local library can be reset.
