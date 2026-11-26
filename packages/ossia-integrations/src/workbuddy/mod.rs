use crate::resources::Resource;
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
#[path = "../../tests/workbuddy/mod.rs"]
mod tests;

const MARKETPLACE: &str = "ossia-workbuddy";
const PLUGIN: &str = "ossia";
const PLUGIN_ID: &str = "ossia@ossia-workbuddy";

pub(crate) struct WorkBuddy {
    instance: detect::AppInstance,
}

pub(crate) fn instances(paths: &HostPaths) -> Vec<WorkBuddy> {
    detect::instances(paths)
        .into_iter()
        .map(|instance| WorkBuddy { instance })
        .collect()
}

pub(crate) fn app_candidates(home: &Path) -> Vec<PathBuf> {
    detect::app_candidates(home)
}

impl AgentAdapter for WorkBuddy {
    fn id(&self) -> &str {
        &self.instance.id
    }

    fn name(&self) -> &str {
        "WorkBuddy"
    }

    fn icon(&self) -> &str {
        "workbuddy"
    }

    fn method(&self) -> InstallMethod {
        InstallMethod::Plugin
    }

    fn profile(&self) -> &'static str {
        "desktop"
    }

    fn reload_hint(&self) -> AgentNotice {
        AgentNotice::WorkbuddyReloadAndTrust
    }

    fn detect(&self, _paths: &HostPaths) -> Option<DetectedAgent> {
        detect::detect(&self.instance)
    }

    fn config_path(&self, ctx: &AgentContext) -> PathBuf {
        installed_plugins_path(ctx.detected)
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
        overwrite: bool,
    ) -> Result<Vec<ManagedEntry>, AppError> {
        lifecycle::install(ctx, previous, overwrite)
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
        delete_entries(&record.entries)?;
        cleanup_shared_dir(ctx);
        Ok(())
    }
}

fn storage_key(config_root: &Path) -> String {
    let mut hash = ContentHash::of(normalize_path(&path_text(config_root)).as_bytes()).to_string();
    hash.truncate(12);
    format!("workbuddy-{hash}")
}

fn shared_instance_root(ctx: &AgentContext) -> PathBuf {
    ctx.shared_root()
        .join("workbuddy")
        .join(storage_key(&ctx.detected.config_root))
}

fn marketplace_root(ctx: &AgentContext) -> PathBuf {
    shared_instance_root(ctx).join("marketplace")
}

fn plugin_dir(ctx: &AgentContext) -> PathBuf {
    marketplace_root(ctx).join("plugins").join(PLUGIN)
}

fn plugin_manifest_path(ctx: &AgentContext) -> PathBuf {
    plugin_dir(ctx)
        .join(".codebuddy-plugin")
        .join("plugin.json")
}

fn known_marketplaces_path(detected: &DetectedAgent) -> PathBuf {
    detected
        .config_root
        .join("plugins")
        .join("known_marketplaces.json")
}

fn installed_plugins_path(detected: &DetectedAgent) -> PathBuf {
    detected
        .config_root
        .join("plugins")
        .join("installed_plugins.json")
}

fn run(ctx: &AgentContext, args: &[&str]) -> Result<String, AppError> {
    let args = args
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    command::run_with_prefix(
        &ctx.detected.command,
        &ctx.detected.command_args,
        &args,
        &ctx.detected.env,
    )
}

fn read_json(path: &Path) -> Result<Option<Value>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(record::storage)?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        record::storage(format!(
            "invalid WorkBuddy configuration {}: {error}",
            path.display()
        ))
    })
}

fn plugin_entry(ctx: &AgentContext) -> Result<Option<Value>, AppError> {
    let text = run(ctx, &["plugin", "list", "--json"])?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        AppError::new(
            ErrorCode::RuntimeUnavailable,
            format!("WorkBuddy plugin list returned invalid JSON: {error}"),
        )
    })?;
    Ok(value.as_array().and_then(|items| {
        items
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(PLUGIN_ID))
            .cloned()
    }))
}

fn marketplace_entry(ctx: &AgentContext) -> Result<Option<Value>, AppError> {
    let value = read_json(&known_marketplaces_path(ctx.detected))?;
    Ok(value.and_then(|value| value.get(MARKETPLACE).cloned()))
}

