use super::*;

pub(super) fn status(
    ctx: &AgentContext,
    record: Option<&ManagedRecord>,
) -> Result<AgentInspection, AppError> {
    let plugin = plugin_entry(ctx)?;
    let marketplace = marketplace_entry(ctx)?;
    let expected_plugin = plugin_dir(ctx.shared_root());
    let expected_marketplace = marketplace_root(ctx.shared_root());
    let mut conflicts = Vec::new();
    if let Some(marketplace) = marketplace.as_ref() {
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
    if let Some(plugin) = plugin.as_ref() {
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
        }
        if record.is_none() {
            conflicts.push(conflict(
                PLUGIN_ID,
                "unmanaged",
                "the plugin exists without an ownership record",
            ));
        }
    }
    if let Some(record) = record {
        conflicts.extend(record_file_conflicts(&record.entries));
    }
    let missing_files = record.is_some_and(|record| record_files_missing(&record.entries));
    let plugin_matches = plugin.as_ref().is_some_and(|plugin| {
        plugin
            .get("source")
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .is_some_and(|path| same_path(path, &expected_plugin))
    });
    let marketplace_matches = marketplace.as_ref().is_some_and(|marketplace| {
        marketplace
            .get("root")
            .and_then(Value::as_str)
            .is_some_and(|path| same_path(path, &expected_marketplace))
    });
    let installed = plugin_matches && marketplace_matches;
    let enabled = plugin
        .as_ref()
        .and_then(|value| value.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let managed = record.is_some();
    let state = if !conflicts.is_empty() {
        AgentState::Conflict
    } else if missing_files {
        AgentState::Broken
    } else if installed && enabled {
        AgentState::Installed
    } else if installed {
        AgentState::Disabled
    } else if managed {
        AgentState::Broken
    } else {
        AgentState::Available
    };
    let message = match state {
        AgentState::Disabled => Some("插件在 Codex 中已禁用；点击可覆盖并重新启用。".into()),
        AgentState::Broken => {
            Some("Codex 中缺少 Ossia 插件或 marketplace；点击可重新安装。".into())
        }
        AgentState::Conflict => Some("检测到不受 Ossia 管理的同名配置，需手动处理。".into()),
        _ => None,
    };
    Ok(AgentInspection {
        state,
        installed,
        enabled,
        managed,
        path: Some(expected_plugin.to_string_lossy().into_owned()),
        message,
        conflicts,
    })
}
