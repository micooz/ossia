use super::*;

pub(super) fn install(
    ctx: &AgentContext,
    previous: Option<&ManagedRecord>,
) -> Result<Vec<ManagedEntry>, AppError> {
    let files = plugin_files(ctx)?;
    let mut conflicts = file_conflicts(&files, previous);
    let expected = bundle_dir(ctx);
    let base = profile_path(ctx)
        .parent()
        .unwrap_or(&expected)
        .to_path_buf();
    let profile = read_profile(ctx)?;
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
        if listed && previous.is_none() && expected.exists() {
            conflicts.push(conflict(
                PLUGIN,
                "unmanaged",
                "the DSH bundle exists without an ownership record",
            ));
        }
        (listed, linked)
    } else {
        (false, false)
    };
    if !conflicts.is_empty() {
        return Err(conflict_error(&conflicts));
    }
    let snapshot = snapshot_files(&files);
    if let Err(error) = write_files(&files) {
        let _ = restore_files(&snapshot);
        return Err(error);
    }
    if !listed || !linked {
        if let Err(error) = run(
            ctx,
            &[
                "plugin",
                "--profile",
                ctx.detected.profile,
                "add",
                &expected.to_string_lossy(),
            ],
        ) {
            let _ = restore_files(&snapshot);
            return Err(error);
        }
    }
    Ok(records_from_files(&files))
}

pub(super) fn sync(
    ctx: &AgentContext,
    _record: &ManagedRecord,
) -> Result<Option<Vec<ManagedEntry>>, AppError> {
    let Some(profile) = read_profile(ctx)? else {
        return Ok(None);
    };
    let expected = bundle_dir(ctx);
    let base = profile_path(ctx)
        .parent()
        .unwrap_or(&expected)
        .to_path_buf();
    if !installed_bundles(&profile)
        .iter()
        .any(|value| value == PLUGIN)
    {
        return Ok(None);
    }
    if !dependency(&profile).is_some_and(|value| dependency_matches(value, &expected, &base)) {
        return Ok(None);
    }
    let files = plugin_files(ctx)?;
    let snapshot = snapshot_files(&files);
    if let Err(error) = write_files(&files) {
        let _ = restore_files(&snapshot);
        return Err(error);
    }
    Ok(Some(records_from_files(&files)))
}

pub(super) fn remove(ctx: &AgentContext, record: &ManagedRecord) -> Result<(), AppError> {
    if let Some(profile) = read_profile(ctx)? {
        if installed_bundles(&profile)
            .iter()
            .any(|value| value == PLUGIN)
        {
            let expected = bundle_dir(ctx);
            let base = profile_path(ctx)
                .parent()
                .unwrap_or(&expected)
                .to_path_buf();
            if dependency(&profile)
                .is_some_and(|value| !dependency_matches(value, &expected, &base))
            {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "the managed DSH bundle link points to another location",
                ));
            }
            run(
                ctx,
                &[
                    "plugin",
                    "--profile",
                    ctx.detected.profile,
                    "remove",
                    PLUGIN,
                ],
            )?;
        }
    }
    delete_entries(&record.entries)?;
    cleanup_shared_dir(ctx.shared_root());
    Ok(())
}

pub(super) fn remove_legacy(ctx: &AgentContext, record: &LegacyRecord) -> Result<(), AppError> {
    for entry in record
        .entries
        .iter()
        .filter(|entry| entry.path.starts_with("dsh:bundle:"))
    {
        let package = entry.path.trim_start_matches("dsh:bundle:");
        if package.is_empty() {
            continue;
        }
        if let Some(profile) = read_profile(ctx)? {
            if installed_bundles(&profile)
                .iter()
                .any(|value| value == package)
            {
                run(
                    ctx,
                    &[
                        "plugin",
                        "--profile",
                        ctx.detected.profile,
                        "remove",
                        package,
                    ],
                )?;
            }
        }
    }
    delete_entries(&record.entries)?;
    let _ = fs::remove_dir(ctx.integrator.paths().dsh_home.join("skills/ossia"));
    cleanup_shared_dir(ctx.shared_root());
    Ok(())
}
