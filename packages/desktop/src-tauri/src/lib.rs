#![cfg(feature = "gui")]
mod preferences;
mod tray;
mod window;

use ossia_runtime::{
    ipc, server, AgentsChange, AgentsMutation, AgentsSnapshot, AppError, DoctorReport, ErrorCode,
    ManagementPreferences, ManagementPreferencesChange, ManagementSnapshot, Result,
    TrayClickAction, UiLanguage, UiPreferences,
};
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
        Err(error) => (None, Some(error)),
    };
    ManagementSnapshot {
        home: config.home.to_string_lossy().into_owned(),
        engine_root: config.engine_root.to_string_lossy().into_owned(),
        runtime,
        error,
    }
}

fn integrator(config: &server::Config) -> Result<ossia_integrations::Integrator> {
    let executable = std::env::current_exe().map_err(storage)?;
    let mut paths = ossia_integrations::HostPaths::detected();
    paths.launch_env.insert(
        "OSSIA_DOCS_ROOT".into(),
        config.docs_root.to_string_lossy().into_owned(),
    );
    paths.launch_env.insert(
        "OSSIA_ENGINE_ROOT".into(),
        config.engine_root.to_string_lossy().into_owned(),
    );
    if let Some(static_dir) = &config.static_dir {
        paths.launch_env.insert(
            "OSSIA_STATIC_DIR".into(),
            static_dir.to_string_lossy().into_owned(),
        );
    }
    ossia_integrations::Integrator::with_paths(&config.home, executable, paths)
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
            eprintln!("ossia: cannot start local runtime: {error}");
            let _ = app.emit("runtime-start-failed", error);
        }
    });
}

/// Stop the runtime before allowing the desktop process to exit.
pub(crate) fn request_quit(app: &tauri::AppHandle) {
    let state = app.state::<Management>();
    if !state.quit.request() {
        return;
    }
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
                Ok(Err(error)) => eprintln!("ossia: cannot stop local runtime: {error}"),
                Err(_) => eprintln!("ossia: local runtime did not stop within 20 seconds"),
            }
        }
        quit.approve();
        handle.exit(0);
    });
}

#[tauri::command]
async fn management_status(state: tauri::State<'_, Management>) -> Result<ManagementSnapshot> {
    Ok(snapshot(&config(&state)?).await)
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
async fn management_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, Management>,
) -> Result<ManagementPreferences> {
    let config = config(&state)?;
    let preferences = ossia_runtime::preferences::load(&config.home)?;
    preferences::snapshot(&app, &preferences)
}

#[tauri::command]
async fn management_preferences_change(
    app: tauri::AppHandle,
    state: tauri::State<'_, Management>,
    change: ManagementPreferencesChange,
) -> Result<ManagementPreferences> {
    let config = config(&state)?;
    if cfg!(target_os = "linux") && change.tray_click_action != TrayClickAction::Menu {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "changing the tray click action is unsupported on Linux",
        ));
    }
    let preferences_lock = state.preferences_lock.clone();
    let guard = preferences_lock
        .lock()
        .map_err(|_| storage("preferences lock is poisoned"))?;
    let previous = ossia_runtime::preferences::load(&config.home)?;
    let next = UiPreferences {
        theme: change.theme,
        language: change.language,
        tray_click_action: change.tray_click_action,
    };
    tray::apply(&app, preferences::language(&next), next.tray_click_action)?;
    window::set_language(&app, preferences::language(&next))?;
    if let Err(error) = ossia_runtime::preferences::save(&config.home, &next) {
        let _ = tray::apply(
            &app,
            preferences::language(&previous),
            previous.tray_click_action,
        );
        let _ = window::set_language(&app, preferences::language(&previous));
        return Err(error);
    }
    drop(guard);
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
fn management_autostart_set(app: tauri::AppHandle, enabled: bool) -> Result<bool> {
    preferences::set_autostart(&app, enabled)
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
        Err(_) => vec!["--no-main-window".into()],
    };
    let management = Management {
        config: config.clone(),
        runtime_lock: Arc::new(tokio::sync::Mutex::new(())),
        preferences_lock: Arc::new(Mutex::new(())),
        quit: QuitState::new(),
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
            window::create(app.handle(), show_main_window, language)?;
            tray::install(app.handle(), language, preferences.tray_click_action)?;
            start_runtime(app.handle().clone(), config.clone(), management.clone());

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
            management_start,
            management_stop,
            management_doctor,
            management_agents_state,
            management_agents_change,
            management_preferences,
            management_preferences_change,
            management_autostart_set
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
