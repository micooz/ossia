//! Tauri commands and lifecycle wiring for the desktop updater.

use super::{
    schedule::{self, NextCheck},
    version, InstallOutcome, UpdateService,
};
use crate::{config, storage, window, Management};
use ossia_backend::{
    ipc, preferences as preference_store, server, AppError, ErrorCode, ManagementPreferences,
    Result, UiPreferences, UpdateChannel, UpdateSnapshot, UpdateSource, UpdateState,
};
use tauri::Emitter;

const UPDATE_EVENT: &str = "ossia://update";

fn service(state: &tauri::State<'_, Management>) -> Result<UpdateService> {
    state
        .update
        .clone()
        .ok_or_else(|| storage("update service is unavailable"))
}

fn load_token(preferences: &UiPreferences) -> Result<Option<String>> {
    preferences
        .github_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(preference_store::normalize_github_token)
        .transpose()
}

fn load_source_token(preferences: &UiPreferences, source: UpdateSource) -> Result<Option<String>> {
    if source == UpdateSource::Github {
        load_token(preferences)
    } else {
        Ok(None)
    }
}

fn channel(preferences: &UiPreferences) -> UpdateChannel {
    version::effective_channel(preferences.update_channel, version::build_stamp())
}

enum ScheduledCheck {
    Checked(UpdateSnapshot),
    Retry,
    Stop,
}

async fn scheduled_check(management: &Management, update: &UpdateService) -> ScheduledCheck {
    let config = match management.config.clone() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("ossia: cannot start automatic update checks: {error}");
            return ScheduledCheck::Stop;
        }
    };
    let preferences = match ossia_backend::preferences::load(&config.home) {
        Ok(preferences) => preferences,
        Err(error) => {
            eprintln!("ossia: cannot read preferences for the update check: {error}");
            return ScheduledCheck::Retry;
        }
    };
    let source = preferences.update_source;
    let token = match load_source_token(&preferences, source) {
        Ok(token) => token,
        Err(error) => {
            eprintln!("ossia: invalid GitHub token: {error}");
            return ScheduledCheck::Retry;
        }
    };
    ScheduledCheck::Checked(
        update
            .check(
                source,
                token.as_deref(),
                channel(&preferences),
                preferences.auto_download,
            )
            .await,
    )
}

pub(crate) fn start_update_schedule(app: tauri::AppHandle, management: Management) {
    let Some(update) = management.update.clone() else {
        return;
    };
    if let Ok(config) = &management.config {
        let saved = ossia_backend::update_status::read(&config.home);
        let preferences = ossia_backend::preferences::load(&config.home);
        if let (Ok(Some(saved)), Ok(preferences)) = (saved, preferences) {
            if saved.current != update.snapshot().current
                || saved.source != preferences.update_source
                || saved.channel != channel(&preferences)
            {
                if let Err(error) = ossia_backend::update_status::clear(&config.home) {
                    eprintln!("ossia: cannot clear update status: {error}");
                }
            }
        }
    }
    let handle = app.clone();
    update.announce(move |snapshot| {
        let _ = handle.emit_to(window::LABEL, UPDATE_EVENT, snapshot);
    });
    if update.is_debug_build() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(schedule::STARTUP_DELAY).await;
        let mut failures = 0u32;
        loop {
            if management.quit.stopping() {
                break;
            }
            let next = match scheduled_check(&management, &update).await {
                ScheduledCheck::Stop => break,
                ScheduledCheck::Retry => {
                    failures = failures.saturating_add(1);
                    NextCheck::After(schedule::retry_delay(failures))
                }
                ScheduledCheck::Checked(snapshot) => {
                    if snapshot.state == UpdateState::Error {
                        failures = failures.saturating_add(1);
                    } else if !snapshot.state.in_flight() {
                        failures = 0;
                    }
                    schedule::next_check(snapshot.state, snapshot.channel, failures)
                }
            };
            match next {
                NextCheck::Stop => break,
                NextCheck::After(delay) => tokio::time::sleep(delay).await,
            }
        }
    });
}

pub(crate) fn finish_update_on_quit(management: &Management) {
    let Some(service) = &management.update else {
        return;
    };
    service.finish_on_quit();
}

#[tauri::command]
pub(crate) fn update_state(state: tauri::State<'_, Management>) -> Result<UpdateSnapshot> {
    Ok(service(&state)?.snapshot())
}

