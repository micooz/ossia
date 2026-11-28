use super::*;
use serde_json::json;
use std::fs;

#[test]
fn installed_cli_defaults_to_port_3212() {
    let args = Args::try_parse_from(["ossia", "status"]).unwrap();
    assert_eq!(args.port, DEFAULT_BACKEND_PORT);
    assert_eq!(args.port, 3212);
}

#[test]
fn schema_accepts_cli_and_mcp_operation_names() {
    let cli = tool_schema("import").unwrap();
    let mcp = tool_schema("ossia_import").unwrap();
    assert_eq!(cli, mcp);
    assert_eq!(cli["name"], "ossia_import");
    assert_eq!(cli["inputSchema"]["type"], "object");
    assert_eq!(cli["inputSchema"]["required"], json!(["path"]));
}

#[test]
fn schema_rejects_unknown_operations() {
    let error = tool_schema("unknown").unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert!(error.message.contains("unknown Ossia operation"));
}

#[test]
fn resources_follow_tauri_install_layouts() {
    let root = tempfile::tempdir().unwrap();

    let contents = root.path().join("Ossia.app/Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::create_dir_all(contents.join("Resources/engine")).unwrap();
    let mac = contents.join("MacOS/ossia");
    fs::write(&mac, "").unwrap();
    assert_eq!(
        resources_dir_for(&mac, ResourceLayout::MacOs).unwrap(),
        contents.join("Resources")
    );
    assert_eq!(
        resources_dir_for(&mac, ResourceLayout::Windows).unwrap(),
        contents.join("MacOS")
    );

    let windows = root.path().join("Ossia");
    fs::create_dir_all(windows.join("engine")).unwrap();
    let windows_exe = windows.join("ossia.exe");
    fs::write(&windows_exe, "").unwrap();
    assert_eq!(
        resources_dir_for(&windows_exe, ResourceLayout::Windows).unwrap(),
        windows
    );

    let linux = root.path().join("root/usr");
    fs::create_dir_all(linux.join("bin")).unwrap();
    fs::create_dir_all(linux.join("lib/ossia/engine")).unwrap();
    let linux_exe = linux.join("bin/ossia");
    fs::write(&linux_exe, "").unwrap();
    assert_eq!(
        resources_dir_for(&linux_exe, ResourceLayout::Linux).unwrap(),
        linux.join("lib/ossia")
    );

    fs::create_dir_all(linux.join("bin/engine")).unwrap();
    fs::write(linux.join("bin/engine/resources.json"), "{}").unwrap();
    assert_eq!(
        resources_dir_for(&linux_exe, ResourceLayout::Linux).unwrap(),
        linux.join("bin")
    );
}

#[test]
fn manual_mcp_resource_arguments_only_include_custom_locations() {
    let resources = PathBuf::from("/install/Resources");
    let mut config = server::Config {
        home: PathBuf::from("/data with spaces"),
        port: 4321,
        engine_root: resources.join("engine"),
        docs_root: resources.join("docs/lilypond/2.26.0"),
        static_dir: Some(resources.join("ui")),
        dev_origin: None,
        listen: None,
        public_url: None,
    };
    assert!(resource_override_args_for(&config, &resources).is_empty());
    config.engine_root = PathBuf::from("/custom engine");
    config.docs_root = PathBuf::from("/custom docs");
    config.static_dir = Some(PathBuf::from("/custom ui"));
    assert_eq!(
        resource_override_args_for(&config, &resources),
        vec![
            "--engine-root",
            "/custom engine",
            "--docs-root",
            "/custom docs",
            "--static-dir",
            "/custom ui"
        ]
    );
    let mut command = vec!["ossia".to_string(), "mcp".to_string()];
    command.extend(resource_override_args_for(&config, &resources));
    let args = Args::try_parse_from(command).unwrap();
    assert_eq!(args.engine_root, Some(config.engine_root));
    assert_eq!(args.docs_root, Some(config.docs_root));
    assert_eq!(args.static_dir, config.static_dir);
}
