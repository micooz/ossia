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

#[derive(Debug, Clone)]
struct Host {
    executable: PathBuf,
    env: BTreeMap<String, String>,
}

pub fn detect(integrator: &Integrator) -> HostDetection {
    let paths = integrator.paths();
    let config_root = paths.codex_home.to_string_lossy().into_owned();
    let skill_root = paths
        .agents_home
        .join("skills")
        .to_string_lossy()
        .into_owned();
    let Some(executable) = command::find_program("codex", paths.codex_bin.as_deref()) else {
        return HostDetection {
            host: HostKind::Codex,
            installed: false,
            version: None,
            executable: None,
            config_root,
            skill_root: Some(skill_root),
            profiles: vec!["default".into()],
            capabilities: vec![],
            problem: Some(AppError::new(
                ErrorCode::NotFound,
                "Codex CLI was not found; set OSSIA_CODEX_BIN or install the Codex app",
            )),
        };
    };
    let env = command::host_env("CODEX_HOME", &paths.codex_home);
    match command::version(&executable, &env) {
        Ok(version) => {
            let mut capabilities = BTreeSet::new();
            if command::run(&executable, &command::arguments(&["mcp", "--help"]), &env).is_ok() {
                capabilities.insert("mcp");
            }
            if command::run(
                &executable,
                &command::arguments(&["plugin", "--help"]),
                &env,
            )
            .is_ok()
            {
                capabilities.insert("plugin");
            }
            HostDetection {
                host: HostKind::Codex,
                installed: true,
                version: Some(version),
                executable: Some(executable.to_string_lossy().into_owned()),
                config_root,
                skill_root: Some(skill_root),
                profiles: vec!["default".into()],
                capabilities: capabilities.into_iter().map(str::to_string).collect(),
                problem: None,
            }
        }
        Err(problem) => HostDetection {
            host: HostKind::Codex,
            installed: false,
            version: None,
            executable: Some(executable.to_string_lossy().into_owned()),
            config_root,
            skill_root: Some(skill_root),
            profiles: vec!["default".into()],
            capabilities: vec![],
            problem: Some(problem),
        },
    }
}

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

pub fn apply(
    integrator: &Integrator,
    plan: &IntegrationPlan,
) -> Result<Vec<ManagedEntry>, AppError> {
    let detection = detect(integrator);
    let Some(host) = host(integrator, &detection) else {
        return Err(AppError::new(ErrorCode::NotFound, "Codex CLI disappeared"));
    };
    let files = match plan.mode {
        IntegrationMode::SkillMcp => vec![FilePlan {
            path: integrator.paths().agents_home.join("skills/ossia/SKILL.md"),
            bytes: integrator.skill()?.into_bytes(),
            kind: "skill",
        }],
        IntegrationMode::Plugin => plugin_files(integrator)?,
    };
    for file in &files {
        record::atomic_write(&file.path, &file.bytes)?;
    }
    match plan.mode {
        IntegrationMode::SkillMcp => {
            let desired = desired_mcp(integrator);
            let current = mcp_entries(&host)?;
            if let Some(entry) = current.iter().find(|entry| entry.name == "ossia") {
                if !mcp_matches(entry, &desired) {
                    codex(&host, &command::arguments(&["mcp", "remove", "ossia"]))?;
                }
            }
            if !mcp_entries(&host)?
                .iter()
                .any(|entry| entry.name == "ossia" && mcp_matches(entry, &desired))
            {
                let mut args = command::arguments(&["mcp", "add", "ossia"]);
                for (key, value) in &desired.env {
                    args.push("--env".into());
                    args.push(format!("{key}={value}"));
                }
                args.push("--".into());
                args.push(integrator.executable().to_string_lossy().into_owned());
                args.extend(desired.args.iter().cloned());
                codex(&host, &args)?;
            }
        }
        IntegrationMode::Plugin => {
            let marketplaces = marketplace_entries(&host)?;
            if !marketplaces.iter().any(|entry| {
                entry.name == MARKETPLACE
                    && entry
                        .root
                        .as_ref()
                        .is_some_and(|root| same_path(root, &plugin_root(integrator)))
            }) {
                codex(
                    &host,
                    &[
                        "plugin".into(),
                        "marketplace".into(),
                        "add".into(),
                        plugin_root(integrator).to_string_lossy().into_owned(),
                    ],
                )?;
            }
            let plugins = plugin_entries(&host)?;
            if !plugins.iter().any(|entry| entry.id == PLUGIN_ID) {
                codex(&host, &["plugin".into(), "add".into(), PLUGIN_ID.into()])?;
            }
        }
    }
    Ok(records_from_plan(plan, &files))
}

