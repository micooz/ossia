use super::*;

#[test]
fn schema_one_records_are_exposed_as_legacy_without_touching_the_host() {
    let root = tempfile::tempdir().unwrap();
    let dir = integration_root(root.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        store_path(root.path()),
        r#"{
          "schemaVersion": 1,
          "records": {
            "ossia:codex:default": {
              "managementId": "ossia:codex:default",
              "host": "codex",
              "profile": "default",
              "mode": "skill-mcp",
              "templateVersion": 1,
              "home": "/tmp/ossia",
              "executable": "/tmp/ossia",
              "updatedAt": "unix:1",
              "entries": [{
                "ownershipId": "codex:mcp:ossia",
                "path": "codex:mcp:ossia",
                "kind": "host-entry",
                "sha256": null
              }]
            }
          }
        }"#,
    )
    .unwrap();

    let loaded = load(root.path()).unwrap();
    assert!(loaded.store.records.is_empty());
    assert_eq!(loaded.legacy.len(), 1);
    assert_eq!(loaded.legacy[0].agent_id, "codex");
    assert_eq!(loaded.legacy[0].mode, "skill-mcp");
}
