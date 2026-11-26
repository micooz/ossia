use super::*;
use crate::{HostPaths, Integrator};
use ossia_types::DEFAULT_BACKEND_PORT;
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
    for resource in [
        Resource::SkillPiano,
        Resource::SkillSingleStaff,
        Resource::SkillGuitarTab,
    ] {
        assert!(resources
            .read_text(resource)
            .unwrap()
            .starts_with("\\version \"2.26.0\"\n"));
    }
    let guitar_tab = resources.read_text(Resource::SkillGuitarTab).unwrap();
    assert!(guitar_tab.contains("\\new TabStaff"));
    assert!(!guitar_tab.contains("\\new StaffGroup"));
    assert!(guitar_tab.contains("\\set Staff.midiInstrument"));
    assert!(guitar_tab.contains("\\key c \\major"));
    assert!(guitar_tab.contains("c4\\5 d4\\5 e4\\4 f4\\4"));
    assert!(guitar_tab.contains("markup-system-spacing"));
    let piano = resources.read_text(Resource::SkillPiano).unwrap();
    assert!(piano.contains("\\key c \\major"));
    assert!(piano.contains("c4 d e f"));
    assert!(resources
        .read_text(Resource::SkillJianpu)
        .unwrap()
        .starts_with("% ossia:notation=jianpu\n\\version \"2.26.0\"\n"));
    for resource in [
        Resource::SkillPiano,
        Resource::SkillSingleStaff,
        Resource::SkillGuitarTab,
    ] {
        assert!(resources
            .read_text(resource)
            .unwrap()
            .contains("tagline = ##f"));
    }
    assert!(!resources
        .read_text(Resource::SkillSingleStaff)
        .unwrap()
        .contains("set-paper-size"));
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
            workbuddy_home: root.path().join("home"),
            workbuddy_apps: Vec::new(),
            codex_bin: None,
            dsh_bin: None,
            launch_env: BTreeMap::new(),
        },
        DEFAULT_BACKEND_PORT,
    )
    .unwrap()
    .with_shared_root(root.path().join("shared"))
    .unwrap();

    let skill = integrator.skill().unwrap();
    assert!(skill.contains("# Ossia"));
    assert!(skill.contains("ossia_guide"));
    assert!(skill.contains("read its `text`"));
    assert!(skill.contains("relevant linked path"));
    assert!(!integrator.icon().unwrap().is_empty());
}
