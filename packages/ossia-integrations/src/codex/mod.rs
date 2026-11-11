use crate::*;
use ossia_types::*;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod detect;
mod lifecycle;
mod status;
#[cfg(test)]
mod tests;

const MARKETPLACE: &str = "ossia-local";
const PLUGIN: &str = "ossia";
const PLUGIN_ID: &str = "ossia@ossia-local";

pub(crate) struct Codex;

impl AgentAdapter for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn name(&self) -> &'static str {
        "Codex"
    }

    fn icon(&self) -> &'static str {
        "codex"
    }

    fn method(&self) -> InstallMethod {
        InstallMethod::Plugin
    }

    fn profile(&self) -> &'static str {
        "default"
    }

    fn reload_hint(&self) -> &'static str {
        "Codex 已运行的会话不会热更新；新建会话或重启 Codex 后加载最新插件。"
    }

    fn detect(&self, paths: &HostPaths) -> Option<DetectedAgent> {
        detect::detect(paths)
    }

    fn config_path(&self, ctx: &AgentContext) -> PathBuf {
        ctx.detected.config_root.join("config.toml")
    }

    fn status(
        &self,
        ctx: &AgentContext,
        record: Option<&ManagedRecord>,
    ) -> Result<AgentInspection, AppError> {
        status::status(ctx, record)
    }

    fn install(
        &self,
        ctx: &AgentContext,
        previous: Option<&ManagedRecord>,
    ) -> Result<Vec<ManagedEntry>, AppError> {
        lifecycle::install(ctx, previous)
    }

    fn sync(
        &self,
        ctx: &AgentContext,
        record: &ManagedRecord,
    ) -> Result<Option<Vec<ManagedEntry>>, AppError> {
        lifecycle::sync(ctx, record)
    }

    fn remove(&self, ctx: &AgentContext, record: &ManagedRecord) -> Result<(), AppError> {
        lifecycle::remove(ctx, record)
    }

    fn remove_legacy(&self, ctx: &AgentContext, record: &LegacyRecord) -> Result<(), AppError> {
        lifecycle::remove_legacy(ctx, record)
    }
}

fn marketplace_root(shared: &Path) -> PathBuf {
    shared.join("codex/marketplace")
}

fn plugin_dir(shared: &Path) -> PathBuf {
    marketplace_root(shared).join("plugins/ossia")
}

pub(super) fn plugin_files(ctx: &AgentContext) -> Result<Vec<FilePlan>, AppError> {
    let root = plugin_dir(ctx.shared_root());
    let skill = ctx.skill()?;
    let icon = ctx.icon()?;
    let server = json!({
        "type": "stdio",
        "command": ctx.executable().to_string_lossy(),
        "args": ctx.mcp_command(),
        "env": ctx.integrator.paths().launch_env,
    });
    let interface = json!({
        "displayName": "Ossia",
        "shortDescription": "Edit local scores with LilyPond",
        "longDescription": "Use the local Ossia runtime to inspect, edit, render, and version LilyPond scores.",
        "developerName": "Ossia",
        "category": "Productivity",
        "capabilities": ["Read", "Write"],
        "composerIcon": "./assets/ossia.svg",
        "logo": "./assets/ossia.svg",
        "defaultPrompt": ["Open my Ossia score library", "Edit the selected score measure"],
    });
    let manifest = json!({
        "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
        "name": PLUGIN,
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Local LilyPond score editing through the Ossia runtime.",
        "author": {"name": "Ossia"},
        "license": "GPL-3.0-or-later",
        "keywords": ["score", "music", "lilypond", "mcp"],
        "skills": "./skills/",
        "mcpServers": "./mcp.json",
        "extensions": {"com.openai": {"interface": interface}},
    });
    let portable_mcp = json!({
        "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
        "mcpServers": {"ossia": server},
    });
    let marketplace = json!({
        "name": MARKETPLACE,
        "interface": {"displayName": "Ossia Local"},
        "plugins": [{
            "name": PLUGIN,
            "source": {"source": "local", "path": "./plugins/ossia"},
            "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
            "category": "Productivity"
        }]
    });
    Ok(vec![
        json_file(
            marketplace_root(ctx.shared_root()).join(".agents/plugins/marketplace.json"),
            marketplace,
            "marketplace",
        ),
        json_file(root.join("plugin.json"), manifest, "plugin-manifest"),
        json_file(root.join("mcp.json"), portable_mcp, "mcp"),
        FilePlan {
            path: root.join("skills/ossia/SKILL.md"),
            bytes: skill.into_bytes(),
            kind: "skill",
        },
        FilePlan {
            path: root.join("assets/ossia.svg"),
            bytes: icon,
            kind: "asset",
        },
    ])
}

pub(super) fn plugin_entry(ctx: &AgentContext) -> Result<Option<Value>, AppError> {
    let text = run(ctx, &["plugin", "list", "--json"])?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        AppError::new(
            ErrorCode::RuntimeUnavailable,
            format!("Codex plugin list returned invalid JSON: {error}"),
        )
    })?;
    Ok(value
        .get("installed")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("pluginId").and_then(Value::as_str) == Some(PLUGIN_ID))
                .cloned()
        }))
}

pub(super) fn marketplace_entry(ctx: &AgentContext) -> Result<Option<Value>, AppError> {
    let text = run(ctx, &["plugin", "marketplace", "list", "--json"])?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        AppError::new(
            ErrorCode::RuntimeUnavailable,
            format!("Codex marketplace list returned invalid JSON: {error}"),
        )
    })?;
    Ok(value
        .get("marketplaces")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some(MARKETPLACE))
                .cloned()
        }))
}

pub(super) fn run(ctx: &AgentContext, args: &[&str]) -> Result<String, AppError> {
    let args = args
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    command::run(&ctx.detected.executable, &args, &ctx.detected.env)
}

pub(super) fn mcp_entry_exists(ctx: &AgentContext) -> Result<bool, AppError> {
    let text = run(ctx, &["mcp", "list", "--json"])?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        AppError::new(
            ErrorCode::RuntimeUnavailable,
            format!("Codex mcp list returned invalid JSON: {error}"),
        )
    })?;
    let items = value
        .get("servers")
        .or_else(|| value.get("mcps"))
        .or_else(|| value.as_array().map(|_| &value));
    Ok(items.and_then(Value::as_array).is_some_and(|items| {
        items
            .iter()
            .any(|item| item.get("name").and_then(Value::as_str) == Some("ossia"))
    }))
}

pub(super) fn same_path(left: &str, right: &Path) -> bool {
    Path::new(left)
        .canonicalize()
        .ok()
        .zip(right.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
        || Path::new(left) == right
}

pub(super) fn cleanup_shared_dir(shared: &Path) {
    let _ = fs::remove_dir_all(shared.join("codex"));
}

pub(super) fn delete_entries(entries: &[ManagedEntry]) -> Result<(), AppError> {
    verify_owned_files(entries)?;
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        if let Some(hash) = entry.sha256.as_ref() {
            delete_owned(Path::new(&entry.path), hash)?;
        }
    }
    Ok(())
}