fn marketplace_path(entry: &Value) -> Option<&str> {
    entry
        .get("installLocation")
        .and_then(Value::as_str)
        .or_else(|| entry.pointer("/source/path").and_then(Value::as_str))
}

fn same_path(left: &str, right: &Path) -> bool {
    Path::new(left)
        .canonicalize()
        .ok()
        .zip(right.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
        || normalize_path(left) == normalize_path(&path_text(right))
}

fn marketplace_matches(ctx: &AgentContext, entry: &Value) -> bool {
    marketplace_path(entry).is_some_and(|path| same_path(path, &marketplace_root(ctx)))
}

fn plugin_files(ctx: &AgentContext) -> Result<(String, Vec<FilePlan>), AppError> {
    let root = plugin_dir(ctx);
    let skill = ctx.skill()?;
    let icon = ctx.icon()?;
    let piano = ctx.resource(Resource::SkillPiano)?;
    let single_staff = ctx.resource(Resource::SkillSingleStaff)?;
    let guitar_tab = ctx.resource(Resource::SkillGuitarTab)?;
    let jianpu = ctx.resource(Resource::SkillJianpu)?;
    let server = json!({
        "type": "stdio",
        "command": path_text(ctx.executable()),
        "args": ctx.mcp_command(),
        "env": ctx.integrator.launch_env(),
    });
    let version = plugin_version(
        skill.as_bytes(),
        &piano,
        &single_staff,
        &guitar_tab,
        &jianpu,
        &icon,
        &server,
    );
    let manifest = json!({
        "name": PLUGIN,
        "version": version,
        "description": "Local score creation, editing, engraving, and export through the Ossia runtime.",
        "author": {"name": "Ossia"},
        "license": "GPL-3.0-or-later",
        "keywords": ["score", "music", "lilypond", "mcp"],
        "skills": "./skills/",
        "mcpServers": "./.mcp.json",
    });
    let marketplace = json!({
        "name": MARKETPLACE,
        "owner": {"name": "Ossia"},
        "plugins": [{
            "name": PLUGIN,
            "source": "./plugins/ossia",
            "description": "Local score editing through the Ossia runtime.",
            "version": version,
            "category": "Productivity"
        }]
    });
    let files = vec![
        json_file(
            marketplace_root(ctx)
                .join(".codebuddy-plugin")
                .join("marketplace.json"),
            marketplace,
            "marketplace",
        ),
        json_file(plugin_manifest_path(ctx), manifest, "plugin-manifest"),
        json_file(
            root.join(".mcp.json"),
            json!({"mcpServers": {"ossia": server}}),
            "mcp",
        ),
        FilePlan {
            path: root.join("skills/ossia/SKILL.md"),
            bytes: skill.into_bytes(),
            kind: "skill",
        },
        FilePlan {
            path: root.join("skills/ossia/references/piano.ly"),
            bytes: piano,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("skills/ossia/references/single-staff.ly"),
            bytes: single_staff,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("skills/ossia/references/guitar-tab.ly"),
            bytes: guitar_tab,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("skills/ossia/references/jianpu.ly"),
            bytes: jianpu,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("assets/ossia.svg"),
            bytes: icon,
            kind: "asset",
        },
    ];
    Ok((version, files))
}

fn plugin_version(
    skill: &[u8],
    piano: &[u8],
    single_staff: &[u8],
    guitar_tab: &[u8],
    jianpu: &[u8],
    icon: &[u8],
    server: &Value,
) -> String {
    let mut identity = Vec::new();
    identity.extend_from_slice(b"ossia-workbuddy\0");
    identity.extend_from_slice(&TEMPLATE_VERSION.to_be_bytes());
    for bytes in [skill, piano, single_staff, guitar_tab, jianpu, icon] {
        identity.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        identity.extend_from_slice(bytes);
    }
    identity.extend_from_slice(server.to_string().as_bytes());
    let hash = ContentHash::of(&identity).to_string();
    format!("{}+ossia.{}", env!("CARGO_PKG_VERSION"), &hash[..12])
}

fn delete_entries(entries: &[ManagedEntry]) -> Result<(), AppError> {
    verify_owned_files(entries)?;
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        if let Some(hash) = entry.sha256.as_ref() {
            delete_owned(Path::new(&entry.path), hash)?;
        }
    }
    Ok(())
}

fn cleanup_shared_dir(ctx: &AgentContext) {
    let _ = fs::remove_dir_all(shared_instance_root(ctx));
}
