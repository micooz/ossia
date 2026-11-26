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
#[path = "../../tests/dsh/mod.rs"]
mod tests;

const PLUGIN: &str = "ossia-dsh-plugin";

pub(crate) struct Dsh;

impl AgentAdapter for Dsh {
    fn id(&self) -> &'static str {
        "dsh"
    }

    fn name(&self) -> &'static str {
        "DeepSeek Harness"
    }

    fn icon(&self) -> &'static str {
        "dsh"
    }

    fn method(&self) -> InstallMethod {
        InstallMethod::Plugin
    }

    fn profile(&self) -> &'static str {
        "desktop"
    }

    fn visible_profile(&self) -> Option<&'static str> {
        Some(self.profile())
    }

    fn reload_hint(&self) -> AgentNotice {
        AgentNotice::RestartSession {
            agent: "DSH".into(),
        }
    }

    fn detect(&self, paths: &HostPaths) -> Option<DetectedAgent> {
        detect::detect(paths)
    }

    fn config_path(&self, ctx: &AgentContext) -> PathBuf {
        profile_path(ctx)
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
        lifecycle::remove_legacy(ctx, record)
    }
}

fn bundle_dir(ctx: &AgentContext) -> PathBuf {
    ctx.shared_root().join("dsh").join(PLUGIN)
}

fn profile_path(ctx: &AgentContext) -> PathBuf {
    ctx.integrator
        .paths()
        .dsh_home
        .join("profiles")
        .join(ctx.detected.profile)
        .join("package.json")
}

fn read_profile(ctx: &AgentContext) -> Result<Option<Value>, AppError> {
    let path = profile_path(ctx);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(record::storage)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| record::storage(format!("invalid DSH profile manifest: {error}")))
}

