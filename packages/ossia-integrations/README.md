# ossia-integrations

Agent integration planning and ownership for the local Ossia runtime.

The crate detects supported Agents, exposes one adapter per Agent, installs the
resources that Agent needs, and records ownership hashes for every file Ossia
writes. It does not open the Ossia library or write score data.

The adapter registry currently contains:

- Codex: a local marketplace plugin that bundles the Ossia Skill and a stdio MCP server.
- DeepSeek Harness: a local desktop-profile plugin bundle that bundles the Skill and MCP server.

Future Agents can use a plugin, a standalone Skill, or an MCP server by
implementing `AgentAdapter` and registering it in `src/registry.rs`.

## Paths

Host plugin files are shared by ossia and ossia-dev:

- `OSSIA_HOST_INTEGRATION_ROOT` or `~/.ossia/integrations/hosts`
- Codex marketplace: `<root>/codex/marketplace`
- DSH bundle: `<root>/dsh/ossia-dsh-plugin`

Ownership records remain instance-local at `<Ossia home>/integrations/managed.json`
so dev and production data directories keep separate state.

## Lifecycle

`Integrator::agents()` detects hosts and reads live state. `set_agents()` installs
or removes the selected Agents. `sync_managed()` runs in the background on app
start and refreshes only integrations that are already managed and still present
in the host.

Codex copies local plugins into `~/.codex/plugins/cache`, so sync reruns
`codex plugin add` to refresh the cache. DSH links local bundles with pnpm
`link:`, so rewriting the shared bundle is enough.

Conflicts are never overwritten. Removal deletes only hash-matching owned files
and the matching host entries.

## Tests

```sh
cargo test -p ossia-integrations
```

Native host tests are ignored unless real CLIs are supplied:

```sh
OSSIA_TEST_INTEGRATION_ROOT="$PWD/packages/ossia-integrations/resources" \
OSSIA_TEST_OSSIA_BIN="$PWD/target/debug/ossia" \
OSSIA_TEST_CODEX_BIN="$(command -v codex)" \
OSSIA_TEST_DSH_BIN="$(command -v dsh)" \
cargo test -p ossia-integrations --test native_hosts -- --ignored
```

They use temporary host homes and shared roots. They do not prove that a running
host session reloads a plugin; use a new session or restart the host for that.
