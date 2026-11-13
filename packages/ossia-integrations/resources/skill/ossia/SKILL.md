---
name: ossia
description: Create, browse, edit, render, version, export, delete, and recover scores in a local Ossia library.
---

# Ossia

Work with the local Ossia score library through MCP or CLI.

## Read the manual first

Before performing score operations, call `ossia_guide` and read the complete
manual returned by the connected runtime. Use the sections that apply to the
requested operation. The guide defines the workflow, parameters, editing rules,
and supported formats.

## Task fit

- Distinguish analysis, source editing, rendering/export, and verification.
  Run only the operations needed for the requested outcome.
- Treat concrete keys, counts, measures, tracks, paper sizes, and note arrays in
  examples as schema examples, not reusable defaults.
- Use `expect`, `reference`, or another optional oracle only when the request,
  supplied artifact, or target version establishes the expected result.

## CLI fallback

If MCP is unavailable, read the guide through this instance's CLI:

{{OSSIA_CLI_GUIDE}}

Keep the executable path and global options from this command for subsequent CLI
operations. The entry is published in the data directory when Ossia starts.

## Capability index

- Runtime, documentation health and diagnostics: `ossia_status` (CLI `doctor` for offline resources).
- Immutable version/job music evidence and measure filters: `ossia_evidence`.
- Offline LilyPond syntax reference: `ossia_docs`.
- Library, score creation, and import: `ossia_list`, `ossia_import`.
- Score preview and selection context: `ossia_open`, `ossia_context`.
- Source reading and editing: `ossia_read_source`, `ossia_edit`, `ossia_write_source`.
- Validation, optional reference comparison, and version creation: `ossia_check`, `ossia_commit`.
- Page, system, staff and source-measure structure: `ossia_layout`.
- Output files from job or version snapshots: `ossia_output`.
- Fixed-snapshot musical evidence: `ossia_job` with `section`.
- Version history and restoration: `ossia_history`, `ossia_restore`.
- Recoverable score deletion: `ossia_delete`, `ossia_trash`, `ossia_untrash`,
  `ossia_purge`.
- Background job status and cancellation: `ossia_job`.
