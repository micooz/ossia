use super::*;
use crate::{HostPaths, Integrator};
use ossia_types::DEFAULT_BACKEND_PORT;
use std::{collections::BTreeMap, fs};

fn fixture() -> (tempfile::TempDir, Integrator, DetectedAgent) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("library");
    let shared = root.path().join("shared");
    let config_root = root.path().join("home/.claude");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&shared).unwrap();
    fs::create_dir_all(&config_root).unwrap();
    let executable = root.path().join("ossia");
    fs::write(&executable, b"test executable").unwrap();
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: root.path().join("home"),
        workbuddy_apps: Vec::new(),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::from([(
            "OSSIA_ENGINE_ROOT".into(),
            format!(r"\\?\{}", root.path().join("engine").display()),
        )]),
    };
    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let integrator =
        Integrator::with_resource_dir(home, executable, resources, paths, DEFAULT_BACKEND_PORT)
            .unwrap()
            .with_shared_root(shared)
            .unwrap();
    let detected = DetectedAgent {
        executable: root.path().join("codebuddy"),
        command: root.path().join("node"),
        command_args: vec![path_text(root.path().join("codebuddy"))],
        version: "5.11.3 (CLI 2.176.0)".into(),
        config_root,
        profile: "desktop",
        env: BTreeMap::new(),
    };
    (root, integrator, detected)
}

#[test]
fn plugin_files_include_marketplace_skill_and_mcp() {
    let (_root, integrator, detected) = fixture();
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let (version, files) = plugin_files(&ctx).unwrap();
    assert!(version.starts_with(concat!(env!("CARGO_PKG_VERSION"), "+ossia.")));
    assert_eq!(files.len(), 9);
    let marketplace = files
        .iter()
        .find(|file| file.kind == "marketplace")
        .unwrap();
    assert!(marketplace
        .path
        .ends_with(".claude-plugin/marketplace.json"));
    let marketplace: Value = serde_json::from_slice(&marketplace.bytes).unwrap();
    assert_eq!(marketplace["name"], MARKETPLACE);
    assert_eq!(marketplace["plugins"][0]["name"], PLUGIN);
    assert_eq!(marketplace["plugins"][0]["version"], version);

    let manifest = files
        .iter()
        .find(|file| file.kind == "plugin-manifest")
        .unwrap();
    assert_eq!(manifest.path, plugin_manifest_path(&ctx));
    let manifest: Value = serde_json::from_slice(&manifest.bytes).unwrap();
    assert_eq!(manifest["name"], PLUGIN);
    assert_eq!(manifest["version"], version);
    assert_eq!(manifest["mcpServers"], "./.mcp.json");

    let mcp = files.iter().find(|file| file.kind == "mcp").unwrap();
    assert_eq!(mcp.path, plugin_dir(&ctx).join(".mcp.json"));
    let mcp: Value = serde_json::from_slice(&mcp.bytes).unwrap();
    assert_eq!(
        mcp["mcpServers"]["ossia"]["command"],
        json!(path_text(integrator.executable()))
    );
    assert_eq!(
        mcp["mcpServers"]["ossia"]["args"],
        json!([
            "mcp",
            "--home",
            path_text(integrator.home()),
            "--port",
            DEFAULT_BACKEND_PORT.to_string()
        ])
    );
    assert!(!mcp.to_string().contains(r"\\?\"));
    assert_eq!(
        files
            .iter()
            .filter(|file| file.kind == "skill-reference")
            .count(),
        4
    );
    assert!(files
        .iter()
        .all(|file| file.path.starts_with(integrator.shared_root())));
}

#[cfg(unix)]
#[path = "lifecycle.rs"]
mod lifecycle_tests;
