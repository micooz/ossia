use super::*;
use std::collections::BTreeMap;

#[cfg(unix)]
#[test]
fn invalid_host_config_does_not_block_agent_list() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("user");
    let cli = home.join(".gemini/antigravity-cli/bin/agy");
    std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
    std::fs::write(&cli, "#!/bin/sh\nif [ \"$1\" = plugin ] || [ \"$1\" = mcp ]; then echo '{}'; else echo 1.0.0; fi\n").unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = home.join(".gemini/config/mcp_config.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: home,
        workbuddy_apps: Vec::new(),
        codex_bin: Some(cli),
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        root.path().join("ossia"),
        root.path().join("bin"),
        paths,
        3211,
    )
    .unwrap();
    {
        let content = "{";
        std::fs::write(&config, content).unwrap();
        let snapshot = integrator.agents().unwrap();
        assert!(snapshot.agents.iter().any(|agent| agent.id == "codex"));
        let broken = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == "antigravity")
            .unwrap();
        assert_eq!(broken.state, AgentState::Unavailable);
        let error = broken.error.as_ref().unwrap();
        assert!(error.contains("invalid Antigravity MCP configuration"));
        assert!(error.contains("mcp_config.json"));
        assert!(snapshot.error.is_none());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), content);
    }
    std::fs::write(&config, "{}").unwrap();
    let snapshot = integrator.agents().unwrap();
    assert!(snapshot.error.is_none(), "{:?}", snapshot.error);
    assert_eq!(
        snapshot
            .agents
            .iter()
            .find(|agent| agent.id == "antigravity")
            .unwrap()
            .state,
        AgentState::Available
    );
    let installed = integrator
        .set_agents(AgentsChange {
            agent_ids: vec!["antigravity".into()],
            enabled: true,
            overwrite: false,
        })
        .unwrap();
    assert!(installed.problems.is_empty());
    assert!(integrator.forget_agent("antigravity").is_err());
    let skill = config.parent().unwrap().join("skills/ossia/SKILL.md");
    let skill_before = std::fs::read(&skill).unwrap();
    std::fs::write(&config, "{").unwrap();
    let result = integrator.forget_agent("antigravity").unwrap();
    let agent = result
        .agents
        .iter()
        .find(|agent| agent.id == "antigravity")
        .unwrap();
    assert!(!agent.managed);
    assert!(agent.error.is_some());
    assert!(!result.connection_order.contains(&"antigravity".into()));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "{");
    assert_eq!(std::fs::read(skill).unwrap(), skill_before);
    assert!(integrator.forget_agent("antigravity").is_err());
}

#[cfg(unix)]
#[test]
fn scans_agents_concurrently() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let codex = root.path().join("codex-cli");
    let dsh = root.path().join("dsh-cli");
    for (program, own, other) in [
        (&codex, "codex-ready", "dsh-ready"),
        (&dsh, "dsh-ready", "codex-ready"),
    ] {
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then\n  touch '{}'
  i=0; while [ ! -f '{}' ]; do i=$((i+1)); [ \"$i\" -lt 100 ] || exit 1; sleep 0.05; done
  echo 1.0\nelse echo '{{\"installed\":[],\"marketplaces\":[],\"servers\":[]}}'; fi\n",
            root.path().join(own).display(),
            root.path().join(other).display()
        );
        std::fs::write(program, script).unwrap();
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: root.path().join("user"),
        workbuddy_apps: Vec::new(),
        codex_bin: Some(codex),
        dsh_bin: Some(dsh),
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        root.path().join("ossia"),
        root.path().join("bin"),
        paths,
        3211,
    )
    .unwrap();
    let snapshot = integrator.agents().unwrap();
    assert!(snapshot.agents.iter().any(|agent| agent.id == "codex"));
    assert!(snapshot.agents.iter().any(|agent| agent.id == "dsh"));
}

