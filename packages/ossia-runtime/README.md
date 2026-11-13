# ossia-runtime

One local runtime owns each canonical library home. Browser HTTP, CLI and stdio MCP use the same library, queue and engine; GUI initialization is unnecessary.

## Development

```sh
pnpm build:headless
pnpm engine:verify
pnpm dev:runtime
```

`dev:runtime` explicitly selects the repository `.ossia`, port 3212, the prepared engine and UI dist, and permits `http://127.0.0.1:5173`. It does not start Vite or install resources. The runtime uses `library.sqlite` in the selected home. The P05 score SPA and Tauri main window consume the same local API; `pnpm dev:desktop` also drives the development service and Vite.

On startup the runtime prunes abandoned imports, stale engine scratch data,
expired jobs and render records, orphaned content and cache entries beyond the
selected 512 MiB, 1 GiB, 3 GiB or 10 GiB limit. It repeats the cleanup every six hours
and after idle work at most hourly. Cleanup waits for the job queue to become
empty, and new jobs remain queued until it finishes. The management window reads
the same status and can request cleanup over IPC; it never removes documents or
version history automatically. Automatic cleanup is enabled by default and can
be disabled in the management window without disabling manual cleanup.

The binary is `target/debug/ossia-desktop`. Global options precede or follow the subcommand:

```sh
target/debug/ossia-desktop --home /absolute/test-library \
  --engine-root /absolute/prepared-engine --port 3211 serve

target/debug/ossia-desktop --home /absolute/test-library status
target/debug/ossia-desktop --home /absolute/test-library open '{}'
target/debug/ossia-desktop --home /absolute/test-library mcp
target/debug/ossia-desktop --home /absolute/test-library stop
```

Home selection is `--home`, `OSSIA_HOME`, then the user's `.ossia`; explicit paths are absolute. Engine selection is `--engine-root`, `OSSIA_ENGINE_ROOT`, then the executable's resources/engine. The offline reference pack follows `--docs-root`, `OSSIA_DOCS_ROOT`, then resources/docs/lilypond/2.26.0. macOS bundles use `Contents/Resources`; other layouts use resources beside the executable. Static files default to resources/ui and can be supplied with `--static-dir`. Resources are never found through cwd or PATH.

CLI commands accept one JSON object matching the corresponding MCP tool. Successful commands print JSON; failures print `{error:{code,message,detail?}}` and exit nonzero. `serve` and `mcp` reserve stdout for their respective protocol roles. Doctor reports offline engine verification and discovery; integrate detects and configures supported hosts.

`docs` searches or reads the offline LilyPond 2.26.0 reference. The pack is
installed into the data directory on first use. CLI/MCP status and context
always return compact Agent projections; `list`, `history` and `trash` are paged.
`import` and `check` return bounded job summaries; use `job` with
`detail:"full"` for complete evidence. `job` returns `{job, analysis}` and
supports bounded evidence sections. `evidence` reads or derives immutable source
and performed-note evidence from a version or completed job without creating a
version. Both source and performance sections accept `measureRange`; source
also accepts `voice`, and performance accepts `track`/`channel`. Performance
notes carry MIDI time-signature measures and canonical pitch names; source
events preserve literal pitch tokens. Both checks accept either `expect` or
`reference`; reference comparison uses performed notes, while `layout`
separately reports page, system, staff, measure counts and reliable
source-measure mappings.

```sh
target/debug/ossia-desktop --home /absolute/test-library import \
  '{"path":"/absolute/input.ly","title":"Example","format":"ly","author":"Ossia"}'

target/debug/ossia-desktop --home /absolute/test-library context \
  '{"documentId":"00000000-0000-4000-8000-000000000001"}'
```

Import, check, restore and output return a durable Job. Poll `job '{"jobId":"…","action":"get"}'`, or pass `waitMs` on the originating tool; cancel with action=cancel. Running cancellation reports phase=cancelling until the worker finishes process cleanup. If a durable operation has completed, its done result wins. The import request job contains its ImportOutcome and library import record. Refused or unconfirmed reports have no document and must be inspected before continuing.

`open` returns a stable `scores?documentId=…` URL and the initial compact context when a document is selected. Without a document it returns the landing page. All context, selection and edit operations use an explicit documentId. Edit credentials stay private to the caller; resume requires the interrupted editId. End preserves files; discard and recovery acknowledgment are separate hash-guarded actions.

