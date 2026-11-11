use super::*;

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
