#![cfg(feature = "gui")]
mod preferences;
mod tray;
mod update;
mod window;

use ossia_backend::{
    ipc, normalize_outbound_error, path_text, server, AgentsChange, AgentsMutation, AgentsSnapshot,
    AppError, DoctorReport, DocumentIdArgs, DocumentSummary, ErrorCode, ListArgs,
    ManagementPreferences, ManagementPreferencesChange, ManagementScorePage,
    ManagementScorePreview, ManagementScorePreviewArgs, ManagementSnapshot, OmrService,
    OmrSettingsChange, PurgedDocument, Result, StorageCleanupReport, StorageStatus,
    TrashedDocumentSummary, TrayClickAction, UiLanguage, UiPreferences,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};

const RUNNING: u8 = 0;
const STOPPING: u8 = 1;
const APPROVED: u8 = 2;

struct QuitState {
    phase: AtomicU8,
}

impl QuitState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            phase: AtomicU8::new(RUNNING),
        })
    }

    fn request(&self) -> bool {
        self.phase
            .compare_exchange(RUNNING, STOPPING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn stopping(&self) -> bool {
        self.phase.load(Ordering::Acquire) != RUNNING
    }

    fn approve(&self) {
        self.phase.store(APPROVED, Ordering::Release);
    }

    fn approved(&self) -> bool {
        self.phase.load(Ordering::Acquire) == APPROVED
    }
}

#[derive(Clone)]
struct Management {
    config: Result<server::Config>,
    runtime_lock: Arc<tokio::sync::Mutex<()>>,
    preferences_lock: Arc<Mutex<()>>,
    quit: Arc<QuitState>,
    update: Option<update::UpdateService>,
}

fn config(state: &tauri::State<'_, Management>) -> Result<server::Config> {
    state.config.clone()
}

fn storage(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::StorageError, error.to_string())
}

async fn snapshot(config: &server::Config) -> ManagementSnapshot {
    let (runtime, error) = match ipc::connect(&config.home).await {
        Ok(discovery) => (Some(discovery.runtime), None),
        // Discovery may not exist yet during startup, or after an intentional stop.
        Err(error) if error.code == ErrorCode::RuntimeUnavailable => (None, None),
        Err(mut error) => {
            normalize_outbound_error(&mut error);
            (None, Some(error))
        }
    };
    ManagementSnapshot {
        home: path_text(&config.home),
        engine_root: path_text(&config.engine_root),
        runtime,
        error,
    }
}

fn integrator(config: &server::Config) -> Result<ossia_integrations::Integrator> {
    let executable = std::env::current_exe().map_err(storage)?;
    let mut paths = ossia_integrations::HostPaths::detected();
    paths
        .launch_env
        .insert("OSSIA_DOCS_ROOT".into(), path_text(&config.docs_root));
    paths
        .launch_env
        .insert("OSSIA_ENGINE_ROOT".into(), path_text(&config.engine_root));
    if let Some(static_dir) = &config.static_dir {
        paths
            .launch_env
            .insert("OSSIA_STATIC_DIR".into(), path_text(static_dir));
    }
    ossia_integrations::Integrator::with_paths(&config.home, executable, paths, config.port)
}

async fn stop_runtime(config: &server::Config) -> Result<()> {
    let discovery = ipc::connect(&config.home).await?;
    ipc::call(&discovery.endpoint, "stop", json!({})).await?;
    Ok(())
}

async fn notify_runtime_language(
    config: &server::Config,
    language: Option<UiLanguage>,
) -> Result<()> {
    let discovery = ipc::connect(&config.home).await?;
    ipc::call(
        &discovery.endpoint,
        "ui-language",
        json!({"language": language}),
    )
    .await?;
    Ok(())
}

fn watch_language(app: tauri::AppHandle, config: server::Config, initial: UiLanguage) {
    tauri::async_runtime::spawn(async move {
        let mut applied = initial;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let Ok(current) = ossia_backend::preferences::load(&config.home) else {
                continue;
            };
            let language = preferences::language(&current);
            if applied == language {
                continue;
            }
            if tray::apply(&app, language, current.tray_click_action).is_err()
                || window::set_language(&app, language).is_err()
            {
                continue;
            }
            applied = language;
            let _ = app.emit("ui-language-changed", language);
        }
    });
}

