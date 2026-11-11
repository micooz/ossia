use super::*;

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
