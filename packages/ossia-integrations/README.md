# ossia-integrations

Host integration planning and ownership for the local Ossia runtime.

The crate detects Codex and DeepSeek Harness, produces a deterministic plan, applies only that plan, verifies the resulting host configuration, and records hashes for files it owns. It does not open the Ossia library or write score data.

## Modes

- `skill-mcp` installs the public `ossia` Skill and registers a local stdio MCP server.
- `plugin` installs a thin host plugin containing the Skill and MCP configuration. Codex uses a local marketplace; DSH uses a prebuilt profile bundle with Host and Client entries.

Only one mode may be active for a host/profile. Switching modes removes the old managed entries before installing the new ones.

## Commands

```sh
ossia integrate detect
ossia integrate plan '{"host":"codex","profile":"default","mode":"plugin"}'
ossia integrate apply '{"host":"codex","profile":"default","mode":"plugin","planHash":"..."}'
ossia integrate status '{"host":"codex","profile":"default"}'
ossia integrate repair '{"host":"codex","profile":"default","mode":"plugin"}'
ossia integrate remove '{"host":"codex","profile":"default"}'
```

Host locations follow `CODEX_HOME`, `DSH_HOME`, and `AGENTS_HOME`; explicit `OSSIA_CODEX_BIN` and `OSSIA_DSH_BIN` override executable discovery. Installation templates are embedded in the executable from `resources/`, so the binary and its Skill, locale, client entry and icon always carry the same version. The MCP command always uses the current executable's absolute path and an explicit absolute Ossia home.

Owned files and host entries are recorded in `<Ossia home>/integrations/managed.json`. A foreign entry with the same name, a changed owned file, or a moved plugin source is a conflict and is never overwritten.

Apply validates the plan hash and rolls back on failure. Status checks the ownership record, file hashes, host CLI output, plugin source, and executable path. Remove preserves unrelated host services and all score data.

## Tests

```sh
cargo test -p ossia-integrations
```

The native host tests are ignored unless the real tools are supplied:

```sh
OSSIA_TEST_INTEGRATION_ROOT="$PWD/packages/ossia-integrations/resources" \
OSSIA_TEST_OSSIA_BIN="$PWD/target/debug/ossia-desktop" \
OSSIA_TEST_DSH_TEMPLATE="$HOME/.dsh/profiles/desktop" \
cargo test -p ossia-integrations --test native_hosts -- --ignored
```

They use temporary host homes and profiles. They do not prove that a desktop host has loaded the plugin into a model session or rendered the browser panel.
