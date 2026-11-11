mod agent;
mod codex;
mod command;
mod dsh;
mod files;
pub mod launcher;
mod record;
mod registry;
mod resources;
mod scan;

pub use record::{ManagedEntry, ManagedRecord, TEMPLATE_VERSION};
use resources::{Resource, ResourceSource};

use agent::{AgentAdapter, AgentContext, AgentInspection};
use files::*;
use ossia_types::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

static SYNCING: AtomicBool = AtomicBool::new(false);

pub(crate) use agent::DetectedAgent;
pub(crate) use record::LegacyRecord;

#[derive(Debug, Clone)]
pub struct HostPaths {
    pub codex_home: PathBuf,
    pub dsh_home: PathBuf,
    pub codex_bin: Option<PathBuf>,
    pub dsh_bin: Option<PathBuf>,
    pub launch_env: BTreeMap<String, String>,
}

impl HostPaths {
    pub fn detected() -> Self {
        let user = dirs::home_dir().unwrap_or_default();
        let mut launch_env = BTreeMap::new();
        for key in ["OSSIA_ENGINE_ROOT", "OSSIA_STATIC_DIR"] {
            if let Some(value) = std::env::var_os(key) {
                let path = PathBuf::from(value);
                if path.is_absolute() {
                    launch_env.insert(key.to_string(), path.to_string_lossy().into_owned());
                }
            }
        }
        Self {
            codex_home: absolute_env("CODEX_HOME").unwrap_or_else(|| user.join(".codex")),
            dsh_home: absolute_env("DSH_HOME").unwrap_or_else(|| user.join(".dsh")),
            codex_bin: absolute_env("OSSIA_CODEX_BIN"),
            dsh_bin: absolute_env("OSSIA_DSH_BIN"),
            launch_env,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Integrator {
    home: PathBuf,
    executable: PathBuf,
    shared_root: PathBuf,
    resources: ResourceSource,
    paths: HostPaths,
}

impl Integrator {
    pub fn new(home: impl AsRef<Path>, executable: impl AsRef<Path>) -> Result<Self, AppError> {
        Self::with_paths(home, executable, HostPaths::detected())
    }

    pub fn with_paths(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        paths: HostPaths,
    ) -> Result<Self, AppError> {
        let home = absolute_path(home.as_ref(), "integration home")?;
        let shared_root = default_shared_root(&home)?;
        Self::build(
            home,
            executable,
            ResourceSource::Bundled,
            paths,
            shared_root,
        )
    }

    pub fn with_resource_dir(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        resources: impl AsRef<Path>,
        paths: HostPaths,
    ) -> Result<Self, AppError> {
        let home = absolute_path(home.as_ref(), "integration home")?;
        let shared_root = default_shared_root(&home)?;
        Self::build(
            home,
            executable,
            ResourceSource::directory(resources.as_ref())?,
            paths,
            shared_root,
        )
    }

    pub fn with_shared_root(mut self, root: impl AsRef<Path>) -> Result<Self, AppError> {
        self.shared_root = absolute_path(root.as_ref(), "shared integration root")?;
        Ok(self)
    }

    fn build(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        resources: ResourceSource,
        paths: HostPaths,
        shared_root: PathBuf,
    ) -> Result<Self, AppError> {
        Ok(Self {
            home: absolute_path(home.as_ref(), "integration home")?,
            executable: absolute_path(executable.as_ref(), "integration executable")?,
            shared_root: absolute_path(&shared_root, "shared integration root")?,
            resources,
            paths,
        })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn shared_root(&self) -> &Path {
        &self.shared_root
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn paths(&self) -> &HostPaths {
        &self.paths
    }

    pub fn agents(&self) -> Result<AgentsSnapshot, AppError> {
        let loaded = record::load(&self.home)?;
        let mut agents = Vec::new();
        for adapter in registry::adapters() {
            let management_id = record::management_id(adapter.id(), adapter.profile());
            let record = loaded.store.records.get(&management_id);
            let legacy = loaded
                .legacy
                .iter()
                .find(|value| value.agent_id == adapter.id() && value.profile == adapter.profile());
            let detected = adapter.detect(&self.paths);
            if detected.is_none() && record.is_none() && legacy.is_none() {
                continue;
            }
            let inspection = match (&detected, record, legacy) {
                (Some(detected), Some(record), _) => {
                    let ctx = AgentContext {
                        integrator: self,
                        detected,
                    };
                    adapter.status(&ctx, Some(record))?
                }
                (Some(_), None, Some(_)) => AgentInspection {
                    state: AgentState::UpdateRequired,
                    installed: false,
                    enabled: true,
                    managed: true,
                    path: None,
                    message: Some(AgentNotice::LegacyReinstall),
                    conflicts: Vec::new(),
                },
                (Some(detected), None, None) => {
                    let ctx = AgentContext {
                        integrator: self,
                        detected,
                    };
                    adapter.status(&ctx, None)?
                }
                (None, Some(record), _) => AgentInspection {
                    state: AgentState::Unavailable,
                    installed: false,
                    enabled: false,
                    managed: true,
                    path: Some(record.shared_root.clone()),
                    message: Some(AgentNotice::HostUnavailable),
                    conflicts: Vec::new(),
                },
                (None, None, Some(_)) => AgentInspection {
                    state: AgentState::UpdateRequired,
                    installed: false,
                    enabled: false,
                    managed: true,
                    path: None,
                    message: Some(AgentNotice::LegacyHostUnavailable),
                    conflicts: Vec::new(),
                },
                (None, None, None) => unreachable!(),
            };
            let config_root = detected
                .as_ref()
                .map(|value| value.config_root.to_string_lossy().into_owned())
                .unwrap_or_else(|| default_config_root(adapter.id(), &self.paths));
            let config_path = detected
                .as_ref()
                .map(|value| {
                    let ctx = AgentContext {
                        integrator: self,
                        detected: value,
                    };
                    adapter.config_path(&ctx).to_string_lossy().into_owned()
                })
                .unwrap_or_default();
            agents.push(AgentIntegration {
                id: adapter.id().into(),
                name: adapter.name().into(),
                icon: adapter.icon().into(),
                method: adapter.method(),
                state: inspection.state,
                installed: inspection.installed,
                enabled: inspection.enabled,
                managed: inspection.managed,
                version: detected.as_ref().map(|value| value.version.clone()),
                executable: detected
                    .as_ref()
                    .map(|value| value.executable.to_string_lossy().into_owned()),
                config_root,
                config_path,
                profile: adapter.visible_profile().map(Into::into),
                path: inspection.path,
                reload_hint: Some(adapter.reload_hint()),
                message: inspection.message,
                conflicts: inspection.conflicts,
            });
        }
        Ok(AgentsSnapshot {
            agents,
            connection_order: loaded.store.connection_order,
            syncing: SYNCING.load(Ordering::Relaxed),
            error: None,
        })
    }

    pub fn set_agents(&self, change: AgentsChange) -> Result<AgentsMutation, AppError> {
        let _shared = record::shared_lock(&self.shared_root)?;
        let _record = record::lock(&self.home)?;
        let loaded = record::load(&self.home)?;
        let mut store = loaded.store.clone();
        self.cleanup_legacy(&loaded.legacy)?;
        let mut problems = Vec::new();
        for id in &change.agent_ids {
            let Some(adapter) = registry::find(id) else {
                problems.push(AgentProblem {
                    agent_id: id.clone(),
                    message: "unknown Agent".into(),
                });
                continue;
            };
            let Some(detected) = adapter.detect(&self.paths) else {
                problems.push(AgentProblem {
                    agent_id: id.clone(),
                    message: format!("{} is unavailable", adapter.name()),
                });
                continue;
            };
            let ctx = AgentContext {
                integrator: self,
                detected: &detected,
            };
            let management_id = record::management_id(adapter.id(), adapter.profile());
            let previous = store.records.get(&management_id).cloned();
            let result = if change.enabled {
                adapter.install(&ctx, previous.as_ref()).map(|entries| {
                    if !store.connection_order.contains(id) {
                        store.connection_order.push(id.clone());
                    }
                    store.records.insert(
                        management_id.clone(),
                        ManagedRecord {
                            management_id,
                            agent_id: adapter.id().into(),
                            method: adapter.method(),
                            profile: adapter.profile().into(),
                            template_version: TEMPLATE_VERSION,
                            home: self.home.to_string_lossy().into_owned(),
                            executable: self.executable.to_string_lossy().into_owned(),
                            shared_root: self.shared_root.to_string_lossy().into_owned(),
                            updated_at: record::now(),
                            entries,
                        },
                    );
                })
            } else if let Some(previous) = previous.as_ref() {
                adapter.remove(&ctx, previous).map(|()| {
                    store.records.remove(&management_id);
                    store.connection_order.retain(|agent_id| agent_id != id);
                })
            } else {
                Ok(())
            };
            if let Err(error) = result {
                problems.push(AgentProblem {
                    agent_id: id.clone(),
                    message: error.message,
                });
            }
        }
        record::write(&self.home, &store)?;
        drop(_record);
        drop(_shared);
        Ok(AgentsMutation {
            snapshot: self.agents()?,
            problems,
        })
    }

    pub fn sync_managed(&self) -> Result<AgentsSnapshot, AppError> {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                SYNCING.store(false, Ordering::Relaxed);
            }
        }
        SYNCING.store(true, Ordering::Relaxed);
        let _guard = Guard;
        let result = (|| {
            let _shared = record::shared_lock(&self.shared_root)?;
            let _record = record::lock(&self.home)?;
            let loaded = record::load(&self.home)?;
            let mut store = loaded.store.clone();
            for current in store.records.values_mut() {
                let Some(adapter) = registry::find(&current.agent_id) else {
                    continue;
                };
                let Some(detected) = adapter.detect(&self.paths) else {
                    continue;
                };
                let ctx = AgentContext {
                    integrator: self,
                    detected: &detected,
                };
                if let Ok(Some(entries)) = adapter.sync(&ctx, current) {
                    current.entries = entries;
                    current.updated_at = record::now();
                    current.template_version = TEMPLATE_VERSION;
                    current.executable = self.executable.to_string_lossy().into_owned();
                    current.shared_root = self.shared_root.to_string_lossy().into_owned();
                }
            }
            record::write(&self.home, &store)
        })();
        drop(_guard);
        result?;
        self.agents()
    }

    fn cleanup_legacy(&self, legacy: &[record::LegacyRecord]) -> Result<(), AppError> {
        for item in legacy {
            let Some(adapter) = registry::find(&item.agent_id) else {
                continue;
            };
            let Some(detected) = adapter.detect(&self.paths) else {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    format!(
                        "cannot clean legacy {} integration while the host is unavailable",
                        adapter.name()
                    ),
                ));
            };
            let ctx = AgentContext {
                integrator: self,
                detected: &detected,
            };
            adapter.remove_legacy(&ctx, item)?;
        }
        Ok(())
    }

    pub(crate) fn skill(&self) -> Result<String, AppError> {
        let command = launcher::guide_command(&self.home, &self.paths.launch_env);
        let language = if cfg!(windows) { "powershell" } else { "sh" };
        Ok(self.resources.read_text(Resource::Skill)?.replace(
            "{{OSSIA_CLI_GUIDE}}",
            &format!("```{language}\n{command}\n```"),
        ))
    }

    pub(crate) fn icon(&self) -> Result<Vec<u8>, AppError> {
        self.resources.read(Resource::Icon)
    }

    pub(crate) fn read_resource(&self, resource: Resource) -> Result<Vec<u8>, AppError> {
        self.resources.read(resource)
    }

    pub(crate) fn mcp_command(&self) -> Vec<String> {
        vec![
            "mcp".into(),
            "--home".into(),
            self.home.to_string_lossy().into_owned(),
        ]
    }
}

fn default_shared_root(home: &Path) -> Result<PathBuf, AppError> {
    absolute_path(
        &absolute_env("OSSIA_HOST_INTEGRATION_ROOT")
            .unwrap_or_else(|| home.join("integrations/hosts")),
        "shared integration root",
    )
}

fn default_config_root(agent_id: &str, paths: &HostPaths) -> String {
    match agent_id {
        "codex" => paths.codex_home.to_string_lossy().into_owned(),
        "dsh" => paths.dsh_home.to_string_lossy().into_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn shared_integration_root_follows_the_runtime_home() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join(".ossia");
        let paths = HostPaths {
            codex_home: root.path().join("codex"),
            dsh_home: root.path().join("dsh"),
            codex_bin: None,
            dsh_bin: None,
            launch_env: BTreeMap::new(),
        };
        let integrator =
            Integrator::with_paths(home.clone(), std::env::current_exe().unwrap(), paths).unwrap();
        assert_eq!(integrator.shared_root(), home.join("integrations/hosts"));
    }
}
