#[cfg(unix)]
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
        management_id: "ossia:pi:default".into(),
        agent_id: "pi".into(),
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
    assert_eq!(Pi.status(&ctx, None).unwrap().state, AgentState::Available);
    let mut record = make_record(Pi.install(&ctx, None, false).unwrap());
    assert_eq!(
        Pi.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Installed
    );
    assert!(fs::read_to_string(skill_path(&ctx))
        .unwrap()
        .contains("ossia_guide"));
    record.entries = Pi.sync(&ctx, &record).unwrap().unwrap();
    let mut value = read(&ctx).unwrap();
    value["theme"] = json!("light");
    record::atomic_write(&config(&ctx), &serde_json::to_vec(&value).unwrap()).unwrap();
    fs::write(skill_path(&ctx), "user edit").unwrap();
    assert_eq!(
        Pi.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Conflict
    );
    assert!(Pi.remove(&ctx, &record).is_err());
    assert!(Pi.install(&ctx, Some(&record), false).is_err());
    record.entries = Pi.install(&ctx, Some(&record), true).unwrap();
    Pi.remove(&ctx, &record).unwrap();
    let value = read(&ctx).unwrap();
    assert_eq!(value["theme"], "light");
    assert_eq!(value["mcpServers"]["other"]["command"], "other");
    assert!(server(&value).is_none());
    assert!(!skill_path(&ctx).exists());
    Pi.remove(&ctx, &record).unwrap();
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
        assert_eq!(Pi.status(&ctx, None).unwrap().state, AgentState::Available);
        assert_eq!(fs::read_to_string(config(&ctx)).unwrap(), content);
        let record = make_record(Pi.install(&ctx, None, false).unwrap());
        assert!(server(&read(&ctx).unwrap()).is_some());
        assert_eq!(
            Pi.status(&ctx, Some(&record)).unwrap().state,
            AgentState::Installed
        );
        Pi.remove(&ctx, &record).unwrap();
    }
}
#[cfg(unix)]
#[test]
fn invalid_conflicting_missing_and_unsupported_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let (integrator, detected) = fixture(temp.path());
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    fs::create_dir_all(&detected.config_root).unwrap();
    for content in ["{", "[]", "{\"mcpServers\":[]}", "\u{00a0}", "\u{000b}"] {
        fs::write(config(&ctx), content).unwrap();
        assert!(Pi.install(&ctx, None, false).is_err());
        assert_eq!(fs::read_to_string(config(&ctx)).unwrap(), content);
    }
    fs::write(
        config(&ctx),
        br#"{"mcpServers":{"ossia":{"command":"foreign"}}}"#,
    )
    .unwrap();
    assert_eq!(Pi.status(&ctx, None).unwrap().state, AgentState::Conflict);
    assert!(Pi.install(&ctx, None, false).is_err());
    let record = make_record(Pi.install(&ctx, None, true).unwrap());
    fs::remove_file(skill_path(&ctx)).unwrap();
    assert_eq!(
        Pi.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Broken
    );
    let record = make_record(Pi.install(&ctx, Some(&record), false).unwrap());
    let mut value = read(&ctx).unwrap();
    value["mcpServers"]["ossia"]["command"] = json!("modified");
    record::atomic_write(&config(&ctx), &serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        Pi.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Conflict
    );
    assert!(Pi.remove(&ctx, &record).is_err());
    fs::write(&detected.command, "#!/bin/sh\necho unsupported\n").unwrap();
    assert_eq!(
        Pi.status(&ctx, Some(&record)).unwrap().state,
        AgentState::UpdateRequired
    );
    assert!(Pi.install(&ctx, Some(&record), true).is_err());
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
    assert!(Pi.install(&ctx, None, false).is_err());
    assert_eq!(fs::read(config(&ctx)).unwrap(), original);
    assert_eq!(
        fs::read_to_string(detected.config_root.join("skills")).unwrap(),
        "not a directory"
    );
}

#[cfg(unix)]
#[test]
fn failed_pi_command_reports_its_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let (integrator, detected) = fixture(temp.path());
    fs::write(
        &detected.command,
        "#!/bin/sh\necho 'env: node: No such file or directory' >&2\nexit 127\n",
    )
    .unwrap();
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let error = Pi
        .status(&ctx, None)
        .expect_err("startup failure must be reported");
    assert!(error.message.contains("node: No such file or directory"));
    assert!(Pi
        .install(&ctx, None, false)
        .unwrap_err()
        .message
        .contains("node: No such file or directory"));
}

#[cfg(unix)]
#[test]
fn pi_recovers_node_from_the_users_shell_path() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let (integrator, mut detected) = fixture(temp.path());
    let bin = temp.path().join("runtime bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(
        bin.join("node"),
        "#!/bin/sh\necho 'pi mcp add reads mcp.json'\n",
    )
    .unwrap();
    fs::set_permissions(bin.join("node"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&detected.command, "#!/bin/sh\nexec node\n").unwrap();
    let shell = temp.path().join("shell");
    fs::write(
        &shell,
        format!(
            "#!/bin/sh\nprintf '\\nOSSIA_SHELL_PATH_BEGIN\\n%s\\nOSSIA_SHELL_PATH_END\\n' '{}'\n",
            bin.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o755)).unwrap();
    detected.env.insert(
        "PATH".into(),
        temp.path().join("empty").to_string_lossy().into_owned(),
    );
    let resolver = command::AgentEnvironment::with_path_loader(move || {
        command::shell_path(&shell, &BTreeMap::new()).ok()
    });
    let (env, _) = resolver
        .probe(&detected.command, detected.env.clone(), &[], false)
        .unwrap();
    detected.env = env;
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    assert_eq!(Pi.status(&ctx, None).unwrap().state, AgentState::Available);
    let record = make_record(Pi.install(&ctx, None, false).unwrap());
    assert_eq!(
        Pi.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Installed
    );
}

#[cfg(unix)]
#[test]
fn pi_keeps_original_environment_when_shell_path_does_not_help() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let (_, detected) = fixture(temp.path());
    fs::write(
        &detected.command,
        "#!/bin/sh\necho 'original failure' >&2\nexit 1\n",
    )
    .unwrap();
    let shell = temp.path().join("shell");
    fs::write(
        &shell,
        "#!/bin/sh\nprintf '\\nOSSIA_SHELL_PATH_BEGIN\\n/other/bin\\nOSSIA_SHELL_PATH_END\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o755)).unwrap();
    let env = BTreeMap::from([("PATH".to_string(), "/original/bin".to_string())]);
    let resolver = command::AgentEnvironment::with_path_loader(move || {
        command::shell_path(&shell, &BTreeMap::new()).ok()
    });
    assert!(resolver
        .probe(&detected.command, env, &[], false)
        .unwrap_err()
        .message
        .contains("original failure"));
}