`delete` moves a clean, idle score to a recoverable trash; `force` cancels its
active jobs, waits for them to stop and ends edits while preserving the current
source. `trash` pages deleted scores, `untrash` restores the original identity
and history, and `purge` permanently removes one trashed score. Ordinary cache
cleanup never removes trashed sources or versions. Purged documents and their
derived data are removed; external original and exported files are untouched.

## HTTP and events

HTTP binds only loopback (`localhost`, `127.0.0.1` and `::1`); the default port is 3211, and tests may request 0. RuntimeInfo exposes the canonical `http://localhost:<port>` base URL. An occupied explicit port fails without probing alternatives.

`/health` returns only alive/protocolVersion. Data routes are beneath `/api/v1` and validate loopback Host and an allowed Origin. There is no bearer token: any local process that knows a documentId can access that document. Browser cross-origin calls remain blocked.

GET documents/{id}/context returns a snapshot with instanceId and seq. PUT documents/{id}/selection updates the document's shared selection; POST documents/{id}/locate resolves page coordinates. Subscribe to GET events?instanceId=…&after=…[&documentId=…]. Events use increasing instance-wide sequence numbers and can be filtered to a document. Initial connection and reconnection both start with a fresh snapshot. A missing/old cursor returns conflict; a stream that falls behind emits resync-required. Runtime restarts preserve direct document URLs because documents are persistent.

Selections carry `targets`: `{kind:"measure",locator}` or `{kind:"element",elementId}`. A `selection-changed` event carries `targetCount`, including `0` when a clear is published. Context regions expose validated source spans, element types, all printed occurrences and insertion anchors for generated context properties. Elements take precedence over measures during hit testing. Source edits retain only unambiguously corresponding targets; measure retention additionally requires unchanged musical identity. Interactive render caches refresh through preview jobs without creating versions. The wire protocol version is 2; the database schema version is 2.

JSON request bodies are limited to 1 MiB. Multipart import accepts one `metadata` JSON field (title, format, confirmation?, author) and one `file`; local filesystem paths are rejected in metadata. File inputs are capped at 32 MiB. Inputs and native operations remain local.

A content poll every second complements native filesystem notifications, including environments where FSEvents cannot start. Changes have a further 300 ms debounce; atomic replacement and deletion/recreation are supported. Only pending previews coalesce, and superseded job IDs remain cancelled and queryable. Explicit checks keep their captured snapshots. Context/check/commit reread source even without a watcher event.

Render keys include document ownership, source hash, engine build and options hash. SVG is served only through a stored manifest and verified artifact; geometry returns page viewBox, measure display rectangles, element bounds and validated source targets from the shared index. `POST /outputs` accepts a completed job or immutable version source, supports SVG/PNG/PDF/MIDI/LilyPond, uses the captured snapshot and never overwrites destinations. SVG and PDF output are compiled without LilyPond point-and-click links; PDF also accepts an optional open password. PNG output accepts optional quality and transparent-background settings; quality uses indexed color quantization. Missing outputPath selects a unique file under the system Downloads/Ossia directory. `GET /jobs/{jobId}/output` returns the verified bytes and media type of a completed output job without exposing its local path. `POST /documents/{id}/versions/{versionId}/render` renders a fixed version without changing the public source or preview. `POST /jobs/{jobId}/reveal` accepts only the verified result path of a successful output job.

## Contracts and validation

```sh
pnpm generate:types
pnpm generate:contracts
pnpm check:types
pnpm check:contracts
cargo test -p ossia-runtime

OSSIA_TEST_EXECUTABLE="$PWD/target/debug/ossia-desktop" \
OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" \
OSSIA_NATIVE_EVIDENCE="$PWD/target/native-runtime.json" \
cargo test -p ossia-runtime --test native_runtime -- --ignored
```

Rust DTOs own TypeScript declarations and all MCP tool schemas. Generated files have non-mutating drift checks in CI. Protocol schema is 1 and library schema is 2. Missing native resources fail the explicit native suite rather than count as skipped success.

The runtime lock is Library's lifetime OS lock, not the discovery JSON. Unix IPC uses a checked private per-user directory and a 0600 socket; Windows pipes receive a DACL for the current user's SID. The lock winner alone publishes/replaces discovery and stale sockets. P05 browser integration is covered separately by `pnpm test:ui` and `pnpm test:ui:native`; headless task and transport tests do not establish other-platform native behavior or P06 host integrations.
