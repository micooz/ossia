use crate::*;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

pub(crate) struct Pi;

#[cfg(test)]
#[path = "../../tests/pi/mod.rs"]
mod tests;

fn config(ctx: &AgentContext) -> PathBuf {
    ctx.detected.config_root.join("mcp.json")
}
fn skill_path(ctx: &AgentContext) -> PathBuf {
    ctx.detected.config_root.join("skills/ossia/SKILL.md")
}
fn read(ctx: &AgentContext) -> Result<Value, AppError> {
    let path = config(ctx);
    let value: Value = if path.exists() {
        parse_json_or_empty(&fs::read(path).map_err(record::storage)?)
            .map_err(|e| record::storage(format!("invalid Pi MCP configuration: {e}")))?
    } else {
        json!({})
    };
    if !value.is_object() || value.get("mcpServers").is_some_and(|v| !v.is_object()) {
        return Err(record::storage("invalid Pi mcpServers configuration"));
    }
    Ok(value)
}
fn server(value: &Value) -> Option<&Value> {
    value.get("mcpServers").and_then(|v| v.get("ossia"))
}
fn digest(value: &Value) -> ContentHash {
    ContentHash::of(value.to_string().as_bytes())
}
fn owned(
    record: Option<&ManagedRecord>,
    id: &str,
    hash: &ContentHash,
    path: &std::path::Path,
) -> bool {
    record.is_some_and(|r| {
        r.entries.iter().any(|e| {
            e.ownership_id == id && e.path == path_text(path) && e.sha256.as_ref() == Some(hash)
        })
    })
}
fn supported(ctx: &AgentContext) -> Result<bool, AppError> {
    command::run(
        &ctx.detected.command,
        &command::arguments(&["mcp", "--help"]),
        &ctx.detected.env,
    )
    .map(|out| out.contains("mcp.json") && out.contains("mcp add"))
}
fn conflicts(
    ctx: &AgentContext,
    value: &Value,
    previous: Option<&ManagedRecord>,
) -> Vec<AgentConflict> {
    let mut found = Vec::new();
    if server(value).is_some_and(|v| !owned(previous, "mcpServers.ossia", &digest(v), &config(ctx)))
    {
        found.push(conflict(
            "mcpServers.ossia",
            "json-entry",
            "Pi MCP entry is not managed or was modified",
        ));
    }
    let path = skill_path(ctx);
    if path.exists()
        && !on_disk(&path).is_some_and(|hash| owned(previous, "skill.ossia", &hash, &path))
    {
        found.push(conflict(
            path_text(&path),
            "file",
            "Pi Ossia skill is not managed or was modified",
        ));
    }
    found
}
fn fail_unsupported(ctx: &AgentContext) -> Result<(), AppError> {
    if supported(ctx)? {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            "Pi does not support the required built-in MCP configuration",
        ))
    }
}

