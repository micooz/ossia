use crate::record::LegacyRecord;
use crate::*;
use ossia_types::{AgentConflict, AgentNotice, AgentState, InstallMethod};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(crate) struct DetectedAgent {
    pub(crate) executable: PathBuf,
    pub(crate) command: PathBuf,
    pub(crate) command_args: Vec<String>,
    pub(crate) version: String,
    pub(crate) config_root: PathBuf,
    pub(crate) profile: &'static str,
    pub(crate) env: BTreeMap<String, String>,
}

pub(crate) struct AgentContext<'a> {
    pub(crate) integrator: &'a Integrator,
    pub(crate) detected: &'a DetectedAgent,
}

impl AgentContext<'_> {
    pub(crate) fn executable(&self) -> &Path {
        self.integrator.executable()
    }

    pub(crate) fn shared_root(&self) -> &Path {
        self.integrator.shared_root()
    }

    pub(crate) fn skill(&self) -> Result<String, AppError> {
        self.integrator.skill()
    }

    pub(crate) fn icon(&self) -> Result<Vec<u8>, AppError> {
        self.integrator.icon()
    }

    pub(crate) fn resource(&self, resource: Resource) -> Result<Vec<u8>, AppError> {
        self.integrator.read_resource(resource)
    }

    pub(crate) fn mcp_command(&self) -> Vec<String> {
        self.integrator.mcp_command()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentInspection {
    pub(crate) state: AgentState,
    pub(crate) installed: bool,
    pub(crate) enabled: bool,
    pub(crate) managed: bool,
    pub(crate) path: Option<String>,
    pub(crate) message: Option<AgentNotice>,
    pub(crate) conflicts: Vec<AgentConflict>,
}

pub(crate) trait AgentAdapter: Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn icon(&self) -> &str;
    fn method(&self) -> InstallMethod;
    fn profile(&self) -> &'static str;
    fn visible_profile(&self) -> Option<&'static str> {
        None
    }
    fn reload_hint(&self) -> AgentNotice;
    fn detect(&self, paths: &HostPaths) -> Option<DetectedAgent>;
    fn detect_with_env(
        &self,
        paths: &HostPaths,
        _: &command::AgentEnvironment,
    ) -> Result<Option<DetectedAgent>, AppError> {
        Ok(self.detect(paths))
    }
    fn config_path(&self, ctx: &AgentContext) -> PathBuf;
    fn status(
        &self,
        ctx: &AgentContext,
        record: Option<&ManagedRecord>,
    ) -> Result<AgentInspection, AppError>;
    fn install(
        &self,
        ctx: &AgentContext,
        previous: Option<&ManagedRecord>,
        overwrite: bool,
    ) -> Result<Vec<ManagedEntry>, AppError>;
    fn sync(
        &self,
        ctx: &AgentContext,
        record: &ManagedRecord,
    ) -> Result<Option<Vec<ManagedEntry>>, AppError>;
    fn remove(&self, ctx: &AgentContext, record: &ManagedRecord) -> Result<(), AppError>;
    fn remove_legacy(&self, ctx: &AgentContext, record: &LegacyRecord) -> Result<(), AppError>;
}
