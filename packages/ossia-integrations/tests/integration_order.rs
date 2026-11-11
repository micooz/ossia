#![cfg(unix)]

use ossia_integrations::{HostPaths, Integrator};
use ossia_types::AgentsChange;
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt};

#[test]
fn connection_order_survives_sync_reopen_and_reinstallation() {
    let root = tempfile::tempdir().unwrap();
    let host = root.path().join("host");
    fs::write(
        &host,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'host 1.0'; else echo '{\"installed\":[],\"marketplaces\":[]}'; fi\n",
    )
    .unwrap();
    fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        codex_bin: Some(host.clone()),
        dsh_bin: Some(host),
        launch_env: BTreeMap::new(),
    };
    let home = root.path().join("library");
    let executable = std::env::current_exe().unwrap();
    let integrator = Integrator::with_paths(&home, &executable, paths.clone()).unwrap();
    let change = |id: &str, enabled| {
        let result = integrator
            .set_agents(AgentsChange {
                agent_ids: vec![id.into()],
                enabled,
            })
            .unwrap();
        assert!(result.problems.is_empty(), "{:?}", result.problems);
        result.snapshot.connection_order
    };
    assert_eq!(change("dsh", true), ["dsh"]);
    assert_eq!(change("codex", true), ["dsh", "codex"]);
    assert_eq!(change("dsh", true), ["dsh", "codex"]);
    assert_eq!(
        integrator.sync_managed().unwrap().connection_order,
        ["dsh", "codex"]
    );
    let reopened = Integrator::with_paths(&home, executable, paths).unwrap();
    assert_eq!(
        reopened.agents().unwrap().connection_order,
        ["dsh", "codex"]
    );
    assert_eq!(change("dsh", false), ["codex"]);
    assert_eq!(change("dsh", true), ["codex", "dsh"]);
    let failed = integrator
        .set_agents(AgentsChange {
            agent_ids: vec!["unknown".into()],
            enabled: true,
        })
        .unwrap();
    assert_eq!(failed.problems.len(), 1);
    assert_eq!(failed.snapshot.connection_order, ["codex", "dsh"]);
}
