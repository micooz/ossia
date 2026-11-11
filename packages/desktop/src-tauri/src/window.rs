use ossia_runtime::{AppError, ErrorCode, Result, UiLanguage};
use tauri::{webview::WebviewWindowBuilder, Manager, WebviewUrl, WindowEvent};

pub const LABEL: &str = "management";

fn title(language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::EnUs => "Ossia",
        UiLanguage::ZhCn => "ossia 管理",
    }
}

fn storage(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::StorageError, error.to_string())
}

pub fn create(
    app: &tauri::AppHandle,
    visible: bool,
    language: UiLanguage,
) -> Result<tauri::WebviewWindow> {
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("main.html".into()))
        .title(title(language))
        .inner_size(1080.0, 720.0)
        .min_inner_size(860.0, 560.0)
        .center()
        .visible(visible)
        .focused(visible);
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(24.0, 26.0));
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        builder = builder.decorations(false);
    }
    let window = builder.build().map_err(storage)?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hide(&handle);
        }
    });
    #[cfg(target_os = "macos")]
    app.set_activation_policy(if visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    })
    .map_err(storage)?;
    Ok(window)
}

pub fn show(app: &tauri::AppHandle) -> Result<()> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "management window is missing"))?;
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Regular)
        .map_err(storage)?;
    window.show().map_err(storage)?;
    window.unminimize().map_err(storage)?;
    window.set_focus().map_err(storage)
}

pub fn hide(app: &tauri::AppHandle) -> Result<()> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "management window is missing"))?;
    window.hide().map_err(storage)?;
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory)
        .map_err(storage)?;
    Ok(())
}

pub fn set_language(app: &tauri::AppHandle, language: UiLanguage) -> Result<()> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "management window is missing"))?;
    window.set_title(title(language)).map_err(storage)
}
