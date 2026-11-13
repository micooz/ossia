use super::*;
use crate::{HostPaths, Integrator};
use std::collections::BTreeMap;

#[test]
fn bundled_resources_are_complete() {
    let resources = ResourceSource::Bundled;
    let skill = resources.read_text(Resource::Skill).unwrap();
    assert!(skill.starts_with("---\nname: ossia\n"));
    assert!(resources.read(Resource::Icon).unwrap().starts_with(b"<svg"));
    assert!(resources
        .read_text(Resource::DshClient)
        .unwrap()
        .contains("__ModuleLoader__"));
    assert!(resources
        .read_text(Resource::DshHost)
        .unwrap()
        .contains("/ossia-selection"));
    for resource in [Resource::DshLocaleEn, Resource::DshLocaleZh] {
        let locale: serde_json::Value =
            serde_json::from_slice(&resources.read(resource).unwrap()).unwrap();
        assert_eq!(locale["meta"]["title"], "Ossia");
    }
}

#[test]
fn directory_resources_report_missing_files() {
    let root = tempfile::tempdir().unwrap();
    let resources = ResourceSource::directory(root.path()).unwrap();
    assert!(resources.read(Resource::Skill).is_err());
}

#[test]
fn default_integrator_reads_embedded_resources() {
    let root = tempfile::tempdir().unwrap();
    let integrator = Integrator::with_paths(
        root.path().join("library"),
        std::env::current_exe().unwrap(),
        HostPaths {
            codex_home: root.path().join("codex"),
            dsh_home: root.path().join("dsh"),
            codex_bin: None,
            dsh_bin: None,
            launch_env: BTreeMap::new(),
        },
    )
    .unwrap()
    .with_shared_root(root.path().join("shared"))
    .unwrap();

    assert!(integrator.skill().unwrap().contains("# Ossia"));
    assert!(!integrator.icon().unwrap().is_empty());
}
