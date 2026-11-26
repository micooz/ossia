use super::*;
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, Integrator) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("library");
    let shared = root.path().join("shared");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&shared).unwrap();
    let executable = root.path().join("ossia.exe");
    fs::write(&executable, b"test executable").unwrap();
    let paths = crate::HostPaths {
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
    (root, integrator)
}

#[test]
fn plugin_files_point_to_the_absolute_runtime_and_shared_root() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        command: std::env::current_exe().unwrap(),
        command_args: Vec::new(),
        version: "test".into(),
        config_root: integrator.paths().codex_home.clone(),
        profile: "default",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    assert_eq!(
        Codex.config_path(&ctx),
        integrator.paths().codex_home.join("config.toml")
    );
    let files = plugin_files(&ctx).unwrap();
    assert_eq!(files.len(), 9);
    let manifest = files
        .iter()
        .find(|file| file.kind == "plugin-manifest")
        .unwrap();
    assert!(manifest.path.ends_with(".codex-plugin/plugin.json"));
    let manifest: Value = serde_json::from_slice(&manifest.bytes).unwrap();
    assert_eq!(
        manifest["extensions"]["com.openai"]["interface"]["composerIcon"],
        "./assets/ossia.svg"
    );
    assert_eq!(
        manifest["extensions"]["com.openai"]["interface"]["logo"],
        "./assets/ossia.svg"
    );
    let icon = files.iter().find(|file| file.kind == "asset").unwrap();
    assert_eq!(
        icon.path,
        plugin_dir(integrator.shared_root()).join("assets/ossia.svg")
    );
    assert!(!files
        .iter()
        .any(|file| file.path == plugin_dir(integrator.shared_root()).join("plugin.json")));
    let mcp = files.iter().find(|file| file.kind == "mcp").unwrap();
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
    assert!(files
        .iter()
        .any(|file| { file.kind == "skill" && file.path.ends_with("skills/ossia/SKILL.md") }));
    assert_eq!(
        files
            .iter()
            .filter(|file| file.kind == "skill-reference")
            .count(),
        4
    );
    for name in ["piano.ly", "single-staff.ly", "guitar-tab.ly", "jianpu.ly"] {
        assert!(files.iter().any(|file| {
            file.kind == "skill-reference"
                && file
                    .path
                    .ends_with(format!("skills/ossia/references/{name}"))
        }));
    }
    assert!(files
        .iter()
        .all(|file| file.path.starts_with(integrator.shared_root())));
}

#[cfg(unix)]
#[test]
fn plugin_status_only_lists_the_owned_marketplace() {
    use std::os::unix::fs::PermissionsExt;

    let (root, integrator) = fixture();
    let host = root.path().join("codex");
    let args = root.path().join("arguments.txt");
    std::fs::write(
        &host,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$OSSIA_TEST_ARGS\"\nprintf '{\"installed\":[]}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&host, std::fs::Permissions::from_mode(0o755)).unwrap();
    let detected = DetectedAgent {
        command: host.clone(),
        command_args: Vec::new(),
        executable: host,
        version: "test".into(),
        config_root: integrator.paths().codex_home.clone(),
        profile: "default",
        env: BTreeMap::from([(
            "OSSIA_TEST_ARGS".into(),
            args.to_string_lossy().into_owned(),
        )]),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };

    assert!(plugin_entry(&ctx).unwrap().is_none());
    assert_eq!(
        std::fs::read_to_string(args).unwrap(),
        "plugin\nlist\n--json\n--marketplace\nossia-local\n"
    );
}

#[cfg(unix)]
#[path = "status.rs"]
mod status_tests;