impl AgentAdapter for Pi {
    fn id(&self) -> &str {
        "pi"
    }
    fn name(&self) -> &str {
        "Pi"
    }
    fn icon(&self) -> &str {
        "pi"
    }
    fn method(&self) -> InstallMethod {
        InstallMethod::Mcp
    }
    fn profile(&self) -> &'static str {
        "default"
    }
    fn reload_hint(&self) -> AgentNotice {
        AgentNotice::PiReload
    }
    fn detect(&self, paths: &HostPaths) -> Option<DetectedAgent> {
        self.detect_with_env(paths, &command::AgentEnvironment::new())
            .ok()
            .flatten()
    }
    fn detect_with_env(
        &self,
        paths: &HostPaths,
        resolver: &command::AgentEnvironment,
    ) -> Result<Option<DetectedAgent>, AppError> {
        let Some((executable, shell_discovered)) = resolver.find(
            "pi",
            absolute_env("OSSIA_PI_BIN").as_deref(),
            scan::common_bin("pi"),
        ) else {
            return Ok(None);
        };
        let root = absolute_env("PI_CODING_AGENT_DIR")
            .unwrap_or_else(|| paths.workbuddy_home.join(".pi/agent"));
        let env = command::host_env("PI_CODING_AGENT_DIR", &root);
        let (env, version) =
            resolver.probe(&executable, env, &["mcp", "--help"], shell_discovered)?;
        let version = version
            .lines()
            .find(|line| !line.trim().is_empty())
            .and_then(|line| line.split_whitespace().last())
            .unwrap_or_default()
            .to_string();
        Ok(Some(DetectedAgent {
            command: executable.clone(),
            executable,
            command_args: vec![],
            version,
            config_root: root,
            profile: "default",
            env,
        }))
    }
    fn config_path(&self, ctx: &AgentContext) -> PathBuf {
        config(ctx)
    }
    fn status(
        &self,
        ctx: &AgentContext,
        previous: Option<&ManagedRecord>,
    ) -> Result<AgentInspection, AppError> {
        if !supported(ctx)? {
            return Ok(AgentInspection {
                state: AgentState::UpdateRequired,
                installed: false,
                enabled: false,
                managed: previous.is_some(),
                path: None,
                message: Some(AgentNotice::PiMcpUnsupported),
                conflicts: vec![],
            });
        }
        let value = read(ctx)?;
        let found = conflicts(ctx, &value, previous);
        let installed = previous.is_some()
            && server(&value).is_some()
            && skill_path(ctx).is_file()
            && found.is_empty();
        let state = if !found.is_empty() {
            AgentState::Conflict
        } else if installed {
            AgentState::Installed
        } else if previous.is_some() {
            AgentState::Broken
        } else {
            AgentState::Available
        };
        Ok(AgentInspection {
            state,
            installed,
            enabled: installed,
            managed: previous.is_some(),
            path: Some(path_text(skill_path(ctx))),
            message: None,
            conflicts: found,
        })
    }
    fn install(
        &self,
        ctx: &AgentContext,
        previous: Option<&ManagedRecord>,
        overwrite: bool,
    ) -> Result<Vec<ManagedEntry>, AppError> {
        fail_unsupported(ctx)?;
        let mut value = read(ctx)?;
        let found = conflicts(ctx, &value, previous);
        if !overwrite && !found.is_empty() {
            return Err(conflict_error(&found));
        }
        let entry = json!({"command": path_text(ctx.executable()), "args": ctx.mcp_command(), "env": ctx.integrator.launch_env()});
        value
            .as_object_mut()
            .unwrap()
            .entry("mcpServers")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .unwrap()
            .insert("ossia".into(), entry.clone());
        let skill = ctx.skill()?.into_bytes();
        let files = vec![
            FilePlan {
                path: config(ctx),
                bytes: serde_json::to_vec_pretty(&value).map_err(record::storage)?,
                kind: "json-entry",
            },
            FilePlan {
                path: skill_path(ctx),
                bytes: skill.clone(),
                kind: "skill",
            },
        ];
        let snapshot = snapshot_files(&files);
        if let Err(error) = write_files(&files) {
            return match restore_files(&snapshot) {
                Ok(()) => Err(error),
                Err(rollback) => Err(record::storage(format!(
                    "{}; rollback failed: {}",
                    error.message, rollback.message
                ))),
            };
        }
        Ok(vec![
            ManagedEntry {
                ownership_id: "mcpServers.ossia".into(),
                path: path_text(config(ctx)),
                kind: "json-entry".into(),
                sha256: Some(digest(&entry)),
            },
            ManagedEntry {
                ownership_id: "skill.ossia".into(),
                path: path_text(skill_path(ctx)),
                kind: "skill".into(),
                sha256: Some(ContentHash::of(&skill)),
            },
        ])
    }
    fn sync(
        &self,
        ctx: &AgentContext,
        previous: &ManagedRecord,
    ) -> Result<Option<Vec<ManagedEntry>>, AppError> {
        let status = self.status(ctx, Some(previous))?;
        if status.state != AgentState::Installed {
            return Ok(None);
        }
        self.install(ctx, Some(previous), false).map(Some)
    }
    fn remove(&self, ctx: &AgentContext, previous: &ManagedRecord) -> Result<(), AppError> {
        let mut value = read(ctx)?;
        let found = conflicts(ctx, &value, Some(previous));
        if !found.is_empty() {
            return Err(conflict_error(&found));
        }
        let path = skill_path(ctx);
        if server(&value).is_some() {
            value
                .get_mut("mcpServers")
                .and_then(Value::as_object_mut)
                .unwrap()
                .remove("ossia");
        }
        let files = vec![
            FilePlan {
                path: config(ctx),
                bytes: serde_json::to_vec_pretty(&value).map_err(record::storage)?,
                kind: "json-entry",
            },
            FilePlan {
                path: path.clone(),
                bytes: vec![],
                kind: "skill",
            },
        ];
        let snapshot = snapshot_files(&files);
        let result = (|| {
            if config(ctx).exists() {
                record::atomic_write(&config(ctx), &files[0].bytes)?;
            }
            if path.exists() {
                fs::remove_file(path).map_err(record::storage)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            return match restore_files(&snapshot) {
                Ok(()) => Err(error),
                Err(rollback) => Err(record::storage(format!(
                    "{}; rollback failed: {}",
                    error.message, rollback.message
                ))),
            };
        }
        Ok(())
    }
    fn remove_legacy(&self, _: &AgentContext, _: &LegacyRecord) -> Result<(), AppError> {
        Ok(())
    }
}
