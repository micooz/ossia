---
name: ossia
description: Create, browse, edit, render, version, and export scores in a local Ossia library.
---

# Ossia

Work with the local Ossia score library through MCP or CLI.

## Read the manual first

Before performing score operations, call `ossia_guide` and read the complete
manual returned by the connected runtime.
The guide defines the workflow, parameters, editing rules, and supported formats.

## CLI fallback

If MCP is unavailable, read the guide through this instance's CLI:

{{OSSIA_CLI_GUIDE}}

Keep the executable path and global options from this command for subsequent CLI
operations. The entry is published in the data directory when Ossia starts.

## Capability index

- Runtime, documentation health and diagnostics: `ossia_status` (CLI `doctor` for offline resources).
- Offline LilyPond syntax reference: `ossia_docs`.
- Library, score creation, and import: `ossia_list`, `ossia_import`.
- Score preview and selection context: `ossia_open`, `ossia_context`.
- Source reading and editing: `ossia_read_source`, `ossia_edit`, `ossia_write_source`.
- Validation and version creation: `ossia_check`, `ossia_commit`.
- Output files from job or version snapshots: `ossia_output`.
- Fixed-snapshot musical evidence: `ossia_job` with `section`.
- Version history and restoration: `ossia_history`, `ossia_restore`.
- Background job status and cancellation: `ossia_job`.