fn installed_bundles(profile: &Value) -> Vec<String> {
    profile
        .pointer("/dsh/profile/bundles")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn dependency(profile: &Value) -> Option<&str> {
    profile
        .get("dependencies")
        .and_then(Value::as_object)
        .and_then(|items| items.get(PLUGIN))
        .and_then(Value::as_str)
}

fn dependency_path(value: &str) -> &str {
    value
        .strip_prefix("link:")
        .or_else(|| value.strip_prefix("file:"))
        .unwrap_or(value)
}

fn dependency_has_verbatim_prefix(value: &str) -> bool {
    let path = dependency_path(value);
    path.starts_with(r"\\?\") || path.starts_with("//?/")
}

fn dependency_matches(value: &str, expected: &Path, base: &Path) -> bool {
    let path = normalize_path(dependency_path(value));
    let path = Path::new(&path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    same_path(&resolved.to_string_lossy(), expected)
}

pub(super) fn same_path(left: &str, right: &Path) -> bool {
    Path::new(left)
        .canonicalize()
        .ok()
        .zip(right.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
        || normalize_path(left) == normalize_path(&right.to_string_lossy())
}

pub(super) fn run(ctx: &AgentContext, args: &[&str]) -> Result<String, AppError> {
    let args = args
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    command::run(&ctx.detected.executable, &args, &ctx.detected.env)
}

pub(super) fn plugin_files(ctx: &AgentContext) -> Result<Vec<FilePlan>, AppError> {
    let root = bundle_dir(ctx);
    let skill = ctx.skill()?;
    let host = ctx.resource(Resource::DshHost)?;
    let client = ctx.resource(Resource::DshClient)?;
    let icon = ctx.icon()?;
    let locale_en = ctx.resource(Resource::DshLocaleEn)?;
    let locale_zh = ctx.resource(Resource::DshLocaleZh)?;
    let browser = browser_enabled(ctx);
    let skill_loader = format!(
        "export const inject = ['skills']\nexport function apply(ctx) {{\n  ctx.effect(() => ctx.skills.register({{ name: 'ossia', source: 'bundled', description: 'Manage, edit, and export scores in a local Ossia library using its MCP tools.', content: {} }}), 'ossia-skill')\n}}\n",
        serde_json::to_string(&skill).expect("skill serialization")
    );
    Ok(vec![
        json_file(
            root.join("package.json"),
            plugin_manifest(),
            "bundle-manifest",
        ),
        FilePlan {
            path: root.join("cordis.patch.yml"),
            bytes: plugin_patch(ctx, browser == Some(false)).into_bytes(),
            kind: "bundle-patch",
        },
        FilePlan {
            path: root.join("index.js"),
            bytes: host,
            kind: "host-entry",
        },
        json_file(
            root.join("config.json"),
            json!({
                "home": path_text(ctx.integrator.home()),
            }),
            "host-config",
        ),
        FilePlan {
            path: root.join("client.js"),
            bytes: client,
            kind: "client-entry",
        },
        FilePlan {
            path: root.join("skill.js"),
            bytes: skill_loader.into_bytes(),
            kind: "skill-loader",
        },
        FilePlan {
            path: root.join("skills/ossia/SKILL.md"),
            bytes: skill.into_bytes(),
            kind: "skill",
        },
        FilePlan {
            path: root.join("skills/ossia/references/piano.ly"),
            bytes: ctx.resource(Resource::SkillPiano)?,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("skills/ossia/references/single-staff.ly"),
            bytes: ctx.resource(Resource::SkillSingleStaff)?,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("skills/ossia/references/guitar-tab.ly"),
            bytes: ctx.resource(Resource::SkillGuitarTab)?,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("skills/ossia/references/jianpu.ly"),
            bytes: ctx.resource(Resource::SkillJianpu)?,
            kind: "skill-reference",
        },
        FilePlan {
            path: root.join("locale/en.json"),
            bytes: locale_en,
            kind: "locale",
        },
        FilePlan {
            path: root.join("locale/zh.json"),
            bytes: locale_zh,
            kind: "locale",
        },
        FilePlan {
            path: root.join("icon.svg"),
            bytes: icon,
            kind: "asset",
        },
    ])
}

fn plugin_manifest() -> Value {
    json!({
        "name": PLUGIN,
        "version": env!("CARGO_PKG_VERSION"),
        "private": true,
        "type": "module",
        "main": "index.js",
        "exports": {
            ".": "./index.js",
            "./client": "./client.js",
            "./skill": "./skill.js",
            "./config.json": "./config.json",
            "./locale/*.json": "./locale/*.json",
            "./icon": "./icon.svg",
            "./skills/*": "./skills/*"
        },
        "files": [
            "index.js",
            "config.json",
            "client.js",
            "skill.js",
            "cordis.patch.yml",
            "locale/*.json",
            "icon.svg",
            "skills/ossia/SKILL.md",
            "skills/ossia/references/*.ly"
        ],
        "engines": {"dsh": ">=0.2.2-rc.1 <0.3.0"},
        "dsh": {
            "bundle": {"patch": "./cordis.patch.yml"},
            "client": {
                "platform": "web",
                "immediately": true,
                "inject": []
            }
        }
    })
}

fn plugin_patch(ctx: &AgentContext, enable_browser: bool) -> String {
    let browser = if enable_browser {
        "- id: ui-sidebar-browser\n  name: '@deepseek-ai/dsh-client-ui-sidebar-browser'\n  disabled: false\n"
    } else {
        ""
    };
    format!(
        "{browser}- insert:\n    - id: ossia-plugin\n      name: 'ossia-dsh-plugin'\n    - id: ossia-mcp\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: ossia\n        transport: stdio\n        command: {}\n        args: {}\n        env: {}\n        failOnStartupError: true\n    - id: ossia-skill\n      name: 'ossia-dsh-plugin/skill'\n",
        yaml_string(&path_text(ctx.executable())),
        yaml_array(&ctx.mcp_command()),
        yaml_env(&ctx.integrator.launch_env()),
    )
}

fn browser_enabled(ctx: &AgentContext) -> Option<bool> {
    let text = match run(ctx, &["--profile", ctx.detected.profile, "--dump-config"]) {
        Ok(text) => text,
        Err(error)
            if ctx.detected.profile == "desktop"
                && error
                    .message
                    .contains("managed exclusively by the Electron application") =>
        {
            return Some(true);
        }
        Err(_) => return None,
    };
    let lines = text.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.contains("ui-sidebar-browser")
            && line.contains("@deepseek-ai/dsh-client-ui-sidebar-browser")
        {
            for following in lines.iter().skip(index + 1).take(8) {
                if following.trim_start().starts_with("disabled:") {
                    return Some(!following.contains("true"));
                }
            }
            return Some(true);
        }
    }
    Some(false)
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).expect("YAML string")
}

fn yaml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| yaml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn yaml_env(values: &std::collections::BTreeMap<String, String>) -> String {
    if values.is_empty() {
        return "{}".into();
    }
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| format!("{}: {}", yaml_string(key), yaml_string(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
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

pub(super) fn cleanup_shared_dir(shared: &Path) {
    let _ = fs::remove_dir_all(shared.join("dsh"));
}
