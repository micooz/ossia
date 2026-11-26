use super::*;
use crate::{HostPaths, Integrator};
use ossia_types::DEFAULT_BACKEND_PORT;
use std::{collections::BTreeMap, fs};

fn fixture() -> (tempfile::TempDir, Integrator, DetectedAgent) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("library");
    let shared = root.path().join("shared");
    let config_root = root.path().join("home/.workbuddy");
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
        .ends_with(".codebuddy-plugin/marketplace.json"));
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

#[test]
fn plugin_version_is_stable_and_binding_sensitive() {
    let (_root, integrator, detected) = fixture();
    let first = {
        let ctx = AgentContext {
            integrator: &integrator,
            detected: &detected,
        };
        plugin_files(&ctx).unwrap().0
    };
    let second = {
        let ctx = AgentContext {
            integrator: &integrator,
            detected: &detected,
        };
        plugin_files(&ctx).unwrap().0
    };
    assert_eq!(first, second);
    let changed = plugin_version(
        b"skill",
        b"piano",
        b"single",
        b"guitar",
        b"jianpu",
        b"icon",
        &json!({"command": "/different/ossia"}),
    );
    let unchanged = plugin_version(
        b"skill",
        b"piano",
        b"single",
        b"guitar",
        b"jianpu",
        b"icon",
        &json!({"command": "/original/ossia"}),
    );
    assert_ne!(unchanged, changed);
    assert_ne!(first, changed);
}

#[cfg(target_os = "macos")]
#[test]
fn duplicate_config_roots_select_the_newer_app() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let older = fake_app(root.path(), "WorkBuddy.app", "5.10.0", ".workbuddy");
    let newer = fake_app(root.path(), "WorkBuddyAI.app", "5.11.3", ".workbuddy");
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: home.clone(),
        workbuddy_apps: vec![older, newer],
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let instances = instances(&paths);
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].instance.app_version, "5.11.3");
    assert_eq!(instances[0].instance.config_root, home.join(".workbuddy"));
}

#[cfg(target_os = "macos")]
fn fake_app(root: &Path, name: &str, version: &str, data_folder: &str) -> PathBuf {
    let app = root.join(name);
    let cli = app.join("Contents/Resources/app.asar.unpacked/cli");
    fs::create_dir_all(cli.join("bin")).unwrap();
    fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    fs::write(app.join("Contents/MacOS/Electron"), b"app").unwrap();
    fs::write(cli.join("bin/codebuddy"), b"cli").unwrap();
    fs::write(
        cli.join("product.json"),
        serde_json::to_vec(&json!({
            "applicationName": "WorkBuddy",
            "dataFolderName": data_folder,
            "genieVersion": version
        }))
        .unwrap(),
    )
    .unwrap();
    app
}

#[cfg(unix)]
#[path = "lifecycle.rs"]
mod lifecycle_tests;
