use crate::resources::Resource;
use crate::*;
use ossia_types::*;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod lifecycle;
mod status;
#[cfg(test)]
#[path = "../../tests/claude_code/mod.rs"]
mod tests;

const MARKETPLACE: &str = "ossia-claude";
const PLUGIN: &str = "ossia";
const PLUGIN_ID: &str = "ossia@ossia-claude";

pub(crate) struct ClaudeCode;

impl AgentAdapter for ClaudeCode {
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
        InstallMethod::Plugin
    }

    fn profile(&self) -> &'static str {
        "code"
    }

    fn visible_profile(&self) -> Option<&'static str> {
        Some("code")
    }

    fn reload_hint(&self) -> AgentNotice {
        AgentNotice::RestartSession {
            agent: "Claude Code".into(),
        }
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
        let explicit = std::env::var_os("OSSIA_CLAUDE_BIN").map(PathBuf::from);
        let Some((executable, shell_discovered)) =
            resolver.find("claude", explicit.as_deref(), scan::common_bin("claude"))
        else {
            return Ok(None);
        };
        let root = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| paths.workbuddy_home.join(".claude"));
        let env = command::host_env("CLAUDE_CONFIG_DIR", &root);
        let (env, text) =
            resolver.probe(&executable, env, &["plugin", "--help"], shell_discovered)?;
        let version = text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        Ok(Some(DetectedAgent {
            command: executable.clone(),
            command_args: Vec::new(),
            executable,
            version,
            config_root: root,
            profile: "code",
            env,
        }))
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
    format!("claude-{hash}")
}

fn shared_instance_root(ctx: &AgentContext) -> PathBuf {
    ctx.shared_root()
        .join("claude")
        .join(storage_key(&ctx.detected.config_root))
}

fn marketplace_root(ctx: &AgentContext) -> PathBuf {
    shared_instance_root(ctx).join("marketplace")
}

fn plugin_dir(ctx: &AgentContext) -> PathBuf {
    marketplace_root(ctx).join("plugins").join(PLUGIN)
}

fn plugin_manifest_path(ctx: &AgentContext) -> PathBuf {
    plugin_dir(ctx).join(".claude-plugin").join("plugin.json")
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
        .pointer("/source/path")
        .and_then(Value::as_str)
        .or_else(|| entry.get("installLocation").and_then(Value::as_str))
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
                .join(".claude-plugin")
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
    identity.extend_from_slice(b"ossia-claude\0");
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
    let root = shared_instance_root(ctx);
    for path in [
        root.join("marketplace/plugins/ossia/skills/ossia/references"),
        root.join("marketplace/plugins/ossia/skills/ossia"),
        root.join("marketplace/plugins/ossia/skills"),
        root.join("marketplace/plugins/ossia/.claude-plugin"),
        root.join("marketplace/plugins/ossia/assets"),
        root.join("marketplace/plugins/ossia"),
        root.join("marketplace/plugins"),
        root.join("marketplace/.claude-plugin"),
        root.join("marketplace"),
        root,
    ] {
        let _ = fs::remove_dir(path);
    }
}
