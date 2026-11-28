mod agent;
mod antigravity;
mod claude_code;
mod claude_desktop;
mod codex;
mod command;
mod dsh;
mod files;
pub mod launcher;
mod pi;
mod record;
mod registry;
mod resources;
mod scan;
mod workbuddy;

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
    /// User home used to resolve WorkBuddy's per-product data directories.
    pub workbuddy_home: PathBuf,
    /// Bounded list of WorkBuddy application roots discovered by the caller.
    pub workbuddy_apps: Vec<PathBuf>,
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
                    launch_env.insert(key.to_string(), path_text(path));
                }
            }
        }
        Self {
            codex_home: absolute_env("CODEX_HOME").unwrap_or_else(|| user.join(".codex")),
            dsh_home: absolute_env("DSH_HOME").unwrap_or_else(|| user.join(".dsh")),
            workbuddy_home: user.clone(),
            workbuddy_apps: workbuddy::app_candidates(&user),
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
    port: u16,
}

impl Integrator {
    pub fn new(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        port: u16,
    ) -> Result<Self, AppError> {
        Self::with_paths(home, executable, HostPaths::detected(), port)
    }

    pub fn with_paths(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        paths: HostPaths,
        port: u16,
    ) -> Result<Self, AppError> {
        let home = absolute_path(home.as_ref(), "integration home")?;
        let shared_root = default_shared_root(&home)?;
        Self::build(
            home,
            executable,
            ResourceSource::Bundled,
            paths,
            shared_root,
            port,
        )
    }