pub fn status(
    integrator: &Integrator,
    _profile: &str,
    mode: Option<IntegrationMode>,
    record: Option<&ManagedRecord>,
) -> Result<HostStatus, AppError> {
    let detection = detect(integrator);
    let Some(host) = host(integrator, &detection) else {
        return Ok((
            IntegrationState::Unavailable,
            vec![],
            vec![],
            Some("Codex CLI is unavailable".into()),
        ));
    };
    let mut observations = Vec::new();
    let mut conflicts = Vec::new();
    if let Some(requested) = mode {
        if let Some(record) = record {
            if record.mode != requested {
                conflicts.push(conflict(
                    record.management_id.clone(),
                    "mode",
                    "the managed Codex integration uses another mode",
                ));
            }
        }
    }
    let effective_mode = mode.or(record.map(|value| value.mode));
    let Some(effective_mode) = effective_mode else {
        return Ok((
            IntegrationState::Available,
            observations,
            conflicts,
            Some("Codex is detected; choose skill-mcp or plugin".into()),
        ));
    };
    if record.is_none() {
        let unmanaged = match effective_mode {
            IntegrationMode::SkillMcp => mcp_entries(&host)?
                .iter()
                .any(|entry| entry.name == "ossia"),
            IntegrationMode::Plugin => plugin_entries(&host)?
                .iter()
                .any(|entry| entry.id == PLUGIN_ID),
        };
        if !unmanaged {
            return Ok((
                IntegrationState::Available,
                observations,
                conflicts,
                Some("no managed integration is installed".into()),
            ));
        }
    }
    if let Some(record) = record {
        for entry in record.entries.iter().filter(|entry| entry.sha256.is_some()) {
            let path = PathBuf::from(&entry.path);
            let state = match (entry.sha256.as_ref(), on_disk(&path)) {
                (Some(expected), Some(actual)) if expected == &actual => {
                    IntegrationState::Installed
                }
                (_, None) => IntegrationState::Broken,
                _ => IntegrationState::Conflict,
            };
            if state != IntegrationState::Installed {
                conflicts.push(conflict(
                    &entry.path,
                    "file",
                    "managed file is missing or was modified",
                ));
            }
            observations.push(observation(entry.path.clone(), &entry.kind, state, None));
        }
    }
    match effective_mode {
        IntegrationMode::SkillMcp => {
            let desired = desired_mcp(integrator);
            match mcp_entries(&host)?
                .into_iter()
                .find(|entry| entry.name == "ossia")
            {
                None => {
                    observations.push(observation(
                        "codex:mcp:ossia",
                        "host-entry",
                        IntegrationState::Broken,
                        None,
                    ));
                    conflicts.push(conflict(
                        "codex:mcp:ossia",
                        "missing",
                        "Codex has no ossia MCP server",
                    ));
                }
                Some(entry) if mcp_matches(&entry, &desired) && entry.enabled => {
                    observations.push(observation(
                        "codex:mcp:ossia",
                        "host-entry",
                        IntegrationState::Installed,
                        None,
                    ));
                }
                Some(_) => {
                    observations.push(observation(
                        "codex:mcp:ossia",
                        "host-entry",
                        IntegrationState::Conflict,
                        None,
                    ));
                    conflicts.push(conflict(
                        "codex:mcp:ossia",
                        "host-entry",
                        "ossia MCP configuration differs from the managed entry",
                    ));
                }
            }
        }
        IntegrationMode::Plugin => {
            let market = marketplace_entries(&host)?
                .into_iter()
                .find(|entry| entry.name == MARKETPLACE);
            match market {
                Some(entry)
                    if entry
                        .root
                        .as_ref()
                        .is_some_and(|root| same_path(root, &plugin_root(integrator))) =>
                {
                    observations.push(observation(
                        "codex:marketplace:ossia-local",
                        "host-entry",
                        IntegrationState::Installed,
                        None,
                    ));
                }
                _ => {
                    observations.push(observation(
                        "codex:marketplace:ossia-local",
                        "host-entry",
                        IntegrationState::Broken,
                        None,
                    ));
                    conflicts.push(conflict(
                        "codex:marketplace:ossia-local",
                        "missing",
                        "local marketplace is not registered",
                    ));
                }
            }
            match plugin_entries(&host)?
                .into_iter()
                .find(|entry| entry.id == PLUGIN_ID)
            {
                Some(entry)
                    if entry.enabled
                        && entry
                            .source
                            .as_ref()
                            .and_then(|source| source.path.as_ref())
                            .is_some_and(|path| same_path(path, &plugin_dir(integrator))) =>
                {
                    observations.push(observation(
                        PLUGIN_ID,
                        "host-entry",
                        IntegrationState::Installed,
                        None,
                    ));
                }
                Some(entry) if entry.enabled => {
                    observations.push(observation(
                        PLUGIN_ID,
                        "host-entry",
                        IntegrationState::Conflict,
                        None,
                    ));
                    conflicts.push(conflict(
                        PLUGIN_ID,
                        "host-entry",
                        "the installed plugin source no longer matches the managed marketplace",
                    ));
                }
                Some(_) => {
                    observations.push(observation(
                        PLUGIN_ID,
                        "host-entry",
                        IntegrationState::PendingHost,
                        None,
                    ));
                }
                None => {
                    observations.push(observation(
                        PLUGIN_ID,
                        "host-entry",
                        IntegrationState::Broken,
                        None,
                    ));
                    conflicts.push(conflict(PLUGIN_ID, "missing", "plugin is not installed"));
                }
            }
        }
    }
    if record.is_none() {
        conflicts.push(conflict(
            match effective_mode {
                IntegrationMode::SkillMcp => "codex:mcp:ossia",
                IntegrationMode::Plugin => PLUGIN_ID,
            },
            "unmanaged",
            "host configuration exists without an ownership record",
        ));
    }
    let state = if !conflicts.is_empty() {
        IntegrationState::Conflict
    } else if observations
        .iter()
        .any(|item| item.state == IntegrationState::PendingHost)
    {
        IntegrationState::PendingHost
    } else if observations
        .iter()
        .any(|item| item.state == IntegrationState::Broken)
    {
        IntegrationState::Broken
    } else if observations.is_empty() && record.is_none() {
        IntegrationState::Available
    } else {
        IntegrationState::Installed
    };
    if effective_mode == IntegrationMode::Plugin && state == IntegrationState::Installed {
        return Ok((
            IntegrationState::PendingHost,
            observations,
            conflicts,
            Some(
                "restart Codex and enable the local plugin before treating it as connected".into(),
            ),
        ));
    }
    Ok((
        state,
        observations,
        conflicts.clone(),
        if state == IntegrationState::PendingHost {
            Some("restart or enable the plugin in Codex".into())
        } else {
            None
        },
    ))
}

