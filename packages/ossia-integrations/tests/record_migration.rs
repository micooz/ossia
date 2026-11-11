use ossia_integrations::{HostPaths, Integrator};
use serde_json::json;
use std::{collections::BTreeMap, fs};

#[test]
fn old_store_initializes_order_once_from_last_known_timestamps() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("library");
    fs::create_dir_all(home.join("integrations")).unwrap();
    let record = |id: &str, profile: &str, time: u64| {
        json!({
            "managementId": format!("ossia:{id}:{profile}"), "agentId": id,
            "method": "plugin", "profile": profile, "templateVersion": 2,
            "home": home, "executable": "/missing", "sharedRoot": "/missing",
            "updatedAt": format!("unix:{time}"), "entries": []
        })
    };
    fs::write(
        home.join("integrations/managed.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "records": {
                "ossia:codex:default": record("codex", "default", 20),
                "ossia:dsh:desktop": record("dsh", "desktop", 2)
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let paths = HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        codex_bin: Some(root.path().join("missing")),
        dsh_bin: Some(root.path().join("missing")),
        launch_env: BTreeMap::new(),
    };
    let integrator =
        Integrator::with_paths(&home, std::env::current_exe().unwrap(), paths).unwrap();
    assert_eq!(
        integrator.agents().unwrap().connection_order,
        ["dsh", "codex"]
    );
    integrator.sync_managed().unwrap();
    let path = home.join("integrations/managed.json");
    let mut stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    stored["records"]["ossia:codex:default"]["updatedAt"] = json!("unix:1");
    fs::write(path, serde_json::to_vec(&stored).unwrap()).unwrap();
    assert_eq!(
        integrator.agents().unwrap().connection_order,
        ["dsh", "codex"]
    );
}
