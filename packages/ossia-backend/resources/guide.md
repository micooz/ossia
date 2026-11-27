# Ossia guide

Ossia edits one self-contained LilyPond file locally. The browser shows score
previews and history. Host integration is managed from the desktop window or
`ossia integrate`; installed integrations launch the same local backend.

`ossia_guide` returns this guide's text, absolute path, byte size and line count.
Read this guide first, then call `ossia_guide` with `document` set to each
relevant linked path, such as `guide/editing.md`. Resolve links relative to
the current document and read every document needed for the task.

## Task index

- [Create and edit](guide/editing.md): score creation, editing, selection,
  title metadata, scaffolds, Jianpu, LilyPond reference and visual annotations.
- [Import and recognition](guide/import.md): supported inputs, conversion,
  remote score recognition, import reports and validation.
- [Evidence and checks](guide/evidence.md): musical evidence, expectations,
  reference comparisons, existing-version review and paged job results.
- [History and deletion](guide/library.md): history, restoration, trash and
  permanent deletion.
- [Rendering and output](guide/output.md): rendered structure, source locations,
  exports, output options and freehand drawing data.
- [Operations](guide/operations.md): CLI transport, background jobs, paged
  responses and application update status.

## Task fit

Distinguish analysis, source editing, rendering/export and verification; run
only the operations needed for the requested outcome. Treat concrete keys,
counts, measures, tracks, paper sizes and note arrays in examples as examples,
not defaults. Use optional expectations or reference comparisons only when the
request, supplied artifact or target version establishes the expected result.

## CLI transport

Use MCP when available; otherwise use the instance-bound CLI command in the
installed Skill. See [Operations](guide/operations.md) for CLI use and schemas.

## Application update notices

When `ossia_status.update.state` is `found` or `ready` and `stale` is false,
briefly mention the new version after the task and suggest the About page.
For details and other states, see [Operations](guide/operations.md).

## Score links

`ossia_list` items and `ossia_context.document` include `url`, a stable
`scores?documentId=…` link with no credential that survives restarts. Include
the link in the final reply for any task about a score; the host decides whether
it opens in an in-app or external browser. Without a specific document,
`ossia_status.runtime.webBaseUrl` is the library landing page.

## Evidence selection

Use Ossia evidence as the authoritative source for the current score's written
or performed content. Choose the smallest section that answers the question:

- `source` for written tokens, spelling, source spans or source-measure events.
- `performance` for sounding pitch, timing or performed-measure events.
- `summary` when coverage and counts must be established before paging.

If the question concerns a user-named external artifact, rendered appearance, or
content that evidence reports as unavailable, inspect that artifact and state
why evidence was insufficient.

Do not infer pitch from preview pixels. Use [Evidence and checks](guide/evidence.md)
for filters and paging.

## User-facing responses

Tool results carry identifiers and source details for continuing the workflow;
they are not copy for the user. In an ordinary reply, refer to a score by its
title, a saved change as "a new version", and its predecessor as "the previous
version". Do not expose standalone machine identifiers or content hashes:
`documentId`, `versionId`, `editId`, `jobId`, `renderKey`, `sourceHash`,
`expectedSourceHash`, or UUID/hash fragments. Score URLs and output paths are
user-facing links: include them even when they contain `documentId` or another
identifier, as returned or as a Markdown link. Keep `editId` private even when
the user requests other technical values.

Use musical and product language. Notes, measures, dynamics, rehearsal marks,
printed title, library title, page layout and sounding notes are user terms.
Name LilyPond commands, source syntax, evidence sections, typed field names or
internal types only when the user asks for source-level detail or they are
needed to disambiguate the requested result; explain the term in the same
sentence. For example, describe `performance` verification as matching
"sounding notes, start times and durations", `TextScript` as a "text
annotation", a source `span` as a "source location", and `\header` as the
"printed title".

Report the result rather than implementation metadata: what changed, what was
checked, whether a new version was saved, any output file and any consequence
the user must handle. A changed source hash, parent version, render key or job
ID is normally not a result; say that the source changed or that previous
selections and annotations were cleared instead. Include the score URL for a
score task; its embedded `documentId` does not make the URL private.

When an Ossia capability limitation prevents the requested result, explain the
limitation and any practical workaround, and suggest reporting it to the
developers at https://github.com/micooz/ossia/issues. Describe uncertainty if
the cause is not confirmed; do not present a suspected limitation as a fact.

If the user explicitly asks for an identifier, hash, field or raw value, provide
the requested technical value. Do not volunteer unrelated identifiers.
