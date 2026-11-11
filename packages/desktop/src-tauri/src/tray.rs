use crate::{preferences, window};
use ossia_runtime::{Result, TrayClickAction, UiLanguage};
use std::sync::atomic::{AtomicU8, Ordering};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

const TRAY_ID: &str = "ossia";

struct TrayState {
    action: AtomicU8,
}

fn storage(error: impl std::fmt::Display) -> ossia_runtime::AppError {
    ossia_runtime::AppError::new(ossia_runtime::ErrorCode::StorageError, error.to_string())
}

fn labels(language: UiLanguage, version: &str) -> (String, String, String) {
    match language {
        UiLanguage::EnUs => (
            "Open Ossia".into(),
            format!("Version {version}"),
            "Quit Ossia".into(),
        ),
        UiLanguage::ZhCn => (
            "打开 Ossia".into(),
            format!("版本 {version}"),
            "退出 Ossia".into(),
        ),
    }
}

fn menu(app: &tauri::AppHandle, language: UiLanguage) -> tauri::Result<Menu<tauri::Wry>> {
    let version = app.package_info().version.to_string();
    let (open_label, version_label, quit_label) = labels(language, &version);
    let open = MenuItem::with_id(app, "open", open_label, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let version = MenuItem::with_id(app, "version", version_label, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &separator, &version, &quit])?;
    Ok(menu)
}

fn icon() -> tauri::Result<tauri::image::Image<'static>> {
    #[cfg(target_os = "macos")]
    let bytes = include_bytes!("../icons/tray-template.png");
    #[cfg(not(target_os = "macos"))]
    let bytes = include_bytes!("../icons/32x32.png");
    tauri::image::Image::from_bytes(bytes)
}

pub fn install(
    app: &tauri::AppHandle,
    language: UiLanguage,
    action: TrayClickAction,
) -> Result<()> {
    let action = action.effective();
    app.manage(TrayState {
        action: AtomicU8::new(preferences::action_value(action)),
    });
    let menu = menu(app, language).map_err(storage)?;
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon().map_err(storage)?)
        .tooltip("Ossia")
        .menu(&menu)
        .show_menu_on_left_click(action == TrayClickAction::Menu)
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            else {
                return;
            };
            let app = tray.app_handle();
            let action = preferences::action_from_value(
                app.state::<TrayState>().action.load(Ordering::Acquire),
            );
            if action == TrayClickAction::Window {
                if let Err(error) = window::show(app) {
                    eprintln!("ossia: cannot open the management window: {error}");
                }
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                if let Err(error) = window::show(app) {
                    eprintln!("ossia: cannot open the management window: {error}");
                }
            }
            "quit" => crate::request_quit(app),
            _ => {}
        });
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    builder.build(app).map_err(storage)?;
    Ok(())
}

pub fn apply(app: &tauri::AppHandle, language: UiLanguage, action: TrayClickAction) -> Result<()> {
    let action = action.effective();
    let menu = menu(app, language).map_err(storage)?;
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| storage("tray icon is unavailable"))?;
    tray.set_menu(Some(menu)).map_err(storage)?;
    tray.set_show_menu_on_left_click(action == TrayClickAction::Menu)
        .map_err(storage)?;
    app.state::<TrayState>()
        .action
        .store(preferences::action_value(action), Ordering::Release);
    Ok(())
}
