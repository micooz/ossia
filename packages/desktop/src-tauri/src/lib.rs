#![cfg(feature = "gui")]
use ossia_runtime::{ipc, server, AppError, DoctorReport, ManagementSnapshot, Result};
use serde_json::json;

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

pub fn run(config: Result<server::Config>) {
    tauri::Builder::default()
        .manage(Management { config })
        .invoke_handler(tauri::generate_handler![
            management_status,
            management_start,
            management_stop,
            management_doctor
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ossia management window");
}
