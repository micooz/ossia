use ossia_runtime::{
    server, AppError, ErrorCode, ManagementPreferences, Result, TrayClickAction, UiLanguage,
    UiPreferences,
};
use tauri_plugin_autostart::ManagerExt;

/// Load preferences and seed the language from the operating system on first launch.
pub fn initialize(config: &server::Config) -> Result<UiPreferences> {
    let mut preferences = ossia_runtime::preferences::load(&config.home)?;
    if preferences.language.is_none() {
        preferences.language = Some(system_language());
        ossia_runtime::preferences::save(&config.home, &preferences)?;
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
        tray_click_action_editable: !cfg!(target_os = "linux"),
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
        config.home.to_string_lossy().into_owned(),
        "--port".into(),
        config.port.to_string(),
        "--engine-root".into(),
        config.engine_root.to_string_lossy().into_owned(),
        "--docs-root".into(),
        config.docs_root.to_string_lossy().into_owned(),
    ];
    if let Some(static_dir) = &config.static_dir {
        args.push("--static-dir".into());
        args.push(static_dir.to_string_lossy().into_owned());
    }
    if let Some(origin) = &config.dev_origin {
        args.push("--dev-origin".into());
        args.push(origin.clone());
    }
    args.push("--no-main-window".into());
    args
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
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn autostart_args_preserve_runtime_configuration() {
        let config = server::Config {
            home: PathBuf::from("/tmp/ossia home"),
            port: 3212,
            engine_root: PathBuf::from("/tmp/engine"),
            docs_root: PathBuf::from("/tmp/docs"),
            static_dir: Some(PathBuf::from("/tmp/ui")),
            dev_origin: Some("http://127.0.0.1:5173".into()),
        };
        assert_eq!(
            autostart_args(&config),
            [
                "--home",
                "/tmp/ossia home",
                "--port",
                "3212",
                "--engine-root",
                "/tmp/engine",
                "--docs-root",
                "/tmp/docs",
                "--static-dir",
                "/tmp/ui",
                "--dev-origin",
                "http://127.0.0.1:5173",
                "--no-main-window",
            ]
        );
    }

    #[test]
    fn language_parsing_uses_primary_subtag() {
        assert_eq!(
            UiLanguage::from_locale("zh-Hans-CN"),
            Some(UiLanguage::ZhCn)
        );
        assert_eq!(UiLanguage::from_locale("en_GB"), Some(UiLanguage::EnUs));
        assert_eq!(UiLanguage::from_locale("ja-JP"), None);
    }

    #[test]
    fn tray_action_values_round_trip() {
        for action in [TrayClickAction::Menu, TrayClickAction::Window] {
            assert_eq!(action_from_value(action_value(action)), action);
        }
    }
}