fn start_runtime(app: tauri::AppHandle, config: Result<server::Config>, management: Management) {
    tauri::async_runtime::spawn(async move {
        let _guard = management.runtime_lock.lock().await;
        if management.quit.stopping() {
            return;
        }
        let Ok(config) = config else {
            return;
        };
        let executable = match std::env::current_exe() {
            Ok(executable) => executable,
            Err(error) => {
                eprintln!("ossia: cannot resolve executable: {error}");
                return;
            }
        };
        if let Err(error) =
            server::ensure_runtime_owned(&config, &executable, std::process::id()).await
        {
            eprintln!("ossia: cannot start local backend: {error}");
            let _ = app.emit("runtime-start-failed", error);
        }
    });
}

/// Stop the backend before allowing the desktop process to exit.
pub(crate) fn request_quit(app: &tauri::AppHandle) {
    let state = app.state::<Management>();
    if !state.quit.request() {
        return;
    }
    let management = state.inner().clone();
    let config = state.config.clone();
    let runtime_lock = state.runtime_lock.clone();
    let quit = state.quit.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = runtime_lock.lock().await;
        if let Ok(config) = config {
            match tokio::time::timeout(Duration::from_secs(20), stop_runtime(&config)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) if error.code == ErrorCode::RuntimeUnavailable => {}
                Ok(Err(error)) => eprintln!("ossia: cannot stop local backend: {error}"),
                Err(_) => eprintln!("ossia: local backend did not stop within 20 seconds"),
            }
        }
        update::runtime::finish_update_on_quit(&management);
        quit.approve();
        handle.exit(0);
    });
}

#[tauri::command]
async fn management_status(state: tauri::State<'_, Management>) -> Result<ManagementSnapshot> {
    Ok(snapshot(&config(&state)?).await)
}

#[tauri::command]
async fn management_open_service(state: tauri::State<'_, Management>) -> Result<()> {
    let runtime = snapshot(&config(&state)?).await.runtime.ok_or_else(|| {
        AppError::new(
            ErrorCode::RuntimeUnavailable,
            "local service is not running",
        )
    })?;
    tauri::async_runtime::spawn_blocking(move || open_external(&runtime.web_base_url))
        .await
        .map_err(storage)?
}

#[tauri::command]
async fn management_start(state: tauri::State<'_, Management>) -> Result<ManagementSnapshot> {
    let config = config(&state)?;
    let _guard = state.runtime_lock.lock().await;
    if state.quit.stopping() {
        return Err(AppError::new(
            ErrorCode::RuntimeUnavailable,
            "application is stopping",
        ));
    }
    let executable = std::env::current_exe().map_err(storage)?;
    server::ensure_runtime_owned(&config, &executable, std::process::id()).await?;
    Ok(snapshot(&config).await)
}

#[tauri::command]
async fn management_stop(state: tauri::State<'_, Management>) -> Result<()> {
    let config = config(&state)?;
    let _guard = state.runtime_lock.lock().await;
    stop_runtime(&config).await
}

#[tauri::command]
async fn management_open_data_directory(state: tauri::State<'_, Management>) -> Result<()> {
    let home = config(&state)?.home;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("/usr/bin/open");
        #[cfg(target_os = "windows")]
        let mut command = std::process::Command::new("explorer.exe");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let mut command = std::process::Command::new("xdg-open");
        let status = command.arg(home).status().map_err(storage)?;
        if !status.success() {
            return Err(storage("file manager could not open the data directory"));
        }
        Ok(())
    })
    .await
    .map_err(storage)?
}

#[tauri::command]
async fn management_doctor(state: tauri::State<'_, Management>) -> Result<DoctorReport> {
    server::doctor(&config(&state)?).await
}

#[tauri::command]
async fn management_agents_state(state: tauri::State<'_, Management>) -> Result<AgentsSnapshot> {
    integrator(&config(&state)?)?.agents()
}

#[tauri::command]
async fn management_agents_change(
    state: tauri::State<'_, Management>,
    change: AgentsChange,
) -> Result<AgentsMutation> {
    integrator(&config(&state)?)?.set_agents(change)
}

#[tauri::command]
async fn management_agents_forget(
    state: tauri::State<'_, Management>,
    agent_id: String,
) -> Result<AgentsSnapshot> {
    integrator(&config(&state)?)?.forget_agent(&agent_id)
}

#[tauri::command]
async fn management_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, Management>,
) -> Result<ManagementPreferences> {
    let config = config(&state)?;
    let preferences = ossia_backend::preferences::load(&config.home)?;
    preferences::snapshot(&app, &preferences)
}

