use super::*;
use std::collections::BTreeMap;

#[cfg(target_os = "macos")]
#[test]
fn reads_desktop_version_from_the_app_bundle() {
    let root = tempfile::tempdir().unwrap();
    let contents = root.path().join("Claude.app/Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    let executable = contents.join("MacOS/Claude");
    let info = plist::Value::Dictionary(plist::Dictionary::from_iter([(
        String::from("CFBundleShortVersionString"),
        plist::Value::String("1.25927.0".into()),
    )]));
    info.to_file_xml(contents.join("Info.plist")).unwrap();
    assert_eq!(app_version(&executable).as_deref(), Some("1.25927.0"));
    assert_eq!(app_version(&root.path().join("missing")), None);
}

#[test]
fn desktop_preserves_other_configuration_and_detects_modified_entry() {
    let root = tempfile::tempdir().unwrap();
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: root.path().join("user"),
        workbuddy_apps: vec![],
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        root.path().join("library"),
        root.path().join("ossia"),
        paths,
        3211,
    )
    .unwrap();
    let detected = DetectedAgent {
        executable: root.path().join("Claude"),
        command: root.path().join("Claude"),
        command_args: vec![],
        version: "1".into(),
        config_root: root.path().join("config"),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let path = config(&ctx);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        br#"{"theme":"dark","mcpServers":{"other":{"command":"other"}}}"#,
    )
    .unwrap();
    let entries = ClaudeDesktop.install(&ctx, None, false).unwrap();
    let record = ManagedRecord {
        management_id: "test".into(),
        agent_id: "claude".into(),
        method: InstallMethod::Mcp,
        profile: "desktop".into(),
        template_version: TEMPLATE_VERSION,
        home: "".into(),
        executable: "".into(),
        shared_root: "".into(),
        updated_at: "".into(),
        entries,
    };
    let mut value = read(&path).unwrap();
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["mcpServers"]["other"]["command"], "other");
    value["theme"] = json!("light");
    write(&path, &value).unwrap();
    assert!(ClaudeDesktop.sync(&ctx, &record).unwrap().is_some());
    value = read(&path).unwrap();
    value["mcpServers"]["ossia"]["command"] = json!("foreign");
    write(&path, &value).unwrap();
    assert_eq!(
        ClaudeDesktop.status(&ctx, Some(&record)).unwrap().state,
        AgentState::Conflict
    );
    assert!(ClaudeDesktop.remove(&ctx, &record).is_err());
    assert_eq!(
        read(&path).unwrap()["mcpServers"]["ossia"]["command"],
        "foreign"
    );
    assert!(ClaudeDesktop.install(&ctx, Some(&record), false).is_err());
    let entries = ClaudeDesktop.install(&ctx, Some(&record), true).unwrap();
    let record = ManagedRecord { entries, ..record };
    ClaudeDesktop.remove(&ctx, &record).unwrap();
    let value = read(&path).unwrap();
    assert_eq!(value["theme"], "light");
    assert_eq!(value["mcpServers"]["other"]["command"], "other");
    assert!(server(&value).is_none());
}

#[test]
fn desktop_empty_config_can_be_read_and_installed() {
    let root = tempfile::tempdir().unwrap();
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: root.path().join("user"),
        workbuddy_apps: vec![],
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        root.path().join("library"),
        root.path().join("ossia"),
        paths,
        3211,
    )
    .unwrap();
    let detected = DetectedAgent {
        executable: root.path().join("Claude"),
        command: root.path().join("Claude"),
        command_args: vec![],
        version: "1".into(),
        config_root: root.path().join("config"),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let path = config(&ctx);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    for content in ["", " \t\r\n"] {
        fs::write(&path, content).unwrap();
        assert_eq!(
            ClaudeDesktop.status(&ctx, None).unwrap().state,
            AgentState::Available
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        let entries = ClaudeDesktop.install(&ctx, None, false).unwrap();
        assert!(server(&read(&path).unwrap()).is_some());
        let record = ManagedRecord {
            management_id: "test".into(),
            agent_id: "claude".into(),
            method: InstallMethod::Mcp,
            profile: "desktop".into(),
            template_version: TEMPLATE_VERSION,
            home: "".into(),
            executable: "".into(),
            shared_root: "".into(),
            updated_at: "".into(),
            entries,
        };
        ClaudeDesktop.remove(&ctx, &record).unwrap();
    }
}

#[test]
fn desktop_rejects_invalid_configuration() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    for content in ["{", "[]", "{\"mcpServers\":[]}", "\u{00a0}", "\u{000b}"] {
        fs::write(&path, content).unwrap();
        assert!(read(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }
}
