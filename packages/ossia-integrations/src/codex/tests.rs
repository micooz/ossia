use super::*;

fn fixture() -> (tempfile::TempDir, Integrator) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::HostPaths {
        codex_home: root.path().join("codex"),
        agents_home: root.path().join("agents"),
        dsh_home: root.path().join("dsh"),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../integrations");
    let integrator = Integrator::with_paths(
        root.path().join("library"),
        std::env::current_exe().unwrap(),
        resources,
        paths,
    )
    .unwrap();
    (root, integrator)
}

#[test]
fn plugin_files_are_self_contained_and_point_to_the_absolute_runtime() {
    let (_root, integrator) = fixture();
    let files = plugin_files(&integrator).unwrap();
    assert_eq!(files.len(), 5);

    let marketplace = files
        .iter()
        .find(|file| file.kind == "marketplace")
        .unwrap();
    let marketplace: Value = serde_json::from_slice(&marketplace.bytes).unwrap();
    assert_eq!(marketplace["name"], MARKETPLACE);
    assert_eq!(
        marketplace["plugins"][0]["source"]["path"],
        "./plugins/ossia"
    );

    let mcp = files.iter().find(|file| file.kind == "mcp").unwrap();
    let mcp: Value = serde_json::from_slice(&mcp.bytes).unwrap();
    assert_eq!(
        mcp["mcpServers"]["ossia"]["command"],
        json!(integrator.executable().to_string_lossy())
    );
    assert_eq!(
        mcp["mcpServers"]["ossia"]["args"],
        json!(["mcp", "--home", integrator.home().to_string_lossy()])
    );
    assert!(files
        .iter()
        .any(|file| file.kind == "skill" && file.path.ends_with("skills/ossia/SKILL.md")));
}

#[test]
fn mcp_matching_includes_args_and_environment() {
    let desired = DesiredMcp {
        command: "/bin/ossia".into(),
        args: vec!["mcp".into(), "--home".into(), "/tmp/a".into()],
        env: BTreeMap::from([("OSSIA_ENGINE_ROOT".into(), "/tmp/engine".into())]),
    };
    let entry = McpEntry {
        name: "ossia".into(),
        enabled: true,
        transport: McpTransport {
            command: Some(desired.command.clone()),
            args: Some(desired.args.clone()),
            env: Some(desired.env.clone()),
        },
    };
    assert!(mcp_matches(&entry, &desired));
    let mut mismatch = entry;
    mismatch.transport.env = Some(BTreeMap::new());
    assert!(!mcp_matches(&mismatch, &desired));
}
