#![cfg(feature = "gui")]
use ossia_runtime::{
    ipc, server, AppError, DoctorReport, IntegrationAction, IntegrationDetection, IntegrationPlan,
    IntegrationRequest, IntegrationStatus, ManagementSnapshot, Result,
};
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

fn integration<T: serde::de::DeserializeOwned>(
    request: IntegrationRequest,
    config: &server::Config,
) -> Result<T> {
    serde_json::from_value(integrator(config)?.execute(request)?).map_err(|error| {
        AppError::new(
            ossia_runtime::ErrorCode::StorageError,
            format!("invalid integration response: {error}"),
        )
    })
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
async fn management_integration_detect(
    state: tauri::State<'_, Management>,
) -> Result<IntegrationDetection> {
    integrator(&config(&state)?)?.detection()
}

#[tauri::command]
async fn management_integration_plan(
    state: tauri::State<'_, Management>,
    mut request: IntegrationRequest,
) -> Result<IntegrationPlan> {
    let config = config(&state)?;
    request.action = IntegrationAction::Plan;
    integration(request, &config)
}

#[tauri::command]
async fn management_integration_status(
    state: tauri::State<'_, Management>,
    mut request: IntegrationRequest,
) -> Result<IntegrationStatus> {
    let config = config(&state)?;
    request.action = IntegrationAction::Status;
    integration(request, &config)
}

#[tauri::command]
async fn management_integration_apply(
    state: tauri::State<'_, Management>,
    mut request: IntegrationRequest,
) -> Result<IntegrationStatus> {
    let config = config(&state)?;
    request.action = IntegrationAction::Apply;
    integration(request, &config)
}

#[tauri::command]
async fn management_integration_repair(
    state: tauri::State<'_, Management>,
    mut request: IntegrationRequest,
) -> Result<IntegrationStatus> {
    let config = config(&state)?;
    request.action = IntegrationAction::Repair;
    integration(request, &config)
}

#[tauri::command]
async fn management_integration_remove(
    state: tauri::State<'_, Management>,
    mut request: IntegrationRequest,
) -> Result<IntegrationStatus> {
    let config = config(&state)?;
    request.action = IntegrationAction::Remove;
    integration(request, &config)
}

pub fn run(config: Result<server::Config>) {
    tauri::Builder::default()
        .manage(Management { config })
        .invoke_handler(tauri::generate_handler![
            management_status,
            management_start,
            management_stop,
            management_doctor,
            management_integration_detect,
            management_integration_plan,
            management_integration_status,
            management_integration_apply,
            management_integration_repair,
            management_integration_remove
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ossia management window");
}
