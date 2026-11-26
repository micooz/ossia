use ossia_backend::{
    path_text, server, AppError, ErrorCode, ManagementPreferences, Result, StartupAction,
    TrayClickAction, UiLanguage, UiPreferences, OSSIA_OMR_URL,
};
use tauri_plugin_autostart::ManagerExt;

/// Load preferences and seed the language from the operating system on first launch.
pub fn initialize(config: &server::Config) -> Result<UiPreferences> {
    let _guard = ossia_backend::preferences::lock(&config.home)?;
    let mut preferences = ossia_backend::preferences::load(&config.home)?;
    if preferences.language.is_none() {
        preferences.language = Some(system_language());
        ossia_backend::preferences::save(&config.home, &preferences)?;
    }
    Ok(preferences)
}

pub fn system_language() -> UiLanguage {
    sys_locale::get_locale()
        .as_deref()
        .and_then(UiLanguage::from_locale)
        .unwrap_or_default()
}

pub fn snapshot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    preferences: &UiPreferences,
) -> Result<ManagementPreferences> {
    let debug = cfg!(debug_assertions);
    let launch_at_login = if debug {
        false
    } else {
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::new(ErrorCode::StorageError, error.to_string()))?
    };
    Ok(ManagementPreferences {
        theme: preferences.theme,
        language: preferences.language,
        tray_click_action: preferences.tray_click_action.effective(),
        startup_action: preferences.startup_action,
        tray_click_action_editable: !cfg!(target_os = "linux"),
        storage_cache_limit: preferences.storage_cache_limit,
        storage_auto_cleanup: preferences.storage_auto_cleanup,
        lan_access: preferences.lan_access,
        auto_download: preferences.auto_download,
        update_channel: crate::update::version::effective_channel(
            preferences.update_channel,
            crate::update::version::build_stamp(),
        ),
        update_channel_preference: preferences.update_channel,
        update_source: preferences.update_source,
        github_token_masked: ossia_backend::preferences::masked_token(
            preferences.github_token.as_deref(),
        ),
        omr_service: preferences.omr_service,
        omr_ossia_url: OSSIA_OMR_URL.to_string(),
        omr_url: preferences.omr_url.clone(),
        omr_token_masked: ossia_backend::preferences::masked_token(
            preferences.omr_token.as_deref(),
        ),
        omr_custom_token_masked: ossia_backend::preferences::masked_token(
            preferences.omr_custom_token.as_deref(),
        ),
        launch_at_login,
        launch_at_login_editable: !debug,
        app_version: app.package_info().version.to_string(),
    })
}

/// Refresh release autostart entries and remove development registrations.
pub fn refresh_autostart<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<()> {
    let manager = app.autolaunch();
    if cfg!(debug_assertions) {
        return manager
            .disable()
            .map_err(|error| AppError::new(ErrorCode::StorageError, error.to_string()));
    }
    if manager
        .is_enabled()
        .map_err(|error| AppError::new(ErrorCode::StorageError, error.to_string()))?
    {
        manager
            .enable()
            .map_err(|error| AppError::new(ErrorCode::StorageError, error.to_string()))?;
    }
    Ok(())
}

pub fn set_autostart<R: tauri::Runtime>(app: &tauri::AppHandle<R>, enabled: bool) -> Result<bool> {
    if cfg!(debug_assertions) {
        return Ok(false);
    }
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|error| AppError::new(ErrorCode::StorageError, error.to_string()))?;
    } else {
        manager
            .disable()
            .map_err(|error| AppError::new(ErrorCode::StorageError, error.to_string()))?;
    }
    manager
        .is_enabled()
        .map_err(|error| AppError::new(ErrorCode::StorageError, error.to_string()))
}

/// Startup flags preserve the selected data root and bundled resources.
pub fn autostart_args(config: &server::Config) -> Vec<String> {
    let mut args = vec![
        "--home".into(),
        path_text(&config.home),
        "--port".into(),
        config.port.to_string(),
        "--engine-root".into(),
        path_text(&config.engine_root),
        "--docs-root".into(),
        path_text(&config.docs_root),
    ];
    if let Some(static_dir) = &config.static_dir {
        args.push("--static-dir".into());
        args.push(path_text(static_dir));
    }
    if let Some(origin) = &config.dev_origin {
        args.push("--dev-origin".into());
        args.push(origin.clone());
    }
    if let Some(listen) = config.listen {
        args.push("--listen".into());
        args.push(listen.to_string());
    }
    if let Some(public_url) = &config.public_url {
        args.push("--public-url".into());
        args.push(public_url.clone());
    }
    args
}

pub fn show_on_startup(action: StartupAction, show_main_window: bool) -> bool {
    show_main_window && action == StartupAction::Window
}

pub fn language(preferences: &UiPreferences) -> UiLanguage {
    preferences.language.unwrap_or_else(system_language)
}

pub fn action_value(action: TrayClickAction) -> u8 {
    match action {
        TrayClickAction::Menu => 0,
        TrayClickAction::Window => 1,
    }
}

pub fn action_from_value(value: u8) -> TrayClickAction {
    if value == 0 {
        TrayClickAction::Menu
    } else {
        TrayClickAction::Window
    }
}

#[cfg(test)]
#[path = "../tests/unit/preferences.rs"]
mod tests;
