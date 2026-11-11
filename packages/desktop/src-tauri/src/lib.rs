#![cfg(feature = "gui")]
use ossia_runtime::{
    ipc, server, AgentsChange, AgentsMutation, AgentsSnapshot, AppError, DoctorReport,
    ManagementSnapshot, Result,
};
use serde_json::json;
use tauri::{Emitter, Manager};

struct Management {
    config: Result<server::Config>,
}

fn config(state: &tauri::State<'_, Management>) -> Result<server::Config> {
    state.config.clone()
}
async fn snapshot(config: &server::Config) -> ManagementSnapshot {
    let (runtime, error) = match ipc::connect(&config.home).await {
        Ok(d) => (Some(d.runtime), None),
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
    let executable = std::env::current_exe().map_err(|error| {
        AppError::new(ossia_runtime::ErrorCode::StorageError, error.to_string())
    })?;
    let mut paths = ossia_integrations::HostPaths::detected();
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

#[tauri::command]
async fn management_status(state: tauri::State<'_, Management>) -> Result<ManagementSnapshot> {
    Ok(snapshot(&config(&state)?).await)
}
#[tauri::command]
async fn management_start(state: tauri::State<'_, Management>) -> Result<ManagementSnapshot> {
    let config = config(&state)?;
    let executable = std::env::current_exe()
        .map_err(|e| AppError::new(ossia_runtime::ErrorCode::StorageError, e.to_string()))?;
    server::ensure_runtime(&config, &executable).await?;
    Ok(snapshot(&config).await)
}
#[tauri::command]
async fn management_stop(state: tauri::State<'_, Management>) -> Result<()> {
    let config = config(&state)?;
    let discovery = ipc::connect(&config.home).await?;
    ipc::call(&discovery.endpoint, "stop", json!({})).await?;
    Ok(())
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

pub fn run(config: Result<server::Config>) {
    tauri::Builder::default()
        .manage(Management { config })
        .setup(|app| {
            let config = app.state::<Management>().config.clone();
            let handle = app.handle().clone();
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
            management_start,
            management_stop,
            management_doctor,
            management_agents_state,
            management_agents_change
        ])
        .run(tauri::generate_context!())
        .expect("error while running ossia management window");
}