#[tauri::command]
pub(crate) async fn update_cancel(state: tauri::State<'_, Management>) -> Result<UpdateSnapshot> {
    Ok(service(&state)?.cancel_download().await)
}

#[tauri::command]
pub(crate) async fn update_check(state: tauri::State<'_, Management>) -> Result<UpdateSnapshot> {
    let config = config(&state)?;
    let preferences = ossia_backend::preferences::load(&config.home)?;
    let source = preferences.update_source;
    let token = load_source_token(&preferences, source)?;
    Ok(service(&state)?
        .check(
            source,
            token.as_deref(),
            channel(&preferences),
            preferences.auto_download,
        )
        .await)
}

#[tauri::command]
pub(crate) async fn release_notes(state: tauri::State<'_, Management>) -> Result<UpdateSnapshot> {
    let config = config(&state)?;
    let preferences = ossia_backend::preferences::load(&config.home)?;
    let source = preferences.update_source;
    let token = load_source_token(&preferences, source)?;
    Ok(service(&state)?
        .check_notes(source, token.as_deref(), channel(&preferences))
        .await)
}

#[tauri::command]
pub(crate) async fn update_token_set(
    app: tauri::AppHandle,
    state: tauri::State<'_, Management>,
    token: Option<String>,
) -> Result<ManagementPreferences> {
    let config = config(&state)?;
    let token = token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(preference_store::normalize_github_token)
        .transpose()?;
    let _file_guard = preference_store::lock(&config.home)?;
    let preferences_lock = state.preferences_lock.clone();
    let _guard = preferences_lock
        .lock()
        .map_err(|_| storage("preferences lock is poisoned"))?;
    let mut preferences = ossia_backend::preferences::load(&config.home)?;
    preferences.github_token = token;
    ossia_backend::preferences::save(&config.home, &preferences)?;
    if let Some(service) = &state.update {
        service.clear_token_requirement();
    }
    crate::preferences::snapshot(&app, &preferences)
}

#[tauri::command]
pub(crate) async fn update_apply(
    app: tauri::AppHandle,
    state: tauri::State<'_, Management>,
) -> Result<UpdateSnapshot> {
    let config = config(&state)?;
    if state.quit.stopping() {
        return Err(AppError::new(
            ErrorCode::RuntimeUnavailable,
            "application is stopping",
        ));
    }
    let preferences = ossia_backend::preferences::load(&config.home)?;
    let source = preferences.update_source;
    let token = load_source_token(&preferences, source)?;
    let service = service(&state)?;
    let checked = service
        .check(source, token.as_deref(), channel(&preferences), true)
        .await;
    if !service.is_updatable() {
        return Ok(checked);
    }
    if checked.state != UpdateState::Ready || checked.error.is_some() {
        return Ok(checked);
    }

    let _guard = state.runtime_lock.lock().await;
    if state.quit.stopping() {
        return Err(AppError::new(
            ErrorCode::RuntimeUnavailable,
            "application is stopping",
        ));
    }
    let was_running = ipc::connect(&config.home).await.is_ok();
    if was_running {
        match crate::stop_runtime(&config).await {
            Ok(()) => {}
            Err(error) if error.code == ErrorCode::RuntimeUnavailable => {}
            Err(error) => return Err(error),
        }
    }
    let outcome = service.install().await;
    let quitting = state.quit.stopping();
    let restart = || {
        let config = config.clone();
        async move {
            if was_running && !quitting {
                let executable = std::env::current_exe().map_err(storage)?;
                let _ =
                    server::ensure_runtime_owned(&config, &executable, std::process::id()).await;
            }
            Ok::<_, AppError>(())
        }
    };
    match outcome {
        InstallOutcome::Completed(snapshot, installation) => {
            if let Err(error) = installation.relaunch(&config.home) {
                restart().await?;
                return Ok(UpdateSnapshot {
                    state: UpdateState::Error,
                    error: Some(format!(
                        "the update is in place but could not be started ({error}); quit and start Ossia again"
                    )),
                    ..snapshot
                });
            }
            state.quit.approve();
            app.exit(0);
            Ok(snapshot)
        }
        InstallOutcome::NotStarted(snapshot) | InstallOutcome::Failed(snapshot) => {
            restart().await?;
            Ok(snapshot)
        }
    }
}

#[tauri::command]
pub(crate) fn open_external_url(url: String) -> Result<()> {
    super::fetch::ensure_public_https(&url)
        .map_err(|error| AppError::new(ErrorCode::InvalidInput, error.to_string()))?;
    crate::open_external(&url)
}
