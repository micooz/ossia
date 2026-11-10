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

#[derive(Debug, Clone)]
struct Host {
    executable: PathBuf,
    env: BTreeMap<String, String>,
}

pub fn detect(integrator: &Integrator) -> HostDetection {
    let paths = integrator.paths();
    let config_root = paths.dsh_home.to_string_lossy().into_owned();
    let skill_root = Some(paths.dsh_home.join("skills").to_string_lossy().into_owned());
    let profiles = profiles(&paths.dsh_home);
    let Some(executable) = command::find_program("dsh", paths.dsh_bin.as_deref()) else {
        return HostDetection {
            host: HostKind::Dsh,
            installed: false,
            version: None,
            executable: None,
            config_root,
            skill_root,
            profiles,
            capabilities: vec![],
            problem: Some(AppError::new(
                ErrorCode::NotFound,
                "DSH CLI was not found; set OSSIA_DSH_BIN or install DeepSeek Harness",
            )),
        };
    };
    let env = command::host_env("DSH_HOME", &paths.dsh_home);
    match command::version(&executable, &env) {
        Ok(version) => {
            let mut capabilities = BTreeSet::new();
            if command::run(
                &executable,
                &command::arguments(&["plugin", "--help"]),
                &env,
            )
            .is_ok()
            {
                capabilities.insert("plugin");
            }
            for profile in &profiles {
                if browser_enabled(&executable, &env, profile).is_some() {
                    capabilities.insert("sidebar-browser");
                }
            }
            HostDetection {
                host: HostKind::Dsh,
                installed: true,
                version: Some(version),
                executable: Some(executable.to_string_lossy().into_owned()),
                config_root,
                skill_root,
                profiles,
                capabilities: capabilities.into_iter().map(str::to_string).collect(),
                problem: None,
            }
        }
        Err(problem) => HostDetection {
            host: HostKind::Dsh,
            installed: false,
            version: None,
            executable: Some(executable.to_string_lossy().into_owned()),
            config_root,
            skill_root,
            profiles,
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

pub fn apply(
    integrator: &Integrator,
    plan: &IntegrationPlan,
) -> Result<Vec<ManagedEntry>, AppError> {
    let detection = detect(integrator);
    let Some(host) = host(integrator, &detection) else {
        return Err(AppError::new(ErrorCode::NotFound, "DSH CLI disappeared"));
    };
    let profile = &plan.profile;
    let package_name = package_name(plan.mode);
    let skill = integrator.skill()?;
    let browser = browser_enabled(&host.executable, &host.env, profile);
    let files = match plan.mode {
        IntegrationMode::SkillMcp => vec![
            FilePlan {
                path: integrator.paths().dsh_home.join("skills/ossia/SKILL.md"),
                bytes: skill.clone().into_bytes(),
                kind: "skill",
            },
            json_file(
                bundle_dir(integrator, plan.mode).join("package.json"),
                skill_mcp_manifest(integrator),
                "bundle-manifest",
            ),
            FilePlan {
                path: bundle_dir(integrator, plan.mode).join("cordis.patch.yml"),
                bytes: skill_mcp_patch(integrator).into_bytes(),
                kind: "bundle-patch",
            },
        ],
        IntegrationMode::Plugin => plugin_files(integrator, &skill, browser == Some(false))?,
    };
    for file in &files {
        record::atomic_write(&file.path, &file.bytes)?;
    }
    let profile_value = read_json(
        &profile_package(&integrator.paths().dsh_home, profile),
        "DSH profile manifest",
    )?;
    if !installed_bundles(&profile_value)?
        .iter()
        .any(|bundle| bundle == package_name)
    {
        dsh(
            &host,
            &[
                "plugin".into(),
                "--profile".into(),
                profile.clone(),
                "add".into(),
                bundle_dir(integrator, plan.mode)
                    .to_string_lossy()
                    .into_owned(),
            ],
        )?;
    }
    Ok(records_from_plan(plan, &files))
}

pub fn status(
    integrator: &Integrator,
    profile: &str,
    mode: Option<IntegrationMode>,
    record: Option<&ManagedRecord>,
) -> Result<HostStatus, AppError> {
    let detection = detect(integrator);
    let Some(host) = host(integrator, &detection) else {
        return Ok((
            IntegrationState::Unavailable,
            vec![],
            vec![],
            Some("DSH CLI is unavailable".into()),
        ));
    };
    let mut observations = Vec::new();
    let mut conflicts = Vec::new();
    if let (Some(requested), Some(record)) = (mode, record) {
        if record.mode != requested {
            conflicts.push(conflict(
                record.management_id.clone(),
                "mode",
                "the managed DSH integration uses another mode",
            ));
        }
    }
    let effective_mode = mode.or(record.map(|value| value.mode));
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
    let Some(effective_mode) = effective_mode else {
        return Ok((
            IntegrationState::Available,
            observations,
            conflicts,
            Some("DSH is detected; choose skill-mcp or plugin".into()),
        ));
    };
    if record.is_none() {
        let package_name = package_name(effective_mode);
        let profile_path = profile_package(&integrator.paths().dsh_home, profile);
        let unmanaged = profile_path.exists()
            && read_json(&profile_path, "DSH profile manifest")
                .ok()
                .and_then(|value| installed_bundles(&value).ok())
                .is_some_and(|bundles| bundles.iter().any(|bundle| bundle == package_name));
        if !unmanaged {
            if let Some(browser) = browser_enabled(&host.executable, &host.env, profile) {
                observations.push(observation(
                    "dsh:ui-sidebar-browser",
                    "capability",
                    if browser {
                        IntegrationState::Installed
                    } else {
                        IntegrationState::PendingHost
                    },
                    if browser {
                        None
                    } else {
                        Some("Sidebar Browser is disabled in this profile".into())
                    },
                ));
            }
            return Ok((
                IntegrationState::Available,
                observations,
                conflicts,
                Some("no managed integration is installed".into()),
            ));
        }
    }
    let package_name = package_name(effective_mode);
    let profile_path = profile_package(&integrator.paths().dsh_home, profile);
    if !profile_path.exists() {
        conflicts.push(conflict(profile, "profile", "DSH profile does not exist"));
    } else {
        match read_json(&profile_path, "DSH profile manifest") {
            Ok(profile_value) => match installed_bundles(&profile_value) {
                Ok(bundles) if bundles.iter().any(|bundle| bundle == package_name) => {
                    let dependency = profile_value
                        .get("dependencies")
                        .and_then(Value::as_object)
                        .and_then(|items| items.get(package_name))
                        .and_then(Value::as_str);
                    if dependency.is_some_and(|value| {
                        !dependency_matches(value, &bundle_dir(integrator, effective_mode))
                    }) {
                        conflicts.push(conflict(
                            package_name,
                            "user-config",
                            "the managed DSH bundle link was changed",
                        ));
                    }
                    observations.push(observation(
                        format!("dsh:bundle:{package_name}"),
                        "host-entry",
                        IntegrationState::Installed,
                        None,
                    ));
                }
                Ok(_) => {
                    observations.push(observation(
                        format!("dsh:bundle:{package_name}"),
                        "host-entry",
                        IntegrationState::Broken,
                        None,
                    ));
                    conflicts.push(conflict(
                        format!("dsh:bundle:{package_name}"),
                        "missing",
                        "the managed bundle is absent from the DSH profile",
                    ));
                }
                Err(error) => {
                    conflicts.push(conflict(profile, "profile", error.message));
                }
            },
            Err(error) => conflicts.push(conflict(profile, "profile", error.message)),
        }
    }
    if let Some(browser) = browser_enabled(&host.executable, &host.env, profile) {
        observations.push(observation(
            "dsh:ui-sidebar-browser",
            "capability",
            if browser {
                IntegrationState::Installed
            } else {
                IntegrationState::PendingHost
            },
            if browser {
                None
            } else {
                Some("Sidebar Browser is disabled in this profile".into())
            },
        ));
    }
    if record.is_none() {
        conflicts.push(conflict(
            format!("dsh:bundle:{package_name}"),
            "unmanaged",
            "the bundle exists without an ownership record",
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
    Ok((
        state,
        observations,
        conflicts,
        if state == IntegrationState::PendingHost {
            Some("enable the DSH Browser in the selected profile".into())
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
    profile: &str,
    mode: IntegrationMode,
    entries: &[ManagedEntry],
) -> Result<(), AppError> {
    verify_owned_files(entries)?;
    let detection = detect(integrator);
    if entries.iter().any(|entry| entry.kind == "host-entry") && detection.executable.is_none() {
        return Err(AppError::new(
            ErrorCode::RuntimeUnavailable,
            "DSH CLI is unavailable; the managed files were left unchanged",
        ));
    }
    if let Some(host) = host(integrator, &detection) {
        let package_name = package_name(mode);
        if entries
            .iter()
            .any(|entry| entry.path == format!("dsh:bundle:{package_name}"))
        {
            let profile_path = profile_package(&integrator.paths().dsh_home, profile);
            let installed = profile_path.exists()
                && read_json(&profile_path, "DSH profile manifest")
                    .ok()
                    .is_some_and(|value| {
                        installed_bundles(&value).is_ok_and(|bundles| {
                            bundles.iter().any(|bundle| bundle == package_name)
                        })
                    });
            if installed {
                dsh(
                    &host,
                    &[
                        "plugin".into(),
                        "--profile".into(),
                        profile.into(),
                        "remove".into(),
                        package_name.into(),
                    ],
                )?;
            }
        }
    }
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        if let Some(hash) = entry.sha256.as_ref() {
            delete_owned(Path::new(&entry.path), hash)?;
        }
    }
    let _ = fs::remove_dir_all(bundle_dir(integrator, mode));
    let _ = fs::remove_dir(integrator.paths().dsh_home.join("skills/ossia"));
    Ok(())
}

#[derive(Debug, Clone)]
struct FilePlan {
    path: PathBuf,
    bytes: Vec<u8>,
    kind: &'static str,
}

fn host(integrator: &Integrator, detection: &HostDetection) -> Option<Host> {
    detection.executable.as_ref().map(|executable| Host {
        executable: PathBuf::from(executable),
        env: command::host_env("DSH_HOME", &integrator.paths().dsh_home),
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
        "version": "0.1.0",
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
        "version": "0.1.0",
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
    let client = fs::read(integrator.resources().join("dsh/client.js")).map_err(|error| {
        AppError::new(
            ErrorCode::UnsupportedSource,
            format!("integration resource dsh/client.js is unavailable: {error}"),
        )
    })?;
    let icon = integrator.icon()?;
    let locale_en = fs::read(integrator.resources().join("dsh/locale/en.json")).map_err(storage)?;
    let locale_zh = fs::read(integrator.resources().join("dsh/locale/zh.json")).map_err(storage)?;
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

fn json_file(path: PathBuf, value: Value, kind: &'static str) -> FilePlan {
    FilePlan {
        path,
        bytes: serde_json::to_vec_pretty(&value).expect("JSON serialization"),
        kind,
    }
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
    fn plugin_package_declares_both_halves_and_absolute_mcp_command() {
        let (_root, integrator) = fixture();
        let files = plugin_files(&integrator, &integrator.skill().unwrap(), true).unwrap();
        let package = files
            .iter()
            .find(|file| file.kind == "bundle-manifest")
            .unwrap();
        let package: Value = serde_json::from_slice(&package.bytes).unwrap();
        assert_eq!(package["name"], PLUGIN);
        assert_eq!(package["dsh"]["bundle"]["patch"], "./cordis.patch.yml");
        assert_eq!(package["dsh"]["client"]["platform"], "web");
        assert!(package["exports"].get("./client").is_some());
        assert_eq!(package["engines"]["dsh"], ">=0.2.2-rc.1 <0.3.0");

        let patch = files
            .iter()
            .find(|file| file.kind == "bundle-patch")
            .unwrap();
        let patch = String::from_utf8_lossy(&patch.bytes);
        assert!(patch.contains("ui-sidebar-browser"));
        assert!(patch.contains("@deepseek-ai/dsh-mcp-client"));
        assert!(patch.contains("transport: stdio"));
        assert!(patch.contains(&format!("\"{}\"", integrator.executable().display())));
        assert!(patch.contains("ossia-dsh-plugin/skill"));
    }

    #[test]
    fn dsh_skill_mcp_mode_uses_a_configuration_only_bundle() {
        let (_root, integrator) = fixture();
        let manifest = skill_mcp_manifest(&integrator);
        assert_eq!(manifest["name"], SKILL_MCP);
        assert!(manifest["dsh"]["client"].is_null());
        let patch = skill_mcp_patch(&integrator);
        assert!(patch.contains("@deepseek-ai/dsh-mcp-client"));
        assert!(!patch.contains("ui-sidebar-browser"));
    }
}