#[tauri::command]
async fn management_preferences_change(
    app: tauri::AppHandle,
    state: tauri::State<'_, Management>,
    change: ManagementPreferencesChange,
) -> Result<ManagementPreferences> {
    let config = config(&state)?;
    if cfg!(target_os = "linux")
        && change
            .tray_click_action
            .is_some_and(|action| action != TrayClickAction::Menu)
    {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "changing the tray click action is unsupported on Linux",
        ));
    }
    let runtime_guard = state.runtime_lock.lock().await;
    let preferences_lock = state.preferences_lock.clone();
    let file_guard = ossia_backend::preferences::lock(&config.home)?;
    let previous = {
        let _guard = preferences_lock
            .lock()
            .map_err(|_| storage("preferences lock is poisoned"))?;
        ossia_backend::preferences::load(&config.home)?
    };
    if change
        .update_source
        .is_some_and(|source| previous.update_source != source)
    {
        if let Some(service) = &state.update {
            if service.is_in_flight() {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "cannot change the update source while an update is in progress",
                ));
            }
        }
    }
    let next = UiPreferences {
        theme: change.theme.unwrap_or(previous.theme),
        language: change.language.or(previous.language),
        tray_click_action: change
            .tray_click_action
            .unwrap_or(previous.tray_click_action),
        startup_action: change.startup_action.unwrap_or(previous.startup_action),
        storage_cache_limit: change
            .storage_cache_limit
            .unwrap_or(previous.storage_cache_limit),
        storage_auto_cleanup: change
            .storage_auto_cleanup
            .unwrap_or(previous.storage_auto_cleanup),
        lan_access: change.lan_access.unwrap_or(previous.lan_access),
        auto_download: change.auto_download.unwrap_or(previous.auto_download),
        update_channel: change.update_channel.unwrap_or(previous.update_channel),
        update_source: change.update_source.unwrap_or(previous.update_source),
        github_token: previous.github_token.clone(),
        omr_service: previous.omr_service,
        omr_enabled: previous.omr_enabled,
        omr_url: previous.omr_url.clone(),
        omr_token: previous.omr_token.clone(),
        omr_custom_token: previous.omr_custom_token.clone(),
    };
    let lan_changed = previous.lan_access != next.lan_access;
    let was_running = if lan_changed {
        ipc::connect(&config.home).await.is_ok()
    } else {
        false
    };
    tray::apply(&app, preferences::language(&next), next.tray_click_action)?;
    window::set_language(&app, preferences::language(&next))?;
    let save = {
        let _guard = preferences_lock
            .lock()
            .map_err(|_| storage("preferences lock is poisoned"))?;
        ossia_backend::preferences::save(&config.home, &next)
    };
    if let Err(error) = save {
        let _ = tray::apply(
            &app,
            preferences::language(&previous),
            previous.tray_click_action,
        );
        let _ = window::set_language(&app, preferences::language(&previous));
        return Err(error);
    }
    if lan_changed && was_running {
        let restart = async {
            match stop_runtime(&config).await {
                Ok(()) => {}
                Err(error) if error.code == ErrorCode::RuntimeUnavailable => {}
                Err(error) => return Err(error),
            }
            let executable = std::env::current_exe().map_err(storage)?;
            server::ensure_runtime_owned(&config, &executable, std::process::id()).await?;
            Ok(())
        }
        .await;
        if let Err(error) = restart {
            if let Ok(preferences_lock) = state.preferences_lock.lock() {
                let _ = ossia_backend::preferences::save(&config.home, &previous);
                drop(preferences_lock);
            }
            let _ = tray::apply(
                &app,
                preferences::language(&previous),
                previous.tray_click_action,
            );
            let _ = window::set_language(&app, preferences::language(&previous));
            if let Ok(executable) = std::env::current_exe() {
                let _ =
                    server::ensure_runtime_owned(&config, &executable, std::process::id()).await;
            }
            return Err(error);
        }
    }
    if previous.update_channel != next.update_channel
        || previous.update_source != next.update_source
    {
        if let Err(error) = ossia_backend::update_status::clear(&config.home) {
            eprintln!("ossia: cannot clear update status: {error}");
        }
    }
    if previous.update_channel != next.update_channel {
        if let Some(service) = &state.update {
            service.set_channel(next.update_channel);
        }
    }
    if previous.update_source != next.update_source {
        if let Some(service) = &state.update {
            service.set_source(next.update_source);
        }
    }
    drop(file_guard);
    drop(runtime_guard);
    if previous.language != next.language {
        if let Err(error) = notify_runtime_language(&config, next.language).await {
            if error.code != ErrorCode::RuntimeUnavailable {
                eprintln!("ossia: cannot update runtime language: {error}");
            }
        }
    }
    preferences::snapshot(&app, &next)
}

