use crate::*;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

pub(crate) struct Antigravity;

#[cfg(test)]
#[path = "../../tests/antigravity/mod.rs"]
mod tests;

fn config(ctx: &AgentContext) -> PathBuf {
    ctx.detected.config_root.join("mcp_config.json")
}
fn skill_path(ctx: &AgentContext) -> PathBuf {
    ctx.detected.config_root.join("skills/ossia/SKILL.md")
}
fn read(ctx: &AgentContext) -> Result<Value, AppError> {
    let path = config(ctx);
    let value: Value = if path.exists() {
        parse_json_or_empty(&fs::read(path).map_err(record::storage)?)
            .map_err(|e| record::storage(format!("invalid Antigravity MCP configuration: {e}")))?
    } else {
        json!({})
    };
    if !value.is_object() || value.get("mcpServers").is_some_and(|v| !v.is_object()) {
        return Err(record::storage(
            "invalid Antigravity mcpServers configuration",
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
            "Antigravity MCP entry is not managed or was modified",
        ));
    }
    let path = skill_path(ctx);
    if path.exists()
        && !on_disk(&path).is_some_and(|hash| owned(previous, "skill.ossia", &hash, &path))
    {
        found.push(conflict(
            path_text(&path),
            "file",
            "Antigravity Ossia skill is not managed or was modified",
        ));
    }
    found
}
impl AgentAdapter for Antigravity {
    fn id(&self) -> &str {
        "antigravity"
    }
    fn name(&self) -> &str {
        "Antigravity"
    }
    fn icon(&self) -> &str {
        "antigravity"
    }
    fn method(&self) -> InstallMethod {
        InstallMethod::Mcp
    }
    fn profile(&self) -> &'static str {
        "default"
    }
    fn reload_hint(&self) -> AgentNotice {
        AgentNotice::RestartSession {
            agent: "Antigravity".into(),
        }
    }
    fn detect(&self, paths: &HostPaths) -> Option<DetectedAgent> {
        detected(paths, &hosts(paths))
    }

    fn config_path(&self, ctx: &AgentContext) -> PathBuf {
        config(ctx)
    }
    fn status(
        &self,
        ctx: &AgentContext,
        previous: Option<&ManagedRecord>,
    ) -> Result<AgentInspection, AppError> {
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

pub(crate) fn detected(paths: &HostPaths, hosts: &[Host]) -> Option<DetectedAgent> {
    let host = hosts.first()?;
    Some(DetectedAgent {
        command: host.executable.clone(),
        executable: host.executable.clone(),
        command_args: vec![],
        version: host.version.clone(),
        config_root: paths.workbuddy_home.join(".gemini/config"),
        profile: "default",
        env: Default::default(),
    })
}

#[derive(Clone)]
pub(crate) struct Host {
    pub(crate) name: &'static str,
    pub(crate) executable: PathBuf,
    pub(crate) version: String,
}

pub(crate) fn hosts(paths: &HostPaths) -> Vec<Host> {
    let mut found = Vec::new();
    let home = &paths.workbuddy_home;
    #[cfg(target_os = "macos")]
    let apps = vec![
        PathBuf::from("/Applications/Antigravity.app/Contents/MacOS/Antigravity"),
        home.join("Applications/Antigravity.app/Contents/MacOS/Antigravity"),
    ];
    #[cfg(windows)]
    let apps = scan::program_files("Antigravity")
        .into_iter()
        .map(|p| p.join("Antigravity.exe"))
        .collect::<Vec<_>>();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let apps = vec![
        PathBuf::from("/opt/Antigravity/Antigravity"),
        home.join(".local/opt/Antigravity/Antigravity"),
        home.join("Applications/Antigravity/Antigravity"),
    ];
    if let Some(executable) = apps
        .into_iter()
        .find(|p| p.is_file() && app_is_standalone(p))
    {
        #[cfg(target_os = "macos")]
        let version = executable
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| plist::Value::from_file(p.join("Info.plist")).ok())
            .and_then(|v| {
                v.as_dictionary()?
                    .get("CFBundleShortVersionString")?
                    .as_string()
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        #[cfg(not(target_os = "macos"))]
        let version = String::new();
        found.push(Host {
            name: "Antigravity",
            executable,
            version,
        });
    }
    if let Some(executable) = scan::find(
        "agy",
        absolute_env("OSSIA_ANTIGRAVITY_CLI_BIN").as_deref(),
        scan::common_bin("agy")
            .into_iter()
            .chain([home.join(".gemini/antigravity-cli/bin/agy")]),
    ) {
        let version = command::version(&executable, &Default::default()).unwrap_or_default();
        found.push(Host {
            name: "Antigravity CLI",
            executable,
            version,
        });
    }
    found
}

#[cfg(target_os = "macos")]
fn app_is_standalone(executable: &std::path::Path) -> bool {
    executable
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| plist::Value::from_file(p.join("Info.plist")).ok())
        .and_then(|v| {
            v.as_dictionary()?
                .get("CFBundleIdentifier")?
                .as_string()
                .map(str::to_owned)
        })
        .is_some_and(|id| id == "com.google.antigravity")
}
#[cfg(not(target_os = "macos"))]
fn app_is_standalone(_: &std::path::Path) -> bool {
    true
}

pub(crate) fn present_hosts(agent: &mut AgentIntegration, hosts: &[Host]) {
    agent.name = if hosts.len() == 1 {
        hosts[0].name
    } else {
        "Antigravity"
    }
    .into();
    agent.hosts = hosts
        .iter()
        .map(|host| AgentHost {
            name: host.name.into(),
            error: None,
            method: agent.method,
            state: agent.state,
            version: (!host.version.is_empty()).then(|| host.version.clone()),
            config_path: agent.config_path.clone(),
            reload_hint: Some(AgentNotice::RestartSession {
                agent: host.name.into(),
            }),
            conflicts: agent.conflicts.clone(),
        })
        .collect();
    if !agent.hosts.is_empty() {
        agent.reload_hint = None;
    }
}
