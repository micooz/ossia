use super::*;
#[cfg(unix)]
use std::collections::BTreeMap;

#[cfg(unix)]
fn fixture(root: &std::path::Path) -> (Integrator, DetectedAgent) {
    use std::os::unix::fs::PermissionsExt;
    let program = root.join("pi");
    fs::write(&program, "#!/bin/sh\nif [ \"$1\" = mcp ]; then echo 'pi mcp add reads mcp.json'; else echo 1.3.0; fi\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let paths = HostPaths {
        codex_home: root.join("codex"),
        dsh_home: root.join("dsh"),
        workbuddy_home: root.join("user"),
        workbuddy_apps: vec![],
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let integrator =
        Integrator::with_paths(root.join("library"), root.join("ossia"), paths, 3211).unwrap();
    let detected = DetectedAgent {
        executable: program.clone(),
        command: program,
        command_args: vec![],
        version: "1.3.0".into(),
        config_root: root.join("pi-agent"),
        profile: "default",
        env: BTreeMap::new(),
    };
    (integrator, detected)
}
#[cfg(unix)]
fn make_record(entries: Vec<ManagedEntry>) -> ManagedRecord {
    ManagedRecord {
        management_id: "ossia:antigravity:default".into(),
        agent_id: "antigravity".into(),
        method: InstallMethod::Mcp,
        profile: "default".into(),
        template_version: TEMPLATE_VERSION,
        home: "".into(),
        executable: "".into(),
        shared_root: "".into(),
        updated_at: "".into(),
        entries,
    }
}
#[cfg(unix)]
#[test]
fn lifecycle_preserves_unrelated_configuration_and_checks_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let (integrator, detected) = fixture(temp.path());
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    fs::create_dir_all(&detected.config_root).unwrap();
    fs::write(
        config(&ctx),
        br#"{"theme":"dark","mcpServers":{"other":{"command":"other"}}}"#,
    )
    .unwrap();
    assert_eq!(
        Antigravity.status(&ctx, None).unwrap().state,
        AgentState::Available
    );
    let mut record = make_record(Antigravity.install(&ctx, None, false).unwrap());
    assert_eq!(
        Antigravity.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Installed
    );
    assert!(fs::read_to_string(skill_path(&ctx))
        .unwrap()
        .contains("ossia_guide"));
    record.entries = Antigravity.sync(&ctx, &record).unwrap().unwrap();
    let mut value = read(&ctx).unwrap();
    value["theme"] = json!("light");
    record::atomic_write(&config(&ctx), &serde_json::to_vec(&value).unwrap()).unwrap();
    fs::write(skill_path(&ctx), "user edit").unwrap();
    assert_eq!(
        Antigravity.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Conflict
    );
    assert!(Antigravity.remove(&ctx, &record).is_err());
    assert!(Antigravity.install(&ctx, Some(&record), false).is_err());
    record.entries = Antigravity.install(&ctx, Some(&record), true).unwrap();
    Antigravity.remove(&ctx, &record).unwrap();
    let value = read(&ctx).unwrap();
    assert_eq!(value["theme"], "light");
    assert_eq!(value["mcpServers"]["other"]["command"], "other");
    assert!(server(&value).is_none());
    assert!(!skill_path(&ctx).exists());
    Antigravity.remove(&ctx, &record).unwrap();
}
#[cfg(unix)]
#[test]
fn empty_config_can_be_read_and_installed() {
    let temp = tempfile::tempdir().unwrap();
    let (integrator, detected) = fixture(temp.path());
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    fs::create_dir_all(&detected.config_root).unwrap();
    for content in ["", " \t\r\n"] {
        fs::write(config(&ctx), content).unwrap();
        assert_eq!(
            Antigravity.status(&ctx, None).unwrap().state,
            AgentState::Available
        );
        assert_eq!(fs::read_to_string(config(&ctx)).unwrap(), content);
        let mut record = make_record(Antigravity.install(&ctx, None, false).unwrap());
        assert!(server(&read(&ctx).unwrap()).is_some());
        assert_eq!(
            Antigravity.status(&ctx, Some(&record)).unwrap().state,
            AgentState::Installed
        );
        fs::write(config(&ctx), content).unwrap();
        assert_eq!(
            Antigravity.status(&ctx, Some(&record)).unwrap().state,
            AgentState::Broken
        );
        assert_eq!(fs::read_to_string(config(&ctx)).unwrap(), content);
        record.entries = Antigravity.install(&ctx, Some(&record), false).unwrap();
        assert_eq!(
            Antigravity.status(&ctx, Some(&record)).unwrap().state,
            AgentState::Installed
        );
        Antigravity.remove(&ctx, &record).unwrap();
    }
}
#[cfg(unix)]
#[test]
fn invalid_conflicting_and_missing_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let (integrator, detected) = fixture(temp.path());
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    fs::create_dir_all(&detected.config_root).unwrap();
    for content in ["{", "[]", "{\"mcpServers\":[]}", "\u{00a0}", "\u{000b}"] {
        fs::write(config(&ctx), content).unwrap();
        assert!(Antigravity.install(&ctx, None, false).is_err());
        assert_eq!(fs::read_to_string(config(&ctx)).unwrap(), content);
    }
    fs::write(
        config(&ctx),
        br#"{"mcpServers":{"ossia":{"command":"foreign"}}}"#,
    )
    .unwrap();
    assert_eq!(
        Antigravity.status(&ctx, None).unwrap().state,
        AgentState::Conflict
    );
    assert!(Antigravity.install(&ctx, None, false).is_err());
    let record = make_record(Antigravity.install(&ctx, None, true).unwrap());
    fs::remove_file(skill_path(&ctx)).unwrap();
    assert_eq!(
        Antigravity.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Broken
    );
    let record = make_record(Antigravity.install(&ctx, Some(&record), false).unwrap());
    let mut value = read(&ctx).unwrap();
    value["mcpServers"]["ossia"]["command"] = json!("modified");
    record::atomic_write(&config(&ctx), &serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        Antigravity.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Conflict
    );
    assert!(Antigravity.remove(&ctx, &record).is_err());
}

