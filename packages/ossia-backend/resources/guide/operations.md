# Operations

For workflow-specific instructions, use [the task index](../guide.md).

## CLI transport

Use MCP when available. If its tools are unavailable or the MCP server cannot
connect, use the CLI command bound to this instance in the installed Skill.
Ossia publishes `<data-root>/ossia` on macOS and `<data-root>\ossia.exe` on
Windows at startup. Keep `--home` and any resource options from that command so
both transports address the same library.

CLI subcommands use kebab-case names derived from the MCP tool without the
`ossia_` prefix: `ossia_update_metadata` becomes `update-metadata`,
`ossia_write_source` becomes `write-source`, and `ossia_read_source` becomes
`read-source`. Operations with parameters accept a single JSON object argument
with the same fields as MCP. For example, `ossia_list` becomes
`list '{"limit":20}'`, `ossia_job` becomes
`job '{"action":"get","jobId":"…"}'`, and `ossia_output` becomes `output`.
CLI `doctor` provides offline diagnostics. Run `guide` to get the manual's path,
byte size and line count; `--help` lists commands and options. CLI results are
JSON. Score operations start the resident backend on demand.
For an operation's JSON schema, run `ossia schema <command>` (CLI or `ossia_`
tool name); this does not start the backend.

An operation error does not mean MCP is unavailable. If a write loses its
response, inspect current state before retrying through another transport.

## Background jobs and paged responses

CLI/MCP `status` and `context` always return compact Agent responses. `status`
lists active jobs only and includes documentation health; `context` includes one
render snapshot rather than duplicated manifests. `import` and `check` return a
bounded job summary: the full import source, repeated render/manifest and all
evidence are omitted or capped. Use `ossia_job` for full evidence. `list` and
`history` and `trash` are paged with `limit`/`offset`.

`ossia_job` returns `{job, progress, analysis}`. Without `section`, it returns a
summary by default; `detail:"full"` is available for diagnostics. Use `section`
(`performance`, `source`, `differences`), zero-based `offset` and optional `limit`
(1..512, default 64) to read bounded evidence from a completed import/check;
`job.result` is then null and evidence is in `analysis`. Follow `nextOffset` until
null. Pass `waitMs` (0..30000) to wait for a running job. For a version or a job
without stored analysis, use `ossia_evidence` instead.

For live export progress, poll `ossia_job` with `action:"get"` and the output
job ID. While the job runs, `progress` may contain `phase`, `completed` and
`total`. `pages` counts generated SVG/PNG pages; `audio` reports WAV rendering
as a percentage out of 100. Counts may be null when a phase cannot be measured,
and `progress` is null when unavailable, queued, cancelling or finished. Counts
apply only to the current phase; use `job.state` to determine whether the whole
job has finished. `waitMs` waits for a terminal state or timeout, not for each
progress update.

The default check result summarizes `render` with `pageCount`, `pdfAvailable`,
`midiAvailable`, `sourceHash`, `diagnostics` and `formatErrors`. Output job
summaries include the destination paths, formats, sizes and source. Use
`ossia_job` with `detail:"full"` to inspect complete render artifacts.

`ossia_context` returns `lastCheck` as a compact summary with analysis status,
counts, verification status and reference provenance. Use `ossia_job` or
`ossia_evidence` for the event lists.

## Application update notices

`ossia_status` and CLI `status` include `update`, the desktop updater's last
check, or null when no applicable check is known. `checkedAt` is Unix time in
seconds; `stale` means the result is older than the channel's check interval
(24 hours for stable, 6 hours for beta), or has a future timestamp. The status
uses the desktop's configured source and channel. The backend does not check
for updates; the desktop application performs the checks.

When `update.state` is `found` or `ready` and `stale` is false, briefly mention
the new version after completing the user's task and suggest updating from
Ossia's About page. Do not proactively mention other states. If asked about
updates, report the actual state and check time, including when it is stale or
unknown.
