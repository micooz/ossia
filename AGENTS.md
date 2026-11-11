# Project architecture

Ossia is a local-only score editor. Rust owns the core; Vite + React owns the
browser score page; Tauri owns the management window.
`packages/desktop/src-tauri` is the only shipping entry and dispatches GUI,
`serve`, `mcp`, `doctor` and `integrate` modes before creating a Tauri window.

- `packages/ossia-types`: shared DTOs, typed identifiers, error codes and
  protocol/schema versions.
- `packages/ossia-score`: pure in-memory analysis. It must not use files, a
  database, the network or LilyPond processes, and must not depend on Tauri.
- `packages/ossia-engine`: bundled LilyPond toolchain, conversion/compilation,
  process supervision and render artifacts.
- `packages/ossia-library`: SQLite, content-addressed snapshots, the public
  `score.ly`, edit ownership, versions and recovery.
- `packages/ossia-runtime`: single-instance runtime, IPC, loopback HTTP, jobs,
  file watching, views/events, CLI and MCP.
- `packages/ossia-integrations`: host detection and configuration plans; it stays
  independent of the runtime's HTTP/IPC implementation.
- `packages/ui`: score SPA (`index.html`) and separate management entry
  (`admin.html`); it owns display and interaction, not parsing or version state.
- `packages/ossia-integrations/resources`: Skill, Codex plugin and DSH plugin
  templates embedded into the installer.

Allowed dependency direction:
`ossia-types` → `ossia-score` → `ossia-engine` → `ossia-library` →
`ossia-runtime`; each crate may depend only on crates before it. One runtime
instance owns each data directory.

The score page talks to the runtime over loopback HTTP. CLI and MCP call the
same runtime operations over IPC; MCP starts the resident runtime on demand.
Closing a page or MCP connection does not stop compilation. The browser score
page has no Tauri privileges; only the `management` window does.

# Cross-layer contracts

- Rust owns shared frontend types. Edit definitions in `packages/ossia-types`,
  register new exports in `packages/ossia-types/src/bin/generate-types.rs`, then
  run `pnpm generate:types`; never edit
  `packages/ui/src/generated/protocol.ts` by hand.
- MCP tool schemas are generated from `ossia_runtime::mcp::tools()`; run
  `pnpm generate:contracts`; never edit
  `tests/fixtures/contracts/mcp-tools.json` by hand.
- A Tauri command needs a function in
  `packages/desktop/src-tauri/src/lib.rs`, an entry in
  `tauri::generate_handler!`, the same command in `AppManifest::commands` in
  `packages/desktop/src-tauri/build.rs`, and an `allow-<command>` grant in
  `packages/desktop/src-tauri/capabilities/default.json`. Keep it scoped to the
  `management` window.

# Required reading

- Before changing code, follow [docs/coding.md](docs/coding.md).
- When writing a commit message, follow
  [.github/COMMIT_MESSAGE_INSTRUCTIONS.md](.github/COMMIT_MESSAGE_INSTRUCTIONS.md).
