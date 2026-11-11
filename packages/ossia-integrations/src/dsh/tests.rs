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
fn plugin_package_declares_both_halves_and_absolute_mcp_command() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let files = plugin_files(&ctx).unwrap();
    let package = files
        .iter()
        .find(|file| file.kind == "bundle-manifest")
        .unwrap();
    let package: Value = serde_json::from_slice(&package.bytes).unwrap();
    assert_eq!(package["name"], PLUGIN);
    assert_eq!(package["dsh"]["bundle"]["patch"], "./cordis.patch.yml");
    assert_eq!(package["dsh"]["client"]["platform"], "web");
    assert_eq!(package["engines"]["dsh"], ">=0.2.2-rc.1 <0.3.0");

    let patch = files
        .iter()
        .find(|file| file.kind == "bundle-patch")
        .unwrap();
    let patch = String::from_utf8_lossy(&patch.bytes);
    assert!(patch.contains("@deepseek-ai/dsh-mcp-client"));
    assert!(patch.contains("transport: stdio"));
    assert!(patch.contains(&yaml_string(&integrator.executable().to_string_lossy())));
    assert!(patch.contains("ossia-dsh-plugin/skill"));
}
