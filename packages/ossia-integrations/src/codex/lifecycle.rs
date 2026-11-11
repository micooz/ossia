use super::*;

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