#[cfg(unix)]
#[test]
fn failed_skill_install_restores_the_original_mcp_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let (integrator, detected) = fixture(temp.path());
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    fs::create_dir_all(&detected.config_root).unwrap();
    let original = br#"{"mcpServers":{"other":{"command":"other"}}}"#;
    fs::write(config(&ctx), original).unwrap();
    fs::write(detected.config_root.join("skills"), "not a directory").unwrap();
    assert!(Antigravity.install(&ctx, None, false).is_err());
    assert_eq!(fs::read(config(&ctx)).unwrap(), original);
    assert_eq!(
        fs::read_to_string(detected.config_root.join("skills")).unwrap(),
        "not a directory"
    );
}

#[test]
fn shared_integration_presents_each_detected_host() {
    let host = |name, version: &str| Host {
        name,
        executable: PathBuf::from("host"),
        version: version.into(),
    };
    let app = host("Antigravity", "2.34.0");
    let cli = host("Antigravity CLI", "1.5.15");
    let mut agent = AgentIntegration {
        id: "antigravity".into(),
        name: "Antigravity".into(),
        icon: "antigravity".into(),
        method: InstallMethod::Mcp,
        state: AgentState::Installed,
        error: None,
        installed: true,
        enabled: true,
        managed: true,
        version: None,
        executable: None,
        config_root: String::new(),
        config_path: "shared/mcp_config.json".into(),
        profile: None,
        path: None,
        reload_hint: None,
        message: None,
        hosts: vec![],
        conflicts: vec![],
    };
    present_hosts(&mut agent, std::slice::from_ref(&cli));
    assert_eq!(agent.name, "Antigravity CLI");
    assert_eq!(agent.hosts[0].version.as_deref(), Some("1.5.15"));
    present_hosts(&mut agent, std::slice::from_ref(&app));
    assert_eq!(agent.name, "Antigravity");
    assert_eq!(agent.hosts[0].version.as_deref(), Some("2.34.0"));
    present_hosts(&mut agent, &[app, cli]);
    assert_eq!(agent.name, "Antigravity");
    assert_eq!(agent.hosts.len(), 2);
    assert!(agent
        .hosts
        .iter()
        .all(|h| h.state == AgentState::Installed && h.config_path == "shared/mcp_config.json"));
}
