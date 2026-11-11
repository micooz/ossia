use crate::files::{json_file, records_from_plan, zero_hash, FilePlan};
use crate::resources::Resource;
use crate::*;
use ossia_types::*;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SKILL_MCP: &str = "ossia-dsh-skill-mcp";
const PLUGIN: &str = "ossia-dsh-plugin";

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
    let detection = detect(integrator);
    let Some(host) = host(integrator, &detection) else {
        return Err(AppError::new(
            ErrorCode::NotFound,
            detection
                .problem
                .as_ref()
                .map(|problem| problem.message.clone())
                .unwrap_or_else(|| "DSH CLI is unavailable".into()),
        ));
    };
    if !detection.profiles.iter().any(|value| value == profile) {
        return Err(AppError::new(
            ErrorCode::NotFound,
            format!("DSH profile {profile} was not found"),
        ));
    }
    if !detection.capabilities.iter().any(|value| value == "plugin") {
        return Err(AppError::new(
            ErrorCode::UnsupportedSource,
            "this DSH version does not expose local plugin installation",
        ));
    }
    let profile_path = profile_package(&integrator.paths().dsh_home, profile);
    let profile_json = read_json(&profile_path, "DSH profile manifest")?;
    let package_name = package_name(mode);
    let bundles = installed_bundles(&profile_json)?;
    let dependencies = profile_json
        .get("dependencies")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut conflicts = Vec::new();
    let mut warnings = Vec::new();
    if bundles.iter().any(|bundle| bundle == package_name)
        && !previous.is_some_and(|record| record.mode == mode)
    {
        conflicts.push(conflict(
            package_name,
            "user-config",
            "the DSH bundle name is already installed outside management",
        ));
    }
    if dependencies.contains_key(package_name)
        && !previous.is_some_and(|record| record.mode == mode)
    {
        conflicts.push(conflict(
            package_name,
            "user-config",
            "the DSH profile dependency already exists outside management",
        ));
    }
    if previous.is_some_and(|record| record.mode == mode)
        && dependencies
            .get(package_name)
            .and_then(Value::as_str)
            .is_some_and(|value| !dependency_matches(value, &bundle_dir(integrator, mode)))
    {
        conflicts.push(conflict(
            package_name,
            "user-config",
            "the managed DSH bundle link points to another location",
        ));
    }
    let skill = integrator.skill()?;
    let skill_path = integrator.paths().dsh_home.join("skills/ossia/SKILL.md");
    let mut files = match mode {
        IntegrationMode::SkillMcp => {
            let browser = browser_enabled(&host.executable, &host.env, profile);
            if browser.is_none() {
                warnings.push(
                    "DSH profile browser state is unavailable; Browser was not changed".into(),
                );
            }
            vec![
                FilePlan {
                    path: skill_path.clone(),
                    bytes: skill.clone().into_bytes(),
                    kind: "skill",
                },
                json_file(
                    bundle_dir(integrator, mode).join("package.json"),
                    skill_mcp_manifest(integrator),
                    "bundle-manifest",
                ),
                FilePlan {
                    path: bundle_dir(integrator, mode).join("cordis.patch.yml"),
                    bytes: skill_mcp_patch(integrator).into_bytes(),
                    kind: "bundle-patch",
                },
            ]
        }
        IntegrationMode::Plugin => {
            let browser = browser_enabled(&host.executable, &host.env, profile);
            if browser.is_none() {
                warnings.push(
                    "DSH profile browser state is unavailable; Browser was not changed".into(),
                );
            }
            plugin_files(integrator, &skill, browser == Some(false))?
        }
    };
    if skill_path.exists() && !record_owns(previous, &skill_path) {
        conflicts.push(conflict(
            skill_path.to_string_lossy(),
            "file",
            "an unowned Ossia skill already exists in the DSH user skill directory",
        ));
    }
    let mut entries = Vec::new();
    append_file_entries(previous, &mut files, &mut entries, &mut conflicts);
    entries.push(IntegrationEntry {
        operation: IntegrationOperation::Create,
        ownership_id: format!("dsh:bundle:{package_name}"),
        path: format!("dsh:bundle:{package_name}"),
        kind: "host-entry".into(),
        sha256: None,
    });
    Ok(IntegrationPlan {
        plan_hash: zero_hash(),
        management_id: record::management_id(HostKind::Dsh, profile),
        host: HostKind::Dsh,
        profile: profile.into(),
        mode,
        home: integrator.home().to_string_lossy().into_owned(),
        executable: integrator.executable().to_string_lossy().into_owned(),
        host_version: detection.version,
        entries,
        conflicts,
        warnings,
        requires_host_completion: false,
    })
}

fn dsh(host: &Host, args: &[String]) -> Result<String, AppError> {
    command::run(&host.executable, args, &host.env)
}

fn profiles(root: &Path) -> Vec<String> {
    let mut values = fs::read_dir(root.join("profiles"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path().join("package.json");
            read_json(&path, "profile manifest")
                .ok()
                .and_then(|value| value.pointer("/dsh/profile").is_some().then_some(name))
        })
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn profile_package(root: &Path, profile: &str) -> PathBuf {
    root.join("profiles").join(profile).join("package.json")
}

fn browser_enabled(
    executable: &Path,
    env: &BTreeMap<String, String>,
    profile: &str,
) -> Option<bool> {
    let text = match command::run(
        executable,
        &["--profile".into(), profile.into(), "--dump-config".into()],
        env,
    ) {
        Ok(text) => text,
        Err(error)
            if profile == "desktop"
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

fn installed_bundles(profile: &Value) -> Result<Vec<String>, AppError> {
    let bundles = profile
        .pointer("/dsh/profile/bundles")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::StorageError,
                "DSH profile has no dsh.profile.bundles array",
            )
        })?;
    Ok(bundles
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect())
}