#[tauri::command]
async fn management_omr_settings_set(
    app: tauri::AppHandle,
    state: tauri::State<'_, Management>,
    change: OmrSettingsChange,
) -> Result<ManagementPreferences> {
    let config = config(&state)?;
    if change.clear_url && change.url.is_some() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "clearUrl and url are mutually exclusive",
        ));
    }
    if change.clear_token && change.token.is_some() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "clearToken and token are mutually exclusive",
        ));
    }
    let _file_guard = ossia_backend::preferences::lock(&config.home)?;
    let preferences_lock = state.preferences_lock.clone();
    let _guard = preferences_lock
        .lock()
        .map_err(|_| storage("preferences lock is poisoned"))?;
    let mut preferences = ossia_backend::preferences::load(&config.home)?;
    preferences.omr_enabled = Some(change.enabled.unwrap_or_else(|| preferences.omr_enabled()));
    preferences.omr_service = change.service;
    if change.service == OmrService::Custom {
        if change.clear_url {
            preferences.omr_url = None;
        } else if let Some(url) = change.url {
            preferences.omr_url = Some(ossia_backend::preferences::validate_omr_url(&url)?);
        }
    }
    let token_slot = match change.service {
        OmrService::Ossia => &mut preferences.omr_token,
        OmrService::Custom => &mut preferences.omr_custom_token,
    };
    if change.clear_token {
        *token_slot = None;
    } else if let Some(token) = change.token {
        *token_slot = Some(ossia_backend::preferences::normalize_omr_token(&token)?);
    }
    ossia_backend::preferences::save(&config.home, &preferences)?;
    preferences::snapshot(&app, &preferences)
}

#[tauri::command]
fn management_autostart_set(app: tauri::AppHandle, enabled: bool) -> Result<bool> {
    preferences::set_autostart(&app, enabled)
}

#[tauri::command]
async fn management_storage_status(state: tauri::State<'_, Management>) -> Result<StorageStatus> {
    let config = config(&state)?;
    let discovery = ipc::connect(&config.home).await?;
    let value = ipc::call(&discovery.endpoint, "storage-status", json!({})).await?;
    serde_json::from_value(value).map_err(storage)
}

#[tauri::command]
async fn management_storage_cleanup(
    state: tauri::State<'_, Management>,
) -> Result<StorageCleanupReport> {
    let config = config(&state)?;
    let discovery = ipc::connect(&config.home).await?;
    let value = ipc::call(&discovery.endpoint, "storage-cleanup", json!({})).await?;
    serde_json::from_value(value).map_err(storage)
}

async fn management_runtime_call<T: DeserializeOwned>(
    config: &server::Config,
    method: &str,
    params: serde_json::Value,
) -> Result<T> {
    let discovery = ipc::connect(&config.home).await?;
    let value = ipc::call(&discovery.endpoint, method, params).await?;
    serde_json::from_value(value).map_err(storage)
}

#[tauri::command]
async fn management_scores(
    state: tauri::State<'_, Management>,
    args: ListArgs,
) -> Result<ManagementScorePage> {
    management_runtime_call(&config(&state)?, "management-scores", json!(args)).await
}

#[tauri::command]
async fn management_archived_scores(
    state: tauri::State<'_, Management>,
    args: ListArgs,
) -> Result<ManagementScorePage> {
    management_runtime_call(&config(&state)?, "management-archived-scores", json!(args)).await
}

#[tauri::command]
async fn management_score_preview(
    state: tauri::State<'_, Management>,
    args: ManagementScorePreviewArgs,
) -> Result<ManagementScorePreview> {
    management_runtime_call(&config(&state)?, "management-score-preview", json!(args)).await
}

#[tauri::command]
async fn management_archive_score(
    state: tauri::State<'_, Management>,
    args: DocumentIdArgs,
) -> Result<TrashedDocumentSummary> {
    management_runtime_call(&config(&state)?, "management-archive-score", json!(args)).await
}

#[tauri::command]
async fn management_restore_score(
    state: tauri::State<'_, Management>,
    args: DocumentIdArgs,
) -> Result<DocumentSummary> {
    management_runtime_call(&config(&state)?, "untrash", json!(args)).await
}

