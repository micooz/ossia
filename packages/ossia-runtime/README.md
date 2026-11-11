# ossia-runtime

One local runtime owns each canonical library home. Browser HTTP, CLI and stdio MCP use the same library, queue and engine; GUI initialization is unnecessary.

## Development

```sh
pnpm build:headless
pnpm engine:verify
pnpm dev:runtime
```

`dev:runtime` explicitly selects the repository `.ossia`, port 3212, the prepared engine and UI dist, and permits `http://127.0.0.1:5173`. It does not start Vite or install resources. The runtime uses `library.sqlite` in the selected home. The P05 score SPA and Tauri main window consume the same local API; `pnpm dev:desktop` also drives the development service and Vite.

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

CLI commands accept one JSON object matching the corresponding MCP tool. Successful commands print JSON; failures print `{error:{code,message,detail?}}` and exit nonzero. `serve` and `mcp` reserve stdout for their respective protocol roles. Doctor reports offline engine verification and discovery; integrate returns unsupported until P06.

`docs` searches or reads the offline LilyPond 2.26.0 reference. The pack is
installed into the data directory on first use. CLI/MCP status and context
always return compact Agent projections; `list` and `history` are paged.
`import` and `check` return bounded job summaries; use `job` with
`detail:"full"` for complete evidence. `job` returns `{job, analysis}` and
supports bounded evidence sections. `layout` reports page, system and staff
structure from a completed import or check job.

```sh
target/debug/ossia-desktop --home /absolute/test-library import \
  '{"path":"/absolute/input.ly","title":"Example","format":"ly","author":"Ossia"}'

target/debug/ossia-desktop --home /absolute/test-library context \
  '{"viewId":"00000000-0000-4000-8000-000000000001"}'
```

Import, check, restore and output return a durable Job. Poll `job '{"jobId":"…","action":"get"}'`, or pass `waitMs` on the originating tool; cancel with action=cancel. Running cancellation reports phase=cancelling until the worker finishes process cleanup. If a durable operation has completed, its done result wins. The import request job contains its ImportOutcome and library import record. Refused or unconfirmed reports have no document and must be inspected before continuing.

`open` returns a viewId, a URL with a one-use ticket, and the initial compact context. All context/edit/check/commit calls use an explicit viewId; edit credentials stay private to the caller. Resume transfers an interrupted edit to a new view only over trusted IPC with the original editId. End preserves files; discard and recovery acknowledgment are separate hash-guarded actions.

## HTTP and events

HTTP binds only 127.0.0.1; the default port is 3211, and tests may request 0. The actual address comes from RuntimeInfo. An occupied explicit port fails without probing alternatives.

`/health` returns only alive/protocolVersion. Data routes are beneath `/api/v1` and validate exact loopback Host, allowed Origin and bearer. POST sessions/exchange accepts a 5-minute one-use ticket and returns a 24-hour view-bound token; browser clients keep it only in sessionStorage and remove the ticket fragment. Runtime restarts invalidate both views and tokens.

GET views/{viewId} returns a snapshot with instanceId and seq. Subscribe to GET events?instanceId=…&after=… using an Authorization-bearing fetch stream. Events use increasing instance-wide sequence numbers; private view events are filtered. Initial connection and reconnection both start with a fresh snapshot. A missing/old cursor returns conflict; a stream that falls behind emits resync-required. Token expiry emits session-expired. Neither event requires guessing a replacement view.

JSON request bodies are limited to 1 MiB. Multipart import accepts one `metadata` JSON field (title, format, confirmation?, author) and one `file`; local filesystem paths are rejected in metadata. File inputs are capped at 32 MiB. Inputs and native operations remain local.

A content poll every second complements native filesystem notifications, including environments where FSEvents cannot start. Changes have a further 300 ms debounce; atomic replacement and deletion/recreation are supported. Only pending previews coalesce, and superseded job IDs remain cancelled and queryable. Explicit checks keep their captured snapshots. Context/check/commit reread source even without a watcher event.

Render keys include document ownership, source hash, engine build and options hash. SVG is served only through a stored manifest and verified artifact; authenticated geometry returns page viewBox, locator capability and staff rectangles from the shared index. `POST /outputs` accepts a completed job or immutable version source, supports SVG/PNG/PDF/MIDI/LilyPond, uses the captured snapshot and never overwrites destinations. Missing outputPath selects a unique file under the system Downloads/Ossia directory. `POST /documents/{id}/versions/{versionId}/render` renders a fixed version without changing the public source or preview. `POST /jobs/{jobId}/reveal` accepts only the verified result path of a successful output job.

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

Rust DTOs own TypeScript declarations and all MCP tool schemas. Generated files have non-mutating drift checks in CI. Protocol and library schema are 1. Missing native resources fail the explicit native suite rather than count as skipped success.

The runtime lock is Library's lifetime OS lock, not the discovery JSON. Unix IPC uses a checked private per-user directory and a 0600 socket; Windows pipes receive a DACL for the current user's SID. The lock winner alone publishes/replaces discovery and stale sockets. P05 browser integration is covered separately by `pnpm test:ui` and `pnpm test:ui:native`; headless task and transport tests do not establish other-platform native behavior or P06 host integrations.