pub fn remove(integrator: &Integrator, record: &ManagedRecord) -> Result<(), AppError> {
    remove_entries(integrator, &record.profile, record.mode, &record.entries)
}

pub fn remove_entries(
    integrator: &Integrator,
    _profile: &str,
    mode: IntegrationMode,
    entries: &[ManagedEntry],
) -> Result<(), AppError> {
    verify_owned_files(entries)?;
    let detection = detect(integrator);
    let host = host(integrator, &detection);
    if entries.iter().any(|entry| entry.kind == "host-entry") && host.is_none() {
        return Err(AppError::new(
            ErrorCode::RuntimeUnavailable,
            "Codex CLI is unavailable; the managed files were left unchanged",
        ));
    }
    if let Some(host) = host.as_ref() {
        if mode == IntegrationMode::Plugin {
            let plugins = plugin_entries(host)?;
            if let Some(entry) = plugins.iter().find(|entry| entry.id == PLUGIN_ID) {
                if entry
                    .source
                    .as_ref()
                    .and_then(|source| source.path.as_ref())
                    .is_none_or(|path| !same_path(path, &plugin_dir(integrator)))
                {
                    return Err(AppError::new(
                        ErrorCode::Conflict,
                        "the installed plugin source no longer matches the managed marketplace",
                    ));
                }
                codex(host, &["plugin".into(), "remove".into(), PLUGIN_ID.into()])?;
            }
            let markets = marketplace_entries(host)?;
            if markets.iter().any(|entry| {
                entry.name == MARKETPLACE
                    && entry
                        .root
                        .as_ref()
                        .is_some_and(|root| same_path(root, &plugin_root(integrator)))
            }) {
                codex(
                    host,
                    &[
                        "plugin".into(),
                        "marketplace".into(),
                        "remove".into(),
                        MARKETPLACE.into(),
                    ],
                )?;
            }
        } else {
            let desired = desired_mcp(integrator);
            let current = mcp_entries(host)?;
            if let Some(entry) = current.iter().find(|entry| entry.name == "ossia") {
                if mcp_matches(entry, &desired) {
                    codex(host, &command::arguments(&["mcp", "remove", "ossia"]))?;
                } else {
                    return Err(AppError::new(
                        ErrorCode::Conflict,
                        "ossia MCP configuration was modified; remove it manually",
                    ));
                }
            }
        }
    }
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        if let Some(hash) = entry.sha256.as_ref() {
            delete_owned(Path::new(&entry.path), hash)?;
        }
    }
    if mode == IntegrationMode::Plugin {
        let _ = fs::remove_dir_all(plugin_root(integrator));
    } else {
        let skill_dir = integrator.paths().agents_home.join("skills/ossia");
        let _ = fs::remove_dir(&skill_dir);
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct FilePlan {
    path: PathBuf,
    bytes: Vec<u8>,
    kind: &'static str,
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

fn host(integrator: &Integrator, detection: &HostDetection) -> Option<Host> {
    detection.executable.as_ref().map(|executable| Host {
        executable: PathBuf::from(executable),
        env: command::host_env("CODEX_HOME", &integrator.paths().codex_home),
    })
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

fn json_file(path: PathBuf, value: Value, kind: &'static str) -> FilePlan {
    FilePlan {
        path,
        bytes: serde_json::to_vec_pretty(&value).expect("JSON serialization"),
        kind,
    }
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

fn records_from_plan(plan: &IntegrationPlan, files: &[FilePlan]) -> Vec<ManagedEntry> {
    plan.entries
        .iter()
        .map(|entry| ManagedEntry {
            ownership_id: entry.ownership_id.clone(),
            path: entry.path.clone(),
            kind: entry.kind.clone(),
            sha256: entry.sha256.clone().or_else(|| {
                files
                    .iter()
                    .find(|file| file.path.to_string_lossy() == entry.path)
                    .map(|file| ContentHash::of(&file.bytes))
            }),
        })
        .collect()
}

fn same_path(left: &str, right: &Path) -> bool {
    Path::new(left)
        .canonicalize()
        .ok()
        .zip(right.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
        || Path::new(left) == right
}

fn zero_hash() -> ContentHash {
    serde_json::from_value(Value::String("0".repeat(64))).expect("valid zero hash")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Integrator) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::HostPaths {
            codex_home: root.path().join("codex"),
            agents_home: root.path().join("agents"),
            dsh_home: root.path().join("dsh"),
            codex_bin: None,
            dsh_bin: None,
            launch_env: BTreeMap::new(),
        };
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../integrations");
        let integrator = Integrator::with_paths(
            root.path().join("library"),
            PathBuf::from("/bin/sh"),
            resources,
            paths,
        )
        .unwrap();
        (root, integrator)
    }

    #[test]
    fn plugin_files_are_self_contained_and_point_to_the_absolute_runtime() {
        let (_root, integrator) = fixture();
        let files = plugin_files(&integrator).unwrap();
        assert_eq!(files.len(), 5);

        let marketplace = files
            .iter()
            .find(|file| file.kind == "marketplace")
            .unwrap();
        let marketplace: Value = serde_json::from_slice(&marketplace.bytes).unwrap();
        assert_eq!(marketplace["name"], MARKETPLACE);
        assert_eq!(
            marketplace["plugins"][0]["source"]["path"],
            "./plugins/ossia"
        );

        let mcp = files.iter().find(|file| file.kind == "mcp").unwrap();
        let mcp: Value = serde_json::from_slice(&mcp.bytes).unwrap();
        assert_eq!(mcp["mcpServers"]["ossia"]["command"], "/bin/sh");
        assert_eq!(
            mcp["mcpServers"]["ossia"]["args"],
            json!(["mcp", "--home", integrator.home().to_string_lossy()])
        );
        assert!(files
            .iter()
            .any(|file| file.kind == "skill" && file.path.ends_with("skills/ossia/SKILL.md")));
    }

    #[test]
    fn mcp_matching_includes_args_and_environment() {
        let desired = DesiredMcp {
            command: "/bin/ossia".into(),
            args: vec!["mcp".into(), "--home".into(), "/tmp/a".into()],
            env: BTreeMap::from([("OSSIA_ENGINE_ROOT".into(), "/tmp/engine".into())]),
        };
        let entry = McpEntry {
            name: "ossia".into(),
            enabled: true,
            transport: McpTransport {
                command: Some(desired.command.clone()),
                args: Some(desired.args.clone()),
                env: Some(desired.env.clone()),
            },
        };
        assert!(mcp_matches(&entry, &desired));
        let mut mismatch = entry;
        mismatch.transport.env = Some(BTreeMap::new());
        assert!(!mcp_matches(&mismatch, &desired));
    }
}