fn package_name(mode: IntegrationMode) -> &'static str {
    match mode {
        IntegrationMode::SkillMcp => SKILL_MCP,
        IntegrationMode::Plugin => PLUGIN,
    }
}

fn bundle_dir(integrator: &Integrator, mode: IntegrationMode) -> PathBuf {
    record::integration_root(integrator.home())
        .join("dsh")
        .join(package_name(mode))
}

fn skill_mcp_manifest(integrator: &Integrator) -> Value {
    let _ = integrator;
    json!({
        "name": SKILL_MCP,
        "version": env!("CARGO_PKG_VERSION"),
        "private": true,
        "type": "module",
        "files": ["cordis.patch.yml"],
        "engines": {"dsh": ">=0.2.2-rc.1 <0.3.0"},
        "dsh": {"bundle": {"patch": "./cordis.patch.yml"}},
    })
}

fn plugin_manifest(integrator: &Integrator) -> Value {
    let _ = integrator;
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
            "./locale/*.json": "./locale/*.json",
            "./icon": "./icon.svg",
            "./skills/*": "./skills/*"
        },
        "files": [
            "index.js",
            "client.js",
            "skill.js",
            "cordis.patch.yml",
            "locale/*.json",
            "icon.svg",
            "skills/ossia/SKILL.md"
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

fn skill_mcp_patch(integrator: &Integrator) -> String {
    format!(
        "- insert:\n    - id: ossia-mcp\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: ossia\n        transport: stdio\n        command: {}\n        args: {}\n        env: {}\n        failOnStartupError: true\n",
        yaml_string(&integrator.executable().to_string_lossy()),
        yaml_array(&integrator.mcp_command()),
        yaml_env(&integrator.host_env()),
    )
}

fn plugin_patch(integrator: &Integrator, enable_browser: bool) -> String {
    let browser = if enable_browser {
        "- id: ui-sidebar-browser\n  name: '@deepseek-ai/dsh-client-ui-sidebar-browser'\n  disabled: false\n"
    } else {
        ""
    };
    format!(
        "{browser}- insert:\n    - id: ossia-mcp\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: ossia\n        transport: stdio\n        command: {}\n        args: {}\n        env: {}\n        failOnStartupError: true\n    - id: ossia-skill\n      name: 'ossia-dsh-plugin/skill'\n",
        yaml_string(&integrator.executable().to_string_lossy()),
        yaml_array(&integrator.mcp_command()),
        yaml_env(&integrator.host_env()),
    )
}

fn plugin_files(
    integrator: &Integrator,
    skill: &str,
    enable_browser: bool,
) -> Result<Vec<FilePlan>, AppError> {
    let root = bundle_dir(integrator, IntegrationMode::Plugin);
    let client = integrator.read_resource(Resource::DshClient)?;
    let icon = integrator.icon()?;
    let locale_en = integrator.read_resource(Resource::DshLocaleEn)?;
    let locale_zh = integrator.read_resource(Resource::DshLocaleZh)?;
    let skill_loader = format!(
        "export const inject = ['skills']\nexport function apply(ctx) {{\n  ctx.effect(() => ctx.skills.register({{ name: 'ossia', description: 'Work with a local Ossia score library through its MCP tools and public LilyPond files.', content: {} }}), 'ossia-skill')\n}}\n",
        serde_json::to_string(skill).expect("skill serialization")
    );
    Ok(vec![
        json_file(root.join("package.json"), plugin_manifest(integrator), "bundle-manifest"),
        FilePlan { path: root.join("cordis.patch.yml"), bytes: plugin_patch(integrator, enable_browser).into_bytes(), kind: "bundle-patch" },
        FilePlan { path: root.join("index.js"), bytes: b"/** Host half; MCP and Skill rows are the contribution. */\nexport function apply() {}\n".to_vec(), kind: "host-entry" },
        FilePlan { path: root.join("client.js"), bytes: client, kind: "client-entry" },
        FilePlan { path: root.join("skill.js"), bytes: skill_loader.into_bytes(), kind: "skill-loader" },
        FilePlan { path: root.join("skills/ossia/SKILL.md"), bytes: skill.as_bytes().to_vec(), kind: "skill" },
        FilePlan { path: root.join("locale/en.json"), bytes: locale_en, kind: "locale" },
        FilePlan { path: root.join("locale/zh.json"), bytes: locale_zh, kind: "locale" },
        FilePlan { path: root.join("icon.svg"), bytes: icon, kind: "asset" },
    ])
}

fn append_file_entries(
    previous: Option<&ManagedRecord>,
    files: &mut [FilePlan],
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
            .any(|entry| entry.path == path.to_string_lossy())
    })
}

fn read_json(path: &Path, label: &str) -> Result<Value, AppError> {
    let bytes = fs::read(path).map_err(|error| {
        AppError::new(
            ErrorCode::StorageError,
            format!("cannot read {label}: {error}"),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(ErrorCode::StorageError, format!("invalid {label}: {error}"))
    })
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

fn yaml_env(values: &BTreeMap<String, String>) -> String {
    if values.is_empty() {
        "{}".into()
    } else {
        format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("{}: {}", yaml_string(key), yaml_string(value)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn dependency_matches(value: &str, expected: &Path) -> bool {
    let path = value.strip_prefix("link:").unwrap_or(value);
    Path::new(path)
        .canonicalize()
        .ok()
        .zip(expected.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
        || Path::new(path) == expected
}
