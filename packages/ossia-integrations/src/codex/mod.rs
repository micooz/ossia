use crate::files::{json_file, records_from_plan, zero_hash, FilePlan};
use crate::*;
use ossia_types::*;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const MARKETPLACE: &str = "ossia-local";
const PLUGIN: &str = "ossia";
const PLUGIN_ID: &str = "ossia@ossia-local";

mod detect;
mod lifecycle;
mod status;
#[cfg(test)]
mod tests;

pub use detect::detect;
pub(super) use detect::{host, Host};
pub use lifecycle::{apply, remove, remove_entries};
pub use status::status;

pub fn plan(
    integrator: &Integrator,
    profile: &str,
    mode: IntegrationMode,
    previous: Option<&ManagedRecord>,
) -> Result<IntegrationPlan, AppError> {
    plan_for(integrator, profile, mode, previous)
}

pub fn plan_for(
    integrator: &Integrator,
    profile: &str,
    mode: IntegrationMode,
    previous: Option<&ManagedRecord>,
) -> Result<IntegrationPlan, AppError> {
    if profile != "default" {
        return Err(AppError::new(
            ErrorCode::NotFound,
            "Codex uses CODEX_HOME rather than named profiles; select the default profile",
        ));
    }
    let detection = detect(integrator);
    let Some(host) = host(integrator, &detection) else {
        return Err(AppError::new(
            ErrorCode::NotFound,
            detection
                .problem
                .as_ref()
                .map(|problem| problem.message.clone())
                .unwrap_or_else(|| "Codex CLI is unavailable".into()),
        ));
    };
    let required = match mode {
        IntegrationMode::SkillMcp => "mcp",
        IntegrationMode::Plugin => "plugin",
    };
    if !detection.capabilities.iter().any(|item| item == required) {
        return Err(AppError::new(
            ErrorCode::UnsupportedSource,
            format!(
                "Codex {} does not advertise the {required} command",
                detection.version.as_deref().unwrap_or("unknown")
            ),
        ));
    }
    let mut conflicts = Vec::new();
    let mut warnings = Vec::new();
    let skill = integrator.skill()?;
    let skill_path = integrator.paths().agents_home.join("skills/ossia/SKILL.md");
    let desired_mcp = desired_mcp(integrator);
    let mut entries = Vec::new();
    let mut files = Vec::new();
    match mode {
        IntegrationMode::SkillMcp => {
            files.push(FilePlan {
                path: skill_path.clone(),
                bytes: skill.into_bytes(),
                kind: "skill",
            });
            let current = mcp_entries(&host)?;
            if let Some(current) = current.iter().find(|entry| entry.name == "ossia") {
                if !mcp_matches(current, &desired_mcp) || !record_owns(previous, &skill_path) {
                    conflicts.push(conflict(
                        "codex:mcp:ossia",
                        "user-config",
                        "an existing Codex MCP server named ossia is not the managed entry",
                    ));
                }
            }
            entries.push(IntegrationEntry {
                operation: IntegrationOperation::Create,
                ownership_id: "codex:mcp:ossia".into(),
                path: "codex:mcp:ossia".into(),
                kind: "host-entry".into(),
                sha256: None,
            });
        }
        IntegrationMode::Plugin => {
            files = plugin_files(integrator)?;
            let marketplaces = marketplace_entries(&host)?;
            if let Some(current) = marketplaces.iter().find(|entry| entry.name == MARKETPLACE) {
                let root = plugin_root(integrator);
                if current
                    .root
                    .as_ref()
                    .is_none_or(|current| !same_path(current, &root))
                    || !record_owns(previous, &root)
                {
                    conflicts.push(conflict(
                        "codex:marketplace:ossia-local",
                        "user-config",
                        "the local marketplace name ossia-local already belongs to another source",
                    ));
                }
            }
            let plugins = plugin_entries(&host)?;
            if let Some(current) = plugins.iter().find(|entry| entry.id == PLUGIN_ID) {
                if current
                    .source
                    .as_ref()
                    .and_then(|source| source.path.as_ref())
                    .is_none_or(|path| !same_path(path, &plugin_dir(integrator)))
                {
                    conflicts.push(conflict(
                        PLUGIN_ID,
                        "user-config",
                        "the plugin source no longer points to the managed marketplace entry",
                    ));
                } else if !record_owns_plugin(previous) {
                    conflicts.push(conflict(
                        PLUGIN_ID,
                        "user-config",
                        "the plugin ossia@ossia-local is already installed outside management",
                    ));
                } else if !current.enabled {
                    warnings.push("the installed plugin is disabled; enable it in the host".into());
                }
            }
            entries.push(IntegrationEntry {
                operation: IntegrationOperation::Create,
                ownership_id: "codex:marketplace:ossia-local".into(),
                path: "codex:marketplace:ossia-local".into(),
                kind: "host-entry".into(),
                sha256: None,
            });
            entries.push(IntegrationEntry {
                operation: IntegrationOperation::Create,
                ownership_id: PLUGIN_ID.into(),
                path: PLUGIN_ID.into(),
                kind: "host-entry".into(),
                sha256: None,
            });
        }
    }
    append_file_entries(previous, &files, &mut entries, &mut conflicts);
    if mode == IntegrationMode::SkillMcp && !previous.is_some_and(|value| value.mode == mode) {
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.ownership_id == "codex:mcp:ossia")
        {
            let _ = entry;
        }
    }
    Ok(IntegrationPlan {
        plan_hash: zero_hash(),
        management_id: record::management_id(HostKind::Codex, profile),
        host: HostKind::Codex,
        profile: profile.into(),
        mode,
        home: integrator.home().to_string_lossy().into_owned(),
        executable: integrator.executable().to_string_lossy().into_owned(),
        host_version: detection.version,
        entries,
        conflicts,
        warnings,
        requires_host_completion: mode == IntegrationMode::Plugin,
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpEntry {
    name: String,
    enabled: bool,
    transport: McpTransport,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct McpTransport {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    env: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone)]
struct DesiredMcp {
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MarketplaceEntry {
    name: String,
    root: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PluginEntry {
    #[serde(rename = "pluginId")]
    id: String,
    enabled: bool,
    #[serde(default)]
    source: Option<PluginSource>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PluginSource {
    #[serde(default)]
    path: Option<String>,
}

fn codex(host: &Host, args: &[String]) -> Result<String, AppError> {
    command::run(&host.executable, args, &host.env)
}

fn mcp_entries(host: &Host) -> Result<Vec<McpEntry>, AppError> {
    let text = codex(host, &["mcp".into(), "list".into(), "--json".into()])?;
    serde_json::from_str(&text).map_err(|error| {
        AppError::new(
            ErrorCode::RuntimeUnavailable,
            format!("Codex mcp list returned invalid JSON: {error}"),
        )
    })
}

fn marketplace_entries(host: &Host) -> Result<Vec<MarketplaceEntry>, AppError> {
    let text = codex(
        host,
        &[
            "plugin".into(),
            "marketplace".into(),
            "list".into(),
            "--json".into(),
        ],
    )?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| AppError::new(ErrorCode::RuntimeUnavailable, error.to_string()))?;
    serde_json::from_value(
        value
            .get("marketplaces")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(|error| AppError::new(ErrorCode::RuntimeUnavailable, error.to_string()))
}

fn plugin_entries(host: &Host) -> Result<Vec<PluginEntry>, AppError> {
    let text = codex(host, &["plugin".into(), "list".into(), "--json".into()])?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| AppError::new(ErrorCode::RuntimeUnavailable, error.to_string()))?;
    serde_json::from_value(value.get("installed").cloned().unwrap_or_else(|| json!([])))
        .map_err(|error| AppError::new(ErrorCode::RuntimeUnavailable, error.to_string()))
}

fn desired_mcp(integrator: &Integrator) -> DesiredMcp {
    DesiredMcp {
        command: integrator.executable().to_string_lossy().into_owned(),
        args: integrator.mcp_command(),
        env: integrator.host_env(),
    }
}

fn mcp_matches(entry: &McpEntry, desired: &DesiredMcp) -> bool {
    entry.transport.command.as_deref() == Some(desired.command.as_str())
        && entry.transport.args.as_ref() == Some(&desired.args)
        && entry.transport.env.as_ref().unwrap_or(&BTreeMap::new()) == &desired.env
}

fn plugin_root(integrator: &Integrator) -> PathBuf {
    record::integration_root(integrator.home()).join("codex/marketplace")
}

fn plugin_dir(integrator: &Integrator) -> PathBuf {
    plugin_root(integrator).join("plugins/ossia")
}

fn plugin_files(integrator: &Integrator) -> Result<Vec<FilePlan>, AppError> {
    let root = plugin_dir(integrator);
    let skill = integrator.skill()?;
    let icon = integrator.icon()?;
    let server = json!({
        "type": "stdio",
        "command": integrator.executable().to_string_lossy(),
        "args": integrator.mcp_command(),
        "env": integrator.host_env(),
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
        "extensions": {"com.openai": {"interface": interface.clone()}},
    });
    let portable_mcp = json!({
        "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
        "mcpServers": {"ossia": server.clone()},
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
            plugin_root(integrator).join(".agents/plugins/marketplace.json"),
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

fn append_file_entries(
    previous: Option<&ManagedRecord>,
    files: &[FilePlan],
    entries: &mut Vec<IntegrationEntry>,
    conflicts: &mut Vec<IntegrationConflict>,
) {
    for file in files {
        if file.path.exists() {
            if !record_owns(previous, &file.path) {
                conflicts.push(conflict(
                    file.path.to_string_lossy(),
                    "file",
                    "an unowned file already exists at the managed target",
                ));
                continue;
            }
            let unchanged = previous.is_some_and(|record| {
                record
                    .entries
                    .iter()
                    .find(|entry| entry.path == file.path.to_string_lossy())
                    .and_then(|entry| entry.sha256.as_ref())
                    == on_disk(&file.path).as_ref()
            });
            if !unchanged {
                conflicts.push(conflict(
                    file.path.to_string_lossy(),
                    "file",
                    "the managed file was modified after installation",
                ));
                continue;
            }
        }
        entries.push(file_entry(
            &file.path,
            &file.bytes,
            file.kind,
            &file.path.to_string_lossy(),
        ));
    }
}

fn record_owns(record: Option<&ManagedRecord>, path: &Path) -> bool {
    record.is_some_and(|record| {
        record
            .entries
            .iter()
            .any(|entry| entry.path == path.to_string_lossy() && entry.sha256.is_some())
    })
}

fn record_owns_plugin(record: Option<&ManagedRecord>) -> bool {
    record.is_some_and(|record| record.mode == IntegrationMode::Plugin)
}

fn same_path(left: &str, right: &Path) -> bool {
    Path::new(left)
        .canonicalize()
        .ok()
        .zip(right.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
        || Path::new(left) == right
}