#[test]
fn claude_name_reflects_detected_hosts() {
    let host = |name: &str, state| AgentHost {
        name: name.into(),
        state,
        error: None,
        method: InstallMethod::Mcp,
        version: None,
        config_path: String::new(),
        reload_hint: None,
        conflicts: Vec::new(),
    };
    let desktop = host("Claude Desktop", AgentState::Installed);
    let code = host("Claude Code", AgentState::Available);
    assert_eq!(
        claude_name(std::slice::from_ref(&desktop)),
        "Claude Desktop"
    );
    assert_eq!(claude_name(std::slice::from_ref(&code)), "Claude Code");
    assert_eq!(claude_name(&[desktop.clone(), code.clone()]), "Claude");
    let unavailable = host("Claude Desktop", AgentState::Unavailable);
    assert_eq!(claude_name(&[unavailable.clone(), code]), "Claude Code");
    assert_eq!(claude_name(&[unavailable]), "Claude Desktop");
}

#[test]
fn shared_integration_root_follows_the_runtime_home() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join(".ossia");
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: root.path().join("home"),
        workbuddy_apps: Vec::new(),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        home.clone(),
        std::env::current_exe().unwrap(),
        paths,
        DEFAULT_BACKEND_PORT,
    )
    .unwrap();
    assert_eq!(integrator.shared_root(), home.join("integrations/hosts"));
}

#[test]
fn launch_environment_values_are_normalized() {
    let root = tempfile::tempdir().unwrap();
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: root.path().join("home"),
        workbuddy_apps: Vec::new(),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::from([(
            "OSSIA_ENGINE_ROOT".into(),
            r"\\?\E:\Projects\ossia\engine".into(),
        )]),
    };
    let integrator = Integrator::with_paths(
        root.path().join(".ossia"),
        std::env::current_exe().unwrap(),
        paths,
        DEFAULT_BACKEND_PORT,
    )
    .unwrap();

    assert_eq!(
        integrator
            .launch_env()
            .get("OSSIA_ENGINE_ROOT")
            .map(String::as_str),
        Some(r"E:\Projects\ossia\engine")
    );
}

#[test]
fn mcp_command_uses_the_integrator_port() {
    let root = tempfile::tempdir().unwrap();
    let integrator = Integrator::with_paths(
        root.path().join(".ossia"),
        std::env::current_exe().unwrap(),
        HostPaths {
            codex_home: root.path().join("codex"),
            dsh_home: root.path().join("dsh"),
            workbuddy_home: root.path().join("home"),
            workbuddy_apps: Vec::new(),
            codex_bin: None,
            dsh_bin: None,
            launch_env: BTreeMap::new(),
        },
        3211,
    )
    .unwrap();

    assert_eq!(
        integrator.mcp_command(),
        vec![
            "mcp".to_string(),
            "--home".to_string(),
            path_text(integrator.home()),
            "--port".to_string(),
            "3211".to_string()
        ]
    );
}

#[test]
fn manual_integration_uses_instance_configuration_and_rendered_skill() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("data with spaces");
    let executable = std::env::current_exe().unwrap();
    let mut paths = HostPaths::detected();
    paths
        .launch_env
        .insert("OSSIA_ENGINE_ROOT".into(), "/engine with spaces".into());
    let integrator = Integrator::with_paths(&home, &executable, paths, 4321).unwrap();
    let manual = integrator.manual_integration().unwrap();
    assert_eq!(manual.command, path_text(executable));
    assert_eq!(
        manual.args,
        vec!["mcp", "--home", &path_text(&home), "--port", "4321"]
    );
    assert_eq!(
        manual.env.get("OSSIA_ENGINE_ROOT").unwrap(),
        "/engine with spaces"
    );
    let skill = std::fs::read_to_string(&manual.skill_path).unwrap();
    assert!(!skill.contains("{{OSSIA_CLI_GUIDE}}"));
    assert!(skill.contains(&path_text(&home)));
    assert!(std::path::Path::new(&manual.skill_path)
        .parent()
        .unwrap()
        .join("references/piano.ly")
        .is_file());
}
