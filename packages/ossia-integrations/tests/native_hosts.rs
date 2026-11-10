use ossia_integrations::{HostPaths, Integrator};
use ossia_types::{HostKind, IntegrationMode, IntegrationState};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn resources() -> PathBuf {
    PathBuf::from(
        std::env::var_os("OSSIA_TEST_INTEGRATION_ROOT").expect("set OSSIA_TEST_INTEGRATION_ROOT"),
    )
}

fn executable() -> PathBuf {
    PathBuf::from(std::env::var_os("OSSIA_TEST_OSSIA_BIN").expect("set OSSIA_TEST_OSSIA_BIN"))
}

#[test]
#[ignore = "requires OSSIA_TEST_INTEGRATION_ROOT, OSSIA_TEST_OSSIA_BIN, and the Codex CLI"]
fn codex_plugin_skill_mcp_and_remove_round_trip() {
    let work = tempfile::tempdir().unwrap();
    let codex_home = work.path().join("codex home");
    fs::create_dir_all(&codex_home).unwrap();
    let paths = HostPaths {
        codex_home,
        agents_home: work.path().join("agents home"),
        dsh_home: work.path().join("dsh"),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        work.path().join("中文 library"),
        executable(),
        resources(),
        paths,
    )
    .unwrap();

    let plugin = integrator
        .plan(HostKind::Codex, None, IntegrationMode::Plugin)
        .unwrap();
    assert!(plugin.conflicts.is_empty());
    let status = integrator
        .apply(
            HostKind::Codex,
            None,
            IntegrationMode::Plugin,
            &plugin.plan_hash,
        )
        .unwrap();
    assert_eq!(status.state, IntegrationState::PendingHost);

    let skill = integrator
        .plan(HostKind::Codex, None, IntegrationMode::SkillMcp)
        .unwrap();
    assert!(skill.conflicts.is_empty(), "{:?}", skill.conflicts);
    let status = integrator
        .apply(
            HostKind::Codex,
            None,
            IntegrationMode::SkillMcp,
            &skill.plan_hash,
        )
        .unwrap();
    assert_eq!(status.state, IntegrationState::Installed);

    let status = integrator.remove(HostKind::Codex, None, None).unwrap();
    assert_eq!(status.state, IntegrationState::Available);
    assert!(status.management.is_none());
}

#[test]
#[ignore = "requires OSSIA_TEST_INTEGRATION_ROOT, OSSIA_TEST_OSSIA_BIN, OSSIA_TEST_DSH_TEMPLATE and the DSH CLI"]
fn dsh_plugin_skill_mcp_and_remove_round_trip() {
    let work = tempfile::tempdir().unwrap();
    let template = PathBuf::from(
        std::env::var_os("OSSIA_TEST_DSH_TEMPLATE").expect("set OSSIA_TEST_DSH_TEMPLATE"),
    );
    let profile = work.path().join("profiles/desktop");
    fs::create_dir_all(&profile).unwrap();
    for name in ["package.json", "cordis.yml", "cordis.patch.yml"] {
        let source = template.join(name);
        if source.exists() {
            fs::copy(source, profile.join(name)).unwrap();
        }
    }
    let paths = HostPaths {
        codex_home: work.path().join("codex"),
        agents_home: work.path().join("agents"),
        dsh_home: work.path().to_path_buf(),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let integrator = Integrator::with_paths(
        work.path().join("中文 library"),
        executable(),
        resources(),
        paths,
    )
    .unwrap();

    let plugin = integrator
        .plan(HostKind::Dsh, Some("desktop"), IntegrationMode::Plugin)
        .unwrap();
    assert!(plugin.conflicts.is_empty(), "{:?}", plugin.conflicts);
    let status = integrator
        .apply(
            HostKind::Dsh,
            Some("desktop"),
            IntegrationMode::Plugin,
            &plugin.plan_hash,
        )
        .unwrap();
    assert_ne!(status.state, IntegrationState::Conflict);

    let skill = integrator
        .plan(HostKind::Dsh, Some("desktop"), IntegrationMode::SkillMcp)
        .unwrap();
    assert!(skill.conflicts.is_empty(), "{:?}", skill.conflicts);
    let status = integrator
        .apply(
            HostKind::Dsh,
            Some("desktop"),
            IntegrationMode::SkillMcp,
            &skill.plan_hash,
        )
        .unwrap();
    assert_eq!(status.state, IntegrationState::Installed);

    let status = integrator
        .remove(HostKind::Dsh, Some("desktop"), None)
        .unwrap();
    assert_eq!(status.state, IntegrationState::Available);
}