#[tauri::command]
async fn management_delete_score(
    state: tauri::State<'_, Management>,
    args: DocumentIdArgs,
) -> Result<PurgedDocument> {
    management_runtime_call(&config(&state)?, "purge", json!(args)).await
}

#[tauri::command]
async fn management_open_score(
    state: tauri::State<'_, Management>,
    args: DocumentIdArgs,
) -> Result<()> {
    let url: String =
        management_runtime_call(&config(&state)?, "management-score-url", json!(args)).await?;
    tauri::async_runtime::spawn_blocking(move || open_external(&url))
        .await
        .map_err(storage)?
}

pub(crate) fn open_external(target: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("rundll32.exe");
    #[cfg(target_os = "windows")]
    command.arg("url.dll,FileProtocolHandler");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    let status = command.arg(target).status().map_err(storage)?;
    if !status.success() {
        return Err(storage("default browser could not open the link"));
    }
    Ok(())
}

pub fn run(config: Result<server::Config>, show_main_window: bool) {
    let config = config.and_then(|config| {
        ossia_integrations::launcher::publish(
            &config.home,
            &std::env::current_exe().map_err(storage)?,
        )?;
        Ok(config)
    });
    let initial_preferences = match &config {
        Ok(config) => preferences::initialize(config),
        Err(error) => Err(error.clone()),
    };
    let autostart_args = match &config {
        Ok(config) => preferences::autostart_args(config),
        Err(_) => Vec::new(),
    };
    let update = config
        .as_ref()
        .ok()
        .map(|config| update::UpdateService::new(&config.home));
    let management = Management {
        config: config.clone(),
        runtime_lock: Arc::new(tokio::sync::Mutex::new(())),
        preferences_lock: Arc::new(Mutex::new(())),
        quit: QuitState::new(),
        update,
    };
    let autostart = tauri_plugin_autostart::Builder::new()
        .app_name(if cfg!(debug_assertions) {
            "ossia-dev"
        } else {
            "ossia"
        })
        .args(autostart_args)
        .build();

    let builder = tauri::Builder::default()
        .plugin(autostart)
        .manage(management.clone());
    let app = builder
        .setup(move |app| {
            preferences::refresh_autostart(app.handle())?;
            let preferences = initial_preferences.clone()?;
            let language = preferences::language(&preferences);
            start_runtime(app.handle().clone(), config.clone(), management.clone());
            window::create(
                app.handle(),
                preferences::show_on_startup(preferences.startup_action, show_main_window),
                language,
            )?;
            tray::install(app.handle(), language, preferences.tray_click_action)?;
            if let Ok(config) = config.clone() {
                watch_language(app.handle().clone(), config, language);
            }
            update::runtime::start_update_schedule(app.handle().clone(), management.clone());

            let handle = app.handle().clone();
            let config = config.clone();
            std::thread::spawn(move || {
                if let Ok(config) = config {
                    if let Ok(integrator) = integrator(&config) {
                        let _ = integrator.sync_managed();
                    }
                }
                let _ = handle.emit("agents-updated", ());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            management_status,
            management_open_data_directory,
            management_open_service,
            management_start,
            management_stop,
            management_doctor,
            management_agents_state,
            management_agents_change,
            management_agents_forget,
            management_preferences,
            management_preferences_change,
            management_omr_settings_set,
            management_autostart_set,
            management_storage_status,
            management_storage_cleanup,
            management_scores,
            management_archived_scores,
            management_score_preview,
            management_archive_score,
            management_restore_score,
            management_delete_score,
            management_open_score,
            update::runtime::update_state,
            update::runtime::update_check,
            update::runtime::update_cancel,
            update::runtime::release_notes,
            update::runtime::update_apply,
            update::runtime::update_token_set,
            update::runtime::open_external_url
        ])
        .build(tauri::generate_context!())
        .expect("error while building ossia desktop");

    app.run(|handle, event| match event {
        tauri::RunEvent::ExitRequested { api, .. } => {
            let quit = handle.state::<Management>().quit.clone();
            if !quit.approved() {
                api.prevent_exit();
                request_quit(handle);
            }
        }
        tauri::RunEvent::Exit => {
            update::runtime::finish_update_on_quit(&handle.state::<Management>())
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            if let Err(error) = window::show(handle) {
                eprintln!("ossia: cannot reopen the management window: {error}");
            }
        }
        _ => {}
    });
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
