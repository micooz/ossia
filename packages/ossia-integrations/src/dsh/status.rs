use super::*;

pub(super) fn status(
    ctx: &AgentContext,
    record: Option<&ManagedRecord>,
) -> Result<AgentInspection, AppError> {
    let expected = bundle_dir(ctx);
    let base = profile_path(ctx)
        .parent()
        .unwrap_or(&expected)
        .to_path_buf();
    let profile = read_profile(ctx)?;
    let mut conflicts = Vec::new();
    let bundle_missing = !expected.exists();
    let (listed, linked) = if let Some(profile) = profile.as_ref() {
        let listed = installed_bundles(profile)
            .iter()
            .any(|value| value == PLUGIN);
        let linked =
            dependency(profile).is_some_and(|value| dependency_matches(value, &expected, &base));
        if let Some(value) = dependency(profile) {
            if !dependency_matches(value, &expected, &base) {
                conflicts.push(conflict(
                    PLUGIN,
                    "user-config",
                    "the managed DSH bundle link points to another location",
                ));
            }
        }
        (listed, linked)
    } else {
        (false, false)
    };
    let recoverable_missing = listed && linked && bundle_missing && record.is_none();
    if listed && record.is_none() && !recoverable_missing {
        conflicts.push(conflict(
            PLUGIN,
            "unmanaged",
            "the DSH bundle exists without an ownership record",
        ));
    }
    if let Some(record) = record {
        conflicts.extend(record_file_conflicts(&record.entries));
    }
    let missing_files = record.is_some_and(|record| record_files_missing(&record.entries));
    let installed = listed && linked && !recoverable_missing;
    let managed = record.is_some();
    let state = if !conflicts.is_empty() {
        AgentState::Conflict
    } else if missing_files || recoverable_missing {
        AgentState::Broken
    } else if installed {
        AgentState::Installed
    } else if managed {
        AgentState::Broken
    } else {
        AgentState::Available
    };
    let message = match state {
        AgentState::Broken => Some(AgentNotice::DshBundleMissing),
        AgentState::Conflict => Some(AgentNotice::DshUnmanagedBundle),
        _ => None,
    };
    Ok(AgentInspection {
        state,
        installed,
        enabled: true,
        managed,
        path: Some(expected.to_string_lossy().into_owned()),
        message,
        conflicts,
    })
}
