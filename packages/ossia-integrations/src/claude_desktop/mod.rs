use crate::*;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) struct ClaudeDesktop;

#[cfg(test)]
#[path = "../../tests/claude_desktop/mod.rs"]
mod tests;

fn config(ctx: &AgentContext) -> PathBuf {
    ctx.detected.config_root.join("claude_desktop_config.json")
}
fn read(path: &Path) -> Result<Value, AppError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let value: Value = parse_json_or_empty(&fs::read(path).map_err(record::storage)?)
        .map_err(|e| record::storage(format!("invalid Claude Desktop configuration: {e}")))?;
    if !value.is_object() || value.get("mcpServers").is_some_and(|v| !v.is_object()) {
        return Err(record::storage(
            "invalid Claude Desktop mcpServers configuration",
        ));
    }
    Ok(value)
}
fn server(value: &Value) -> Option<&Value> {
    value.get("mcpServers").and_then(|v| v.get("ossia"))
}
fn digest(value: &Value) -> ContentHash {
    ContentHash::of(value.to_string().as_bytes())
}
fn expected(ctx: &AgentContext) -> Value {
    json!({"command": path_text(ctx.executable()), "args": ctx.mcp_command(), "env": ctx.integrator.launch_env()})
}
fn owned(value: &Value, record: Option<&ManagedRecord>) -> bool {
    record
        .and_then(|r| r.entries.first())
        .and_then(|e| e.sha256.as_ref())
        .is_some_and(|hash| *hash == digest(value))
}
fn write(path: &Path, value: &Value) -> Result<(), AppError> {
    record::atomic_write(
        path,
        &serde_json::to_vec_pretty(value).map_err(record::storage)?,
    )
}

#[cfg(target_os = "macos")]
fn app_version(executable: &Path) -> Option<String> {
    let info = plist::Value::from_file(executable.parent()?.parent()?.join("Info.plist")).ok()?;
    info.as_dictionary()?
        .get("CFBundleShortVersionString")?
        .as_string()
        .filter(|version| !version.is_empty())
        .map(str::to_string)
}

impl AgentAdapter for ClaudeDesktop {
    fn id(&self) -> &str {
        "claude"
    }
    fn name(&self) -> &str {
        "Claude"
    }
    fn icon(&self) -> &str {
        "claude"
    }
    fn method(&self) -> InstallMethod {
        InstallMethod::Mcp
    }
    fn profile(&self) -> &'static str {
        "desktop"
    }
    fn visible_profile(&self) -> Option<&'static str> {
        Some("desktop")
    }
    fn reload_hint(&self) -> AgentNotice {
        AgentNotice::ClaudeDesktopRestart
    }
    fn detect(&self, paths: &HostPaths) -> Option<DetectedAgent> {
        let home = &paths.workbuddy_home;
        #[cfg(target_os = "macos")]
        let candidates = vec![
            PathBuf::from("/Applications/Claude.app/Contents/MacOS/Claude"),
            home.join("Applications/Claude.app/Contents/MacOS/Claude"),
        ];
        #[cfg(windows)]
        let candidates = scan::program_files("Claude")
            .into_iter()
            .flat_map(|p| [p.join("Claude.exe"), p.join("app/Claude.exe")])
            .collect::<Vec<_>>();
        #[cfg(all(not(windows), not(target_os = "macos")))]
        let candidates = vec![
            PathBuf::from("/usr/bin/claude-desktop"),
            PathBuf::from("/opt/Claude/claude-desktop"),
            home.join(".local/bin/claude-desktop"),
        ];
        let executable = candidates.into_iter().find(|p| p.is_file())?;
        #[cfg(target_os = "macos")]
        let version = app_version(&executable).unwrap_or_default();
        #[cfg(not(target_os = "macos"))]
        let version = String::new();
        #[cfg(target_os = "macos")]
        let root = home.join("Library/Application Support/Claude");
        #[cfg(windows)]
        let root = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"))
            .join("Claude");
        #[cfg(all(not(windows), not(target_os = "macos")))]
        let root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("Claude");
        Some(DetectedAgent {
            command: executable.clone(),
            executable,
            command_args: vec![],
            version,
            config_root: root,
            profile: "desktop",
            env: Default::default(),
        })
    }
    fn config_path(&self, ctx: &AgentContext) -> PathBuf {
        config(ctx)
    }
    fn status(
        &self,
        ctx: &AgentContext,
        record: Option<&ManagedRecord>,
    ) -> Result<AgentInspection, AppError> {
        let value = read(&config(ctx))?;
        let entry = server(&value);
        let conflict_found = entry.is_some_and(|v| !owned(v, record));
        let installed = entry.is_some() && !conflict_found;
        let state = if conflict_found {
            AgentState::Conflict
        } else if installed {
            AgentState::Installed
        } else if record.is_some() {
            AgentState::Broken
        } else {
            AgentState::Available
        };
        Ok(AgentInspection {
            state,
            installed,
            enabled: installed,
            managed: record.is_some(),
            path: None,
            message: None,
            conflicts: if conflict_found {
                vec![conflict(
                    "mcpServers.ossia",
                    "user-config",
                    "Claude Desktop MCP entry is not managed or was modified",
                )]
            } else {
                vec![]
            },
        })
    }
    fn install(
        &self,
        ctx: &AgentContext,
        previous: Option<&ManagedRecord>,
        overwrite: bool,
    ) -> Result<Vec<ManagedEntry>, AppError> {
        let path = config(ctx);
        let mut value = read(&path)?;
        if !overwrite && server(&value).is_some_and(|v| !owned(v, previous)) {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "Claude Desktop MCP entry is not managed or was modified",
            ));
        }
        let entry = expected(ctx);
        let root = value.as_object_mut().expect("validated object");
        let servers = root
            .entry("mcpServers")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("validated object");
        servers.insert("ossia".into(), entry.clone());
        write(&path, &value)?;
        Ok(vec![ManagedEntry {
            ownership_id: "mcpServers.ossia".into(),
            path: path_text(path),
            kind: "json-entry".into(),
            sha256: Some(digest(&entry)),
        }])
    }
    fn sync(
        &self,
        ctx: &AgentContext,
        record: &ManagedRecord,
    ) -> Result<Option<Vec<ManagedEntry>>, AppError> {
        let value = read(&config(ctx))?;
        if !server(&value).is_some_and(|v| owned(v, Some(record))) {
            return Ok(None);
        }
        self.install(ctx, Some(record), false).map(Some)
    }
    fn remove(&self, ctx: &AgentContext, record: &ManagedRecord) -> Result<(), AppError> {
        let path = config(ctx);
        let mut value = read(&path)?;
        if let Some(entry) = server(&value) {
            if !owned(entry, Some(record)) {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "Claude Desktop MCP entry was modified",
                ));
            }
            value
                .get_mut("mcpServers")
                .and_then(Value::as_object_mut)
                .expect("validated object")
                .remove("ossia");
            write(&path, &value)?;
        }
        Ok(())
    }
    fn remove_legacy(&self, _ctx: &AgentContext, _record: &LegacyRecord) -> Result<(), AppError> {
        Ok(())
    }
}
