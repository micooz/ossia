mod codex;
mod command;
mod dsh;
mod record;

pub use record::{ManagedEntry, ManagedRecord, TEMPLATE_VERSION};

use ossia_types::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct HostPaths {
    pub codex_home: PathBuf,
    pub agents_home: PathBuf,
    pub dsh_home: PathBuf,
    pub codex_bin: Option<PathBuf>,
    pub dsh_bin: Option<PathBuf>,
    pub launch_env: BTreeMap<String, String>,
}

pub(crate) type HostStatus = (
    IntegrationState,
    Vec<IntegrationObservation>,
    Vec<IntegrationConflict>,
    Option<String>,
);

impl HostPaths {
    pub fn detected() -> Self {
        let user = dirs::home_dir().unwrap_or_default();
        let codex_home = absolute_env("CODEX_HOME").unwrap_or_else(|| user.join(".codex"));
        let dsh_home = absolute_env("DSH_HOME").unwrap_or_else(|| user.join(".dsh"));
        let agents_home = absolute_env("AGENTS_HOME").unwrap_or_else(|| user.join(".agents"));
        let codex_bin = absolute_env("OSSIA_CODEX_BIN");
        let dsh_bin = absolute_env("OSSIA_DSH_BIN");
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
            codex_home,
            agents_home,
            dsh_home,
            codex_bin,
            dsh_bin,
            launch_env,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Integrator {
    home: PathBuf,
    executable: PathBuf,
    resources: PathBuf,
    paths: HostPaths,
}

impl Integrator {
    pub fn new(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        resources: impl AsRef<Path>,
    ) -> Result<Self, AppError> {
        Self::with_paths(home, executable, resources, HostPaths::detected())
    }

    pub fn with_paths(
        home: impl AsRef<Path>,
        executable: impl AsRef<Path>,
        resources: impl AsRef<Path>,
        paths: HostPaths,
    ) -> Result<Self, AppError> {
        let home = absolute_path(home.as_ref(), "integration home")?;
        let executable = absolute_path(executable.as_ref(), "integration executable")?;
        let resources = absolute_path(resources.as_ref(), "integration resources")?;
        Ok(Self {
            home,
            executable,
            resources,
            paths,
        })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn resources(&self) -> &Path {
        &self.resources
    }

    pub fn detection(&self) -> Result<IntegrationDetection, AppError> {
        Ok(IntegrationDetection {
            hosts: vec![codex::detect(self), dsh::detect(self)],
        })
    }

    pub fn execute(&self, request: IntegrationRequest) -> Result<Value, AppError> {
        match request.action {
            IntegrationAction::Detect => serialize(self.detection()?),
            IntegrationAction::Plan => {
                let host = request.host.ok_or_else(|| missing("host"))?;
                let mode = request.mode.ok_or_else(|| missing("mode"))?;
                serialize(self.plan(host, request.profile.as_deref(), mode)?)
            }
            IntegrationAction::Apply => {
                let host = request.host.ok_or_else(|| missing("host"))?;
                let mode = request.mode.ok_or_else(|| missing("mode"))?;
                let hash = request.plan_hash.ok_or_else(|| missing("planHash"))?;
                serialize(self.apply(host, request.profile.as_deref(), mode, &hash)?)
            }
            IntegrationAction::Status => {
                let host = request.host.ok_or_else(|| missing("host"))?;
                serialize(self.status(host, request.profile.as_deref(), request.mode)?)
            }
            IntegrationAction::Repair => {
                let host = request.host.ok_or_else(|| missing("host"))?;
                let mode = request.mode.ok_or_else(|| missing("mode"))?;
                serialize(self.apply(host, request.profile.as_deref(), mode, &zero_plan_hash())?)
            }
            IntegrationAction::Remove => {
                let host = request.host.ok_or_else(|| missing("host"))?;
                serialize(self.remove(host, request.profile.as_deref(), request.mode)?)
            }
        }
    }

    pub fn plan(
        &self,
        host: HostKind,
        profile: Option<&str>,
        mode: IntegrationMode,
    ) -> Result<IntegrationPlan, AppError> {
        let store = record::load(&self.home)?;
        let profile = profile_for(host, profile);
        let management_id = record::management_id(host, &profile);
        let previous = store.records.get(&management_id);
        let mut plan = match host {
            HostKind::Codex => codex::plan(self, &profile, mode, previous)?,
            HostKind::Dsh => dsh::plan(self, &profile, mode, previous)?,
        };
        if let Some(previous) = previous {
            if previous.mode != mode {
                plan.warnings.push(format!(
                    "switches this host/profile from {} to {}",
                    mode_name(previous.mode),
                    mode_name(mode)
                ));
            }
        }
        plan.plan_hash = plan_hash(&plan)?;
        Ok(plan)
    }

    pub fn apply(
        &self,
        host: HostKind,
        profile: Option<&str>,
        mode: IntegrationMode,
        requested_hash: &ContentHash,
    ) -> Result<IntegrationStatus, AppError> {
        let _guard = record::lock(&self.home)?;
        let store = record::load(&self.home)?;
        let profile = profile_for(host, profile);
        let management_id = record::management_id(host, &profile);
        let previous = store.records.get(&management_id).cloned();
        let plan = self.plan(host, Some(&profile), mode)?;
        if !plan.conflicts.is_empty() {
            return Err(AppError::new(
                ErrorCode::Conflict,
                plan.conflicts
                    .iter()
                    .map(|item| item.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        if !requested_hash.as_str().chars().all(|value| value == '0')
            && requested_hash != &plan.plan_hash
        {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "integration plan changed; run plan again",
            ));
        }
        let old_plan = previous
            .as_ref()
            .filter(|record| record.mode != mode)
            .map(|record| match host {
                HostKind::Codex => codex::plan_for(self, &profile, record.mode, Some(record)),
                HostKind::Dsh => dsh::plan_for(self, &profile, record.mode, Some(record)),
            })
            .transpose()?;
        if let (Some(record), Some(_)) = (&previous, &old_plan) {
            host_remove(self, host, record)?;
        }
        let applied = host_apply(self, host, &plan);
        let entries = match applied {
            Ok(entries) => entries,
            Err(error) => {
                let pending = managed_from_plan(&plan);
                let _ = host_remove_for_entries(self, host, &profile, mode, &pending);
                if let (Some(_record), Some(old_plan)) = (&previous, &old_plan) {
                    let _ = host_apply(self, host, old_plan);
                    let _ = record::write(&self.home, &store);
                }
                return Err(error);
            }
        };
        let mut next = store.clone();
        next.records.insert(
            management_id.clone(),
            ManagedRecord {
                management_id,
                host,
                profile: profile.clone(),
                mode,
                template_version: TEMPLATE_VERSION,
                home: self.home.to_string_lossy().into_owned(),
                executable: self.executable.to_string_lossy().into_owned(),
                updated_at: record::now(),
                entries: entries.clone(),
            },
        );
        if let Err(error) = record::write(&self.home, &next) {
            let _ = host_remove_for_entries(self, host, &profile, mode, &entries);
            return Err(error);
        }
        let status = self.status(host, Some(&profile), Some(mode))?;
        if matches!(
            status.state,
            IntegrationState::Conflict | IntegrationState::Broken | IntegrationState::Unavailable
        ) {
            host_remove_for_entries(self, host, &profile, mode, &entries)?;
            record::write(&self.home, &store)?;
            if let (Some(_record), Some(old_plan)) = (&previous, &old_plan) {
                let _ = host_apply(self, host, old_plan);
            }
            let message = if status.conflicts.is_empty() {
                status.message.clone()
            } else {
                Some(
                    status
                        .conflicts
                        .iter()
                        .map(|item| item.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            };
            return Err(AppError::new(
                ErrorCode::Conflict,
                message.unwrap_or_else(|| "host configuration did not verify after apply".into()),
            ));
        }
        Ok(status)
    }

    pub fn status(
        &self,
        host: HostKind,
        profile: Option<&str>,
        requested_mode: Option<IntegrationMode>,
    ) -> Result<IntegrationStatus, AppError> {
        let store = record::load(&self.home)?;
        let profile = profile_for(host, profile);
        let management_id = record::management_id(host, &profile);
        let record = store.records.get(&management_id);
        let mode = requested_mode.or(record.map(|value| value.mode));
        let detection = match host {
            HostKind::Codex => codex::detect(self),
            HostKind::Dsh => dsh::detect(self),
        };
        if !detection.installed {
            return Ok(IntegrationStatus {
                detection,
                state: IntegrationState::Unavailable,
                mode,
                management: record.map(record_view),
                observations: vec![],
                conflicts: vec![],
                message: Some("host is not installed or did not pass capability detection".into()),
            });
        }
        let (state, observations, conflicts, message) = match host {
            HostKind::Codex => codex::status(self, &profile, mode, record)?,
            HostKind::Dsh => dsh::status(self, &profile, mode, record)?,
        };
        Ok(IntegrationStatus {
            detection,
            state,
            mode,
            management: record.map(record_view),
            observations,
            conflicts,
            message,
        })
    }

    pub fn remove(
        &self,
        host: HostKind,
        profile: Option<&str>,
        requested_mode: Option<IntegrationMode>,
    ) -> Result<IntegrationStatus, AppError> {
        let _guard = record::lock(&self.home)?;
        let profile = profile_for(host, profile);
        let management_id = record::management_id(host, &profile);
        let mut store = record::load(&self.home)?;
        let Some(record) = store.records.get(&management_id).cloned() else {
            return self.status(host, Some(&profile), requested_mode);
        };
        if requested_mode.is_some_and(|mode| mode != record.mode) {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "managed integration uses a different mode",
            ));
        }
        host_remove(self, host, &record)?;
        store.records.remove(&management_id);
        record::write(&self.home, &store)?;
        self.status(host, Some(&profile), requested_mode)
    }

    pub(crate) fn paths(&self) -> &HostPaths {
        &self.paths
    }

    pub(crate) fn host_env(&self) -> BTreeMap<String, String> {
        self.paths.launch_env.clone()
    }

    pub(crate) fn skill(&self) -> Result<String, AppError> {
        read_resource(&self.resources, "skill/ossia/SKILL.md")
    }

    pub(crate) fn icon(&self) -> Result<Vec<u8>, AppError> {
        read_resource_bytes(&self.resources, "assets/ossia.svg")
    }

    pub(crate) fn mcp_command(&self) -> Vec<String> {
        vec![
            "mcp".into(),
            "--home".into(),
            self.home.to_string_lossy().into_owned(),
        ]
    }
}

fn host_apply(
    integrator: &Integrator,
    host: HostKind,
    plan: &IntegrationPlan,
) -> Result<Vec<ManagedEntry>, AppError> {
    match host {
        HostKind::Codex => codex::apply(integrator, plan),
        HostKind::Dsh => dsh::apply(integrator, plan),
    }
}

fn host_remove(
    integrator: &Integrator,
    host: HostKind,
    record: &ManagedRecord,
) -> Result<(), AppError> {
    match host {
        HostKind::Codex => codex::remove(integrator, record),
        HostKind::Dsh => dsh::remove(integrator, record),
    }
}

fn host_remove_for_entries(
    integrator: &Integrator,
    host: HostKind,
    profile: &str,
    mode: IntegrationMode,
    entries: &[ManagedEntry],
) -> Result<(), AppError> {
    match host {
        HostKind::Codex => codex::remove_entries(integrator, profile, mode, entries),
        HostKind::Dsh => dsh::remove_entries(integrator, profile, mode, entries),
    }
}

fn managed_from_plan(plan: &IntegrationPlan) -> Vec<ManagedEntry> {
    plan.entries
        .iter()
        .map(|entry| ManagedEntry {
            ownership_id: entry.ownership_id.clone(),
            path: entry.path.clone(),
            kind: entry.kind.clone(),
            sha256: entry.sha256.clone(),
        })
        .collect()
}

pub(crate) fn file_entry(
    path: &Path,
    bytes: &[u8],
    kind: &str,
    ownership: &str,
) -> IntegrationEntry {
    let exists = path.exists();
    IntegrationEntry {
        operation: if exists {
            IntegrationOperation::Replace
        } else {
            IntegrationOperation::Create
        },
        ownership_id: ownership.into(),
        path: path.to_string_lossy().into_owned(),
        kind: kind.into(),
        sha256: Some(ContentHash::of(bytes)),
    }
}

pub(crate) fn on_disk(path: &Path) -> Option<ContentHash> {
    fs::read(path).ok().map(|bytes| ContentHash::of(&bytes))
}

pub(crate) fn delete_owned(path: &Path, expected: &ContentHash) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }
    let current = on_disk(path)
        .ok_or_else(|| AppError::new(ErrorCode::StorageError, "managed file is unreadable"))?;
    if &current != expected {
        return Err(AppError::new(
            ErrorCode::Conflict,
            format!("managed file was modified: {}", path.display()),
        ));
    }
    fs::remove_file(path).map_err(record::storage)
}

pub(crate) fn verify_owned_files(entries: &[ManagedEntry]) -> Result<(), AppError> {
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        let path = Path::new(&entry.path);
        if !path.exists() {
            continue;
        }
        let current = on_disk(path)
            .ok_or_else(|| AppError::new(ErrorCode::StorageError, "managed file is unreadable"))?;
        if entry.sha256.as_ref() != Some(&current) {
            return Err(AppError::new(
                ErrorCode::Conflict,
                format!("managed file was modified: {}", path.display()),
            ));
        }
    }
    Ok(())
}

pub(crate) fn record_view(record: &ManagedRecord) -> IntegrationRecordView {
    IntegrationRecordView {
        management_id: record.management_id.clone(),
        mode: record.mode,
        executable: record.executable.clone(),
        home: record.home.clone(),
        template_version: record.template_version,
        updated_at: record.updated_at.clone(),
        entries: record
            .entries
            .iter()
            .map(|entry| IntegrationEntry {
                operation: IntegrationOperation::Create,
                ownership_id: entry.ownership_id.clone(),
                path: entry.path.clone(),
                kind: entry.kind.clone(),
                sha256: entry.sha256.clone(),
            })
            .collect(),
    }
}

pub(crate) fn conflict(
    target: impl Into<String>,
    kind: &str,
    message: impl Into<String>,
) -> IntegrationConflict {
    IntegrationConflict {
        target: target.into(),
        kind: kind.into(),
        message: message.into(),
    }
}

pub(crate) fn observation(
    target: impl Into<String>,
    kind: &str,
    state: IntegrationState,
    message: Option<String>,
) -> IntegrationObservation {
    IntegrationObservation {
        target: target.into(),
        kind: kind.into(),
        state,
        message,
    }
}

pub(crate) fn storage(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::StorageError, error.to_string())
}

fn absolute_path(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    if !path.is_absolute() {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            format!("{label} must be absolute"),
        ));
    }
    if path.exists() {
        path.canonicalize().map_err(storage)
    } else {
        Ok(path.to_path_buf())
    }
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn profile_for(host: HostKind, profile: Option<&str>) -> String {
    profile
        .unwrap_or(match host {
            HostKind::Codex => "default",
            HostKind::Dsh => "desktop",
        })
        .to_string()
}

