use super::*;

pub(super) fn install(
    ctx: &AgentContext,
    previous: Option<&ManagedRecord>,
) -> Result<Vec<ManagedEntry>, AppError> {
    let files = plugin_files(ctx)?;
    let mut conflicts = file_conflicts(&files, previous);
    let expected_marketplace = marketplace_root(ctx.shared_root());
    let expected_plugin = plugin_dir(ctx.shared_root());
    if let Some(marketplace) = marketplace_entry(ctx)? {
        if !marketplace
            .get("root")
            .and_then(Value::as_str)
            .is_some_and(|path| same_path(path, &expected_marketplace))
        {
            conflicts.push(conflict(
                "codex:marketplace:ossia-local",
                "user-config",
                "the local marketplace name belongs to another source",
            ));
        }
    }
    if let Some(plugin) = plugin_entry(ctx)? {
        if !plugin
            .get("source")
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .is_some_and(|path| same_path(path, &expected_plugin))
        {
            conflicts.push(conflict(
                PLUGIN_ID,
                "user-config",
                "the installed plugin source no longer matches the managed marketplace",
            ));
        } else if previous.is_none() {
            conflicts.push(conflict(
                PLUGIN_ID,
                "unmanaged",
                "the plugin exists without an ownership record",
            ));
        }
    }
    if !conflicts.is_empty() {
        return Err(conflict_error(&conflicts));
    }
    let snapshot = snapshot_files(&files);
    if let Err(error) = write_files(&files)
        .and_then(|()| ensure_marketplace(ctx))
        .and_then(|()| run(ctx, &["plugin", "add", PLUGIN_ID]).map(|_| ()))
    {
        let _ = restore_files(&snapshot);
        return Err(error);
    }
    Ok(records_from_files(&files))
}

pub(super) fn sync(
    ctx: &AgentContext,
    _record: &ManagedRecord,
) -> Result<Option<Vec<ManagedEntry>>, AppError> {
    let Some(plugin) = plugin_entry(ctx)? else {
        return Ok(None);
    };
    if !plugin
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let expected_marketplace = marketplace_root(ctx.shared_root());
    let expected_plugin = plugin_dir(ctx.shared_root());
    if !plugin
        .get("source")
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str)
        .is_some_and(|path| same_path(path, &expected_plugin))
    {
        return Ok(None);
    }
    if !marketplace_entry(ctx)?.is_some_and(|marketplace| {
        marketplace
            .get("root")
            .and_then(Value::as_str)
            .is_some_and(|path| same_path(path, &expected_marketplace))
    }) {
        return Ok(None);
    }
    let files = plugin_files(ctx)?;
    let snapshot = snapshot_files(&files);
    if let Err(error) =
        write_files(&files).and_then(|()| run(ctx, &["plugin", "add", PLUGIN_ID]).map(|_| ()))
    {
        let _ = restore_files(&snapshot);
        return Err(error);
    }
    Ok(Some(records_from_files(&files)))
}

pub(super) fn remove(ctx: &AgentContext, record: &ManagedRecord) -> Result<(), AppError> {
    if let Some(plugin) = plugin_entry(ctx)? {
        let expected_plugin = plugin_dir(ctx.shared_root());
        if !plugin
            .get("source")
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .is_some_and(|path| same_path(path, &expected_plugin))
        {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "the installed plugin source no longer matches the managed marketplace",
            ));
        }
        run(ctx, &["plugin", "remove", PLUGIN_ID])?;
    }
    if let Some(marketplace) = marketplace_entry(ctx)? {
        if marketplace
            .get("root")
            .and_then(Value::as_str)
            .is_some_and(|path| same_path(path, &marketplace_root(ctx.shared_root())))
        {
            run(ctx, &["plugin", "marketplace", "remove", MARKETPLACE])?;
        }
    }
    delete_entries(&record.entries)?;
    cleanup_shared_dir(ctx.shared_root());
    Ok(())
}

pub(super) fn remove_legacy(ctx: &AgentContext, record: &LegacyRecord) -> Result<(), AppError> {
    if record.mode == "plugin" {
        if plugin_entry(ctx)?.is_some() {
            run(ctx, &["plugin", "remove", PLUGIN_ID])?;
        }
        if marketplace_entry(ctx)?.is_some() {
            run(ctx, &["plugin", "marketplace", "remove", MARKETPLACE])?;
        }
    } else if mcp_entry_exists(ctx)? {
        run(ctx, &["mcp", "remove", "ossia"])?;
    }
    delete_entries(&record.entries)?;
    cleanup_shared_dir(ctx.shared_root());
    Ok(())
}

fn ensure_marketplace(ctx: &AgentContext) -> Result<(), AppError> {
    let expected = marketplace_root(ctx.shared_root());
    match marketplace_entry(ctx)? {
        Some(marketplace)
            if marketplace
                .get("root")
                .and_then(Value::as_str)
                .is_some_and(|path| same_path(path, &expected)) =>
        {
            Ok(())
        }
        Some(_) => Err(AppError::new(
            ErrorCode::Conflict,
            "the local marketplace name belongs to another source",
        )),
        None => run(
            ctx,
            &["plugin", "marketplace", "add", &expected.to_string_lossy()],
        )
        .map(|_| ()),
    }
}