    pub fn with_resource_dir(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        resources: impl AsRef<Path>,
        paths: HostPaths,
        port: u16,
    ) -> Result<Self, AppError> {
        let home = absolute_path(home.as_ref(), "integration home")?;
        let shared_root = default_shared_root(&home)?;
        Self::build(
            home,
            executable,
            ResourceSource::directory(resources.as_ref())?,
            paths,
            shared_root,
            port,
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
        port: u16,
    ) -> Result<Self, AppError> {
        Ok(Self {
            home: absolute_path(home.as_ref(), "integration home")?,
            executable: absolute_path(executable.as_ref(), "integration executable")?,
            shared_root: absolute_path(&shared_root, "shared integration root")?,
            resources,
            paths,
            port,
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
        self.agents_with_env(&command::AgentEnvironment::new())
    }

    fn agents_with_env(
        &self,
        environment: &command::AgentEnvironment,
    ) -> Result<AgentsSnapshot, AppError> {
        let loaded = record::load(&self.home)?;
        let adapters = registry::adapters(&self.paths);
        let mut agents = std::thread::scope(|scope| {
            let handles: Vec<_> = adapters
                .iter()
                .map(|adapter| {
                    let loaded = &loaded;
                    scope.spawn(move || {
                        command::scanning(|| {
                            let management_id =
                                record::management_id(adapter.id(), adapter.profile());
                            let record = loaded.store.records.get(&management_id);
                            let legacy = loaded.legacy.iter().find(|value| {
                                value.agent_id == adapter.id() && value.profile == adapter.profile()
                            });
                            let antigravity_hosts = (adapter.id() == "antigravity")
                                .then(|| antigravity::hosts(&self.paths));
                            let result = match &antigravity_hosts {
                                Some(hosts) => Ok(antigravity::detected(&self.paths, hosts)),
                                None => adapter.detect_with_env(&self.paths, environment),
                            };
                            let (detected, detection_error) = match result {
                                Ok(detected) => (detected, None),
                                Err(error) => (None, Some(error)),
                            };
                            if detected.is_none()
                                && detection_error.is_none()
                                && record.is_none()
                                && legacy.is_none()
                            {
                                return None;
                            }
                            let mut inspection_error =
                                detection_error.as_ref().map(|error| error.message.clone());
                            let inspection = if detection_error.is_some() {
                                Ok(AgentInspection {
                                    state: AgentState::Unavailable,
                                    installed: false,
                                    enabled: false,
                                    managed: record.is_some(),
                                    path: None,
                                    message: None,
                                    conflicts: Vec::new(),
                                })
                            } else {
                                match (&detected, record, legacy) {
                                    (Some(detected), Some(record), _) => {
                                        let ctx = AgentContext {
                                            integrator: self,
                                            detected,
                                        };
                                        adapter.status(&ctx, Some(record))
                                    }
                                    (Some(_), None, Some(_)) => Ok(AgentInspection {
                                        state: AgentState::UpdateRequired,
                                        installed: false,
                                        enabled: true,
                                        managed: true,
                                        path: None,
                                        message: Some(AgentNotice::LegacyReinstall),
                                        conflicts: Vec::new(),
                                    }),
                                    (Some(detected), None, None) => {
                                        let ctx = AgentContext {
                                            integrator: self,
                                            detected,
                                        };
                                        adapter.status(&ctx, None)
                                    }
                                    (None, Some(record), _) => Ok(AgentInspection {
                                        state: AgentState::Unavailable,
                                        installed: false,
                                        enabled: false,
                                        managed: true,
                                        path: Some(record.shared_root.clone()),
                                        message: Some(AgentNotice::HostUnavailable),
                                        conflicts: Vec::new(),
                                    }),
                                    (None, None, Some(_)) => Ok(AgentInspection {
                                        state: AgentState::UpdateRequired,
                                        installed: false,
                                        enabled: false,
                                        managed: true,
                                        path: None,
                                        message: Some(AgentNotice::LegacyHostUnavailable),
                                        conflicts: Vec::new(),
                                    }),
                                    (None, None, None) => unreachable!(),
                                }
                            }
                            .unwrap_or_else(|error| {
                                let detected = detected
                                    .as_ref()
                                    .expect("inspection requires a detected host");
                                let ctx = AgentContext {
                                    integrator: self,
                                    detected,
                                };
                                inspection_error = Some(format!(
                                    "{}: {}",
                                    path_text(adapter.config_path(&ctx)),
                                    error.message
                                ));
                                AgentInspection {
                                    state: AgentState::Unavailable,
                                    installed: false,
                                    enabled: false,
                                    managed: record.is_some(),
                                    path: None,
                                    message: None,
                                    conflicts: Vec::new(),
                                }
                            });
                            let config_root = detected
                                .as_ref()
                                .map(|value| path_text(&value.config_root))
                                .unwrap_or_else(|| default_config_root(adapter.id(), &self.paths));
                            let config_path = detected
                                .as_ref()
                                .map(|value| {
                                    let ctx = AgentContext {
                                        integrator: self,
                                        detected: value,
                                    };
                                    path_text(adapter.config_path(&ctx))
                                })
                                .or_else(|| {
                                    record
                                        .and_then(|record| record.entries.first())
                                        .map(|entry| normalize_path(&entry.path))
                                })
                                .unwrap_or_default();
                            let mut agent = AgentIntegration {
                                id: adapter.id().into(),
                                name: adapter.name().into(),
                                icon: adapter.icon().into(),
                                method: adapter.method(),
                                state: inspection.state,
                                error: inspection_error,
                                installed: inspection.installed,
                                enabled: inspection.enabled,
                                managed: inspection.managed,
                                version: detected.as_ref().and_then(|value| {
                                    (!value.version.is_empty()).then(|| value.version.clone())
                                }),
                                executable: detected
                                    .as_ref()
                                    .map(|value| path_text(&value.executable)),
                                config_root,
                                config_path,
                                profile: adapter.visible_profile().map(Into::into),
                                path: inspection.path.map(|value| normalize_path(&value)),
                                reload_hint: Some(adapter.reload_hint()),
                                message: inspection.message,
                                hosts: Vec::new(),
                                conflicts: inspection
                                    .conflicts
                                    .into_iter()
                                    .map(|mut conflict| {
                                        conflict.target = normalize_path(&conflict.target);
                                        conflict
                                    })
                                    .collect(),
                            };
                            if let Some(hosts) = &antigravity_hosts {
                                antigravity::present_hosts(&mut agent, hosts);
                            }
                            Some(agent)
                        })
                    })
                })
                .collect();
            handles
                .into_iter()
                .filter_map(|handle| handle.join().expect("Agent scan panicked"))
                .collect::<Vec<_>>()
        });
        let claude: Vec<_> = agents
            .iter()
            .filter(|agent| agent.id == "claude")
            .cloned()
            .collect();
        if !claude.is_empty() {
            agents.retain(|agent| agent.id != "claude");
            let mut combined = claude[0].clone();
            combined.profile = None;
            let errors: Vec<_> = claude
                .iter()
                .filter_map(|host| host.error.as_deref())
                .collect();
            combined.error = (errors.len() == claude.len()).then(|| errors.join("; "));
            combined.hosts = claude
                .iter()
                .map(|host| AgentHost {
                    name: if host.profile.as_deref() == Some("desktop") {
                        "Claude Desktop"
                    } else {
                        "Claude Code"
                    }
                    .into(),
                    error: host.error.clone().or_else(|| {
                        loaded
                            .store
                            .group_problems
                            .get(&record::management_id(
                                "claude",
                                host.profile.as_deref().unwrap_or_default(),
                            ))
                            .cloned()
                    }),
                    method: host.method,
                    state: host.state,
                    version: host.version.clone(),
                    config_path: host.config_path.clone(),
                    reload_hint: host.reload_hint.clone(),
                    conflicts: host.conflicts.clone(),
                })
                .collect();
            combined.name = claude_name(&combined.hosts).into();
            combined.managed = claude.iter().any(|host| host.managed)
                || loaded.store.enabled_groups.iter().any(|id| id == "claude");
            combined.installed = claude.iter().any(|host| host.installed || host.managed);
            combined.enabled = claude.iter().all(|host| host.enabled && host.installed)
                && combined.hosts.iter().all(|host| host.error.is_none());
            combined.conflicts = claude
                .iter()
                .flat_map(|host| host.conflicts.clone())
                .collect();
            combined.state = if combined.error.is_some() {
                AgentState::Unavailable
            } else if !combined.conflicts.is_empty() {
                AgentState::Conflict
            } else if combined.enabled {
                AgentState::Installed
            } else if combined.managed || !errors.is_empty() {
                AgentState::Broken
            } else {
                AgentState::Available
            };
            combined.reload_hint = None;
            agents.push(combined);
        }
        Ok(AgentsSnapshot {
            agents,
            connection_order: loaded.store.connection_order,
            syncing: SYNCING.load(Ordering::Relaxed),
            error: None,
        })
    }

    /// Forget a managed Agent whose status cannot be inspected, without changing its host files.
    pub fn forget_agent(&self, id: &str) -> Result<AgentsSnapshot, AppError> {
        let shared = record::shared_lock(&self.shared_root)?;
        let lock = record::lock(&self.home)?;
        let snapshot = self.agents()?;
        if !snapshot
            .agents
            .iter()
            .any(|agent| agent.id == id && agent.managed && agent.error.is_some())
        {
            return Err(AppError::invalid(
                "Agent has no managed integration with an inspection error",
            ));
        }
        let mut store = record::load(&self.home)?.store;
        store.records.retain(|_, record| record.agent_id != id);
        store.connection_order.retain(|agent_id| agent_id != id);
        store.enabled_groups.retain(|agent_id| agent_id != id);
        for adapter in registry::adapters(&self.paths)
            .iter()
            .filter(|adapter| adapter.id() == id)
        {
            store
                .group_problems
                .remove(&record::management_id(id, adapter.profile()));
        }
        record::write(&self.home, &store)?;
        drop(lock);
        drop(shared);
        self.agents()
    }

    pub fn set_agents(&self, change: AgentsChange) -> Result<AgentsMutation, AppError> {
        let environment = command::AgentEnvironment::new();
        let _shared = record::shared_lock(&self.shared_root)?;
        let _record = record::lock(&self.home)?;
        let loaded = record::load(&self.home)?;
        let mut store = loaded.store.clone();
        self.cleanup_legacy(&loaded.legacy, &environment)?;
        let adapters = registry::adapters(&self.paths);
        let mut problems = Vec::new();
        for id in &change.agent_ids {
            let matching: Vec<_> = adapters
                .iter()
                .map(Box::as_ref)
                .filter(|adapter| adapter.id() == id)
                .collect();
            if matching.is_empty() {
                problems.push(AgentProblem {
                    agent_id: id.clone(),
                    message: "unknown Agent".into(),
                });
                continue;
            }
            if id == "claude" {
                store.enabled_groups.retain(|group| group != id);
                if change.enabled {
                    store.enabled_groups.push(id.clone());
                }
                record::write(&self.home, &store)?;
            }
            for adapter in matching {
                let management_id = record::management_id(adapter.id(), adapter.profile());
                let previous = store.records.get(&management_id).cloned();
                let detection = adapter.detect_with_env(&self.paths, &environment);
                if let Err(error) = &detection {
                    problems.push(AgentProblem {
                        agent_id: id.clone(),
                        message: format!("{}: {}", adapter.profile(), error.message),
                    });
                    continue;
                }
                let Some(detected) = detection? else {
                    if id == "claude" && (change.enabled || previous.is_none()) {
                        continue;
                    }
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
                let result = if change.enabled {
                    adapter
                        .install(&ctx, previous.as_ref(), change.overwrite)
                        .map(|entries| {
                            if !store.connection_order.contains(id) {
                                store.connection_order.push(id.clone());
                            }
                            store.records.insert(
                                management_id.clone(),
                                ManagedRecord {
                                    management_id: management_id.clone(),
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
                        if !store.records.values().any(|record| record.agent_id == *id) {
                            store.connection_order.retain(|agent_id| agent_id != id);
                        }
                    })
                } else {
                    Ok(())
                };
                if id == "claude" {
                    match &result {
                        Ok(()) => {
                            store.group_problems.remove(&management_id);
                        }
                        Err(error) => {
                            store
                                .group_problems
                                .insert(management_id.clone(), normalize_path(&error.message));
                        }
                    }
                }
                if let Err(error) = result {
                    problems.push(AgentProblem {
                        agent_id: id.clone(),
                        message: format!(
                            "{}: {}",
                            adapter.profile(),
                            normalize_path(&error.message)
                        ),
                    });
                }
                record::write(&self.home, &store)?;
            }
        }
        record::write(&self.home, &store)?;
        drop(_record);
        drop(_shared);
        Ok(AgentsMutation {
            snapshot: self.agents_with_env(&environment)?,
            problems,
        })
    }

    pub fn sync_managed(&self) -> Result<AgentsSnapshot, AppError> {
        let environment = command::AgentEnvironment::new();
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
            let adapters = registry::adapters(&self.paths);
            let claude_enabled = store.enabled_groups.iter().any(|id| id == "claude");
            for current in store.records.values_mut() {
                if current.agent_id == "claude" && !claude_enabled {
                    continue;
                }
                let Some(adapter) =
                    registry::find_profile(&adapters, &current.agent_id, &current.profile)
                else {
                    continue;
                };
                let Ok(Some(detected)) = adapter.detect_with_env(&self.paths, &environment) else {
                    continue;
                };
                let ctx = AgentContext {
                    integrator: self,
                    detected: &detected,
                };
                match adapter.sync(&ctx, current) {
                    Ok(Some(entries)) => {
                        store.group_problems.remove(&current.management_id);
                        current.entries = entries;
                        current.updated_at = record::now();
                        current.template_version = TEMPLATE_VERSION;
                        current.executable = self.executable.to_string_lossy().into_owned();
                        current.shared_root = self.shared_root.to_string_lossy().into_owned();
                    }
                    Err(error) if current.agent_id == "claude" => {
                        store.group_problems.insert(
                            current.management_id.clone(),
                            normalize_path(&error.message),
                        );
                    }
                    _ => {}
                }
            }
            if store.enabled_groups.iter().any(|id| id == "claude") {
                for adapter in adapters
                    .iter()
                    .map(Box::as_ref)
                    .filter(|a| a.id() == "claude")
                {
                    let management_id = record::management_id(adapter.id(), adapter.profile());
                    if store.records.contains_key(&management_id) {
                        continue;
                    }
                    let Ok(Some(detected)) = adapter.detect_with_env(&self.paths, &environment)
                    else {
                        continue;
                    };
                    let ctx = AgentContext {
                        integrator: self,
                        detected: &detected,
                    };
                    match adapter.install(&ctx, None, false) {
                        Ok(entries) => {
                            store.group_problems.remove(&management_id);
                            store.records.insert(
                                management_id.clone(),
                                ManagedRecord {
                                    management_id,
                                    agent_id: adapter.id().into(),
                                    method: adapter.method(),
                                    profile: adapter.profile().into(),
                                    template_version: TEMPLATE_VERSION,
                                    home: path_text(&self.home),
                                    executable: path_text(&self.executable),
                                    shared_root: path_text(&self.shared_root),
                                    updated_at: record::now(),
                                    entries,
                                },
                            );
                            if !store.connection_order.iter().any(|id| id == "claude") {
                                store.connection_order.push("claude".into());
                            }
                        }
                        Err(error) => {
                            store
                                .group_problems
                                .insert(management_id, normalize_path(&error.message));
                        }
                    }
                }
            }
            record::write(&self.home, &store)
        })();
        drop(_guard);
        result?;
        self.agents_with_env(&environment)
    }

    fn cleanup_legacy(
        &self,
        legacy: &[record::LegacyRecord],
        environment: &command::AgentEnvironment,
    ) -> Result<(), AppError> {
        let adapters = registry::adapters(&self.paths);
        for item in legacy {
            let Some(adapter) = registry::find_profile(&adapters, &item.agent_id, &item.profile)
            else {
                continue;
            };
            let Some(detected) = adapter.detect_with_env(&self.paths, environment)? else {
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

    /// Return the current instance's Skill and MCP configuration for manual installation.
    pub fn manual_integration(&self) -> Result<AgentManualIntegration, AppError> {
        let root = self.home.join("integrations/manual/skills/ossia");
        let skill_path = root.join("SKILL.md");
        let mut files = vec![FilePlan {
            path: skill_path.clone(),
            bytes: self.skill()?.into_bytes(),
            kind: "skill",
        }];
        for (name, resource) in [
            ("piano.ly", Resource::SkillPiano),
            ("single-staff.ly", Resource::SkillSingleStaff),
            ("guitar-tab.ly", Resource::SkillGuitarTab),
            ("jianpu.ly", Resource::SkillJianpu),
        ] {
            files.push(FilePlan {
                path: root.join("references").join(name),
                bytes: self.resources.read(resource)?,
                kind: "skill-reference",
            });
        }
        write_files(&files)?;
        Ok(AgentManualIntegration {
            command: path_text(&self.executable),
            args: self.mcp_command(),
            env: self.launch_env(),
            skill_path: path_text(skill_path),
        })
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
            path_text(&self.home),
            "--port".into(),
            self.port.to_string(),
        ]
    }

    pub(crate) fn launch_env(&self) -> BTreeMap<String, String> {
        self.paths
            .launch_env
            .iter()
            .map(|(key, value)| (key.clone(), normalize_path(value)))
            .collect()
    }
}

fn claude_name(hosts: &[AgentHost]) -> &str {
    let mut available = hosts
        .iter()
        .filter(|host| host.state != AgentState::Unavailable);
    match (available.next(), available.next()) {
        (Some(host), None) => &host.name,
        (None, _) if hosts.len() == 1 => &hosts[0].name,
        _ => "Claude",
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
        "codex" => path_text(&paths.codex_home),
        "dsh" => path_text(&paths.dsh_home),
        "pi" => path_text(
            absolute_env("PI_CODING_AGENT_DIR")
                .unwrap_or_else(|| paths.workbuddy_home.join(".pi/agent")),
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
