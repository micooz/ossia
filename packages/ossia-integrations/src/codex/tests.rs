use super::*;
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, Integrator) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let integrator = Integrator::with_resource_dir(
        root.path().join("library"),
        std::env::current_exe().unwrap(),
        resources,
        paths,
    )
    .unwrap()
    .with_shared_root(root.path().join("shared"))
    .unwrap();
    (root, integrator)
}

#[test]
fn plugin_files_point_to_the_absolute_runtime_and_shared_root() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().codex_home.clone(),
        profile: "default",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let files = plugin_files(&ctx).unwrap();
    assert_eq!(files.len(), 5);
    let mcp = files.iter().find(|file| file.kind == "mcp").unwrap();
    let mcp: Value = serde_json::from_slice(&mcp.bytes).unwrap();
    assert_eq!(
        mcp["mcpServers"]["ossia"]["command"],
        json!(integrator.executable().to_string_lossy())
    );
    assert!(files
        .iter()
        .any(|file| { file.kind == "skill" && file.path.ends_with("skills/ossia/SKILL.md") }));
    assert!(files
        .iter()
        .all(|file| file.path.starts_with(integrator.shared_root())));
}