fn mode_name(mode: IntegrationMode) -> &'static str {
    match mode {
        IntegrationMode::SkillMcp => "skill-mcp",
        IntegrationMode::Plugin => "plugin",
    }
}

fn missing(field: &str) -> AppError {
    AppError::new(ErrorCode::InvalidRequest, format!("{field} is required"))
}

fn serialize(value: impl Serialize) -> Result<Value, AppError> {
    serde_json::to_value(value).map_err(storage)
}

fn read_resource(root: &Path, name: &str) -> Result<String, AppError> {
    read_resource_bytes(root, name).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn read_resource_bytes(root: &Path, name: &str) -> Result<Vec<u8>, AppError> {
    fs::read(root.join(name)).map_err(|error| {
        AppError::new(
            ErrorCode::UnsupportedSource,
            format!("integration resource {name} is unavailable: {error}"),
        )
    })
}

fn zero_plan_hash() -> ContentHash {
    serde_json::from_value(Value::String("0".repeat(64))).expect("valid zero hash")
}

pub(crate) fn plan_hash(plan: &IntegrationPlan) -> Result<ContentHash, AppError> {
    let mut normalized = plan.clone();
    normalized.plan_hash = zero_plan_hash();
    let bytes = serde_json::to_vec(&normalized).map_err(storage)?;
    Ok(ContentHash::of(&bytes))
}

use serde::Serialize;
