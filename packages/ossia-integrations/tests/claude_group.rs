#![cfg(target_os = "macos")]
use ossia_integrations::{HostPaths, Integrator};
use ossia_types::{AgentState, AgentsChange};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt};

#[test]
fn claude_combines_hosts_and_adds_new_host_on_sync() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let app = home.join("Applications/Claude.app/Contents/MacOS/Claude");
    fs::create_dir_all(app.parent().unwrap()).unwrap();
    fs::write(&app, "app").unwrap();
    let cli = temp.path().join("claude-cli");
    let cli_home = temp.path().join("code config");
    std::env::set_var("OSSIA_CLAUDE_BIN", &cli);
    std::env::set_var("CLAUDE_CONFIG_DIR", &cli_home);
    let paths = HostPaths {
        codex_home: temp.path().join("codex"),
        dsh_home: temp.path().join("dsh"),
        workbuddy_home: home.clone(),
        workbuddy_apps: vec![],
        codex_bin: Some(temp.path().join("no-codex")),
        dsh_bin: Some(temp.path().join("no-dsh")),
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        temp.path().join("library"),
        temp.path().join("ossia"),
        paths,
        3212,
    )
    .unwrap();
    let change = |enabled| AgentsChange {
        agent_ids: vec!["claude".into()],
        enabled,
        overwrite: false,
    };
    let first = integrator.set_agents(change(true)).unwrap();
    assert!(first.problems.is_empty());
    let group = first
        .snapshot
        .agents
        .iter()
        .find(|a| a.id == "claude")
        .unwrap();
    assert_eq!(group.state, AgentState::Installed);
    assert_eq!(group.name, "Claude Desktop");
    assert_eq!(group.hosts.len(), 1);
    fs::write(&cli, "#!/bin/sh\ncase \"$1 $2\" in '--version ') echo '2.1.123 (Claude Code)';; 'plugin --help') exit 0;; 'plugin list') echo '[]';; *) exit 1;; esac\n").unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
    let partial = integrator.set_agents(change(true)).unwrap();
    assert_eq!(partial.problems.len(), 1);
    let group = partial
        .snapshot
        .agents
        .iter()
        .find(|a| a.id == "claude")
        .unwrap();
    assert_eq!(group.state, AgentState::Broken);
    assert_eq!(group.hosts.len(), 2);
    assert_eq!(group.name, "Claude");
    assert_eq!(
        group
            .hosts
            .iter()
            .find(|h| h.name == "Claude Desktop")
            .unwrap()
            .state,
        AgentState::Installed
    );
    fs::write(&cli, r##"#!/usr/bin/env python3
import json, os, pathlib, sys
root=pathlib.Path(os.environ['CLAUDE_CONFIG_DIR']); root.mkdir(parents=True, exist_ok=True)
a=sys.argv[1:]; plugin=root/'plugins'; plugin.mkdir(exist_ok=True); known=plugin/'known_marketplaces.json'; installed=root/'list.json'
if a == ['--version']: print('2.1.123 (Claude Code)')
elif a == ['plugin','--help']: print('plugins')
elif a[:2] == ['plugin','list']: print(installed.read_text() if installed.exists() else '[]')
elif a[:3] == ['plugin','marketplace','add']:
 p=a[3]; known.write_text(json.dumps({'ossia-claude':{'source':{'path':p}}}))
elif a[:3] == ['plugin','marketplace','remove']:
 known.unlink(missing_ok=True); installed.unlink(missing_ok=True)
elif a[:2] == ['plugin','install']:
 p=json.loads(known.read_text())['ossia-claude']['source']['path']; manifest=json.loads((pathlib.Path(p)/'.claude-plugin/marketplace.json').read_text()); version=manifest['plugins'][0]['version']; cache=plugin/'cache'; cache.mkdir(exist_ok=True); installed.write_text(json.dumps([{'id':'ossia@ossia-claude','scope':'user','version':version,'enabled':True,'installPath':str(cache)}]))
elif a[:2] == ['plugin','uninstall']: installed.unlink(missing_ok=True)
"##).unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
    let snapshot = integrator.sync_managed().unwrap();
    let group = snapshot.agents.iter().find(|a| a.id == "claude").unwrap();
    assert_eq!(group.hosts.len(), 2);
    assert_eq!(group.name, "Claude");
    assert_eq!(group.state, AgentState::Installed);
    assert_eq!(
        group
            .hosts
            .iter()
            .find(|h| h.name == "Claude Code")
            .unwrap()
            .version
            .as_deref(),
        Some("2.1.123")
    );
    assert_eq!(
        snapshot.agents.iter().filter(|a| a.id == "claude").count(),
        1
    );
    let list_path = cli_home.join("list.json");
    let saved_cli = fs::read(&cli).unwrap();
    let saved_list = fs::read(&list_path).unwrap();
    fs::write(
        &cli,
        "#!/bin/sh\necho 'claude startup failed' >&2\nexit 1\n",
    )
    .unwrap();
    let failed = integrator.sync_managed().unwrap();
    let group = failed.agents.iter().find(|a| a.id == "claude").unwrap();
    assert_eq!(group.state, AgentState::Broken);
    assert!(group.error.is_none());
    let code = group
        .hosts
        .iter()
        .find(|host| host.name == "Claude Code")
        .unwrap();
    assert_eq!(code.state, AgentState::Unavailable);
    assert!(code
        .error
        .as_deref()
        .unwrap()
        .contains("claude startup failed"));
    assert_eq!(
        group
            .hosts
            .iter()
            .find(|host| host.name == "Claude Desktop")
            .unwrap()
            .state,
        AgentState::Installed
    );
    assert_eq!(fs::read(&list_path).unwrap(), saved_list);
    fs::write(&cli, saved_cli).unwrap();
    let mut installed: serde_json::Value =
        serde_json::from_slice(&fs::read(&list_path).unwrap()).unwrap();
    installed[0]["enabled"] = false.into();
    fs::write(&list_path, serde_json::to_vec(&installed).unwrap()).unwrap();
    let disabled = integrator.sync_managed().unwrap();
    let group = disabled.agents.iter().find(|a| a.id == "claude").unwrap();
    assert_eq!(group.state, AgentState::Broken);
    assert_eq!(
        group
            .hosts
            .iter()
            .find(|h| h.name == "Claude Code")
            .unwrap()
            .state,
        AgentState::Disabled
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&list_path).unwrap()).unwrap()[0]
            ["enabled"],
        false
    );
    let removed = integrator.set_agents(change(false)).unwrap();
    assert!(removed.problems.is_empty(), "{:?}", removed.problems);
    assert_eq!(
        integrator
            .sync_managed()
            .unwrap()
            .agents
            .iter()
            .find(|a| a.id == "claude")
            .unwrap()
            .state,
        AgentState::Available
    );
    assert!(
        !home
            .join("Library/Application Support/Claude/claude_desktop_config.json")
            .as_path()
            .exists()
            || !fs::read_to_string(
                home.join("Library/Application Support/Claude/claude_desktop_config.json")
            )
            .unwrap()
            .contains("ossia")
    );
    assert!(!cli_home.join("list.json").exists());
}
