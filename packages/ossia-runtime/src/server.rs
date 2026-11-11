use crate::*;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Config {
    pub home: PathBuf,
    pub port: u16,
    pub engine_root: PathBuf,
    pub static_dir: Option<PathBuf>,
    pub dev_origin: Option<String>,
}

pub fn resolve_home(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let path = explicit
        .or_else(|| std::env::var_os("OSSIA_HOME").map(PathBuf::from))
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".ossia"));
    if !path.is_absolute() {
        return Err(invalid("home must be an absolute path"));
    }
    canonical_home(&path)
}
pub fn canonical_home(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path.canonicalize().map_err(storage);
    }
    let parent = path.parent().ok_or_else(|| invalid("invalid home"))?;
    Ok(canonical_home(parent)?.join(path.file_name().ok_or_else(|| invalid("invalid home"))?))
}
fn secure_run(home: &Path) -> Result<PathBuf> {
    let path = home.join("run");
    if path.exists() {
        let meta = std::fs::symlink_metadata(&path).map_err(storage)?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(storage("run must be a real directory"));
        }
    }
    std::fs::create_dir_all(&path).map_err(storage)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).map_err(storage)?;
    }
    Ok(path)
}

pub async fn serve(config: Config) -> Result<()> {
    if let Some(origin) = &config.dev_origin {
        let uri: axum::http::Uri = origin
            .parse()
            .map_err(|_| invalid("invalid development Origin"))?;
        if uri.scheme_str() != Some("http")
            || !matches!(uri.host(), Some("127.0.0.1" | "localhost"))
            || uri.port_u16().is_none()
            || uri.path() != "/"
            || uri.query().is_some()
            || origin.ends_with('/')
        {
            return Err(invalid("development Origin must be an explicit loopback HTTP origin without trailing slash"));
        }
    }
    let library = ossia_library::Library::open(&config.home)?;
    let ui_preferences = crate::preferences::load(library.home())?;
    let run = secure_run(library.home())?;
    let root = config.engine_root.clone();
    let scratch = run.join("scratch");
    let engine = tokio::task::spawn_blocking(move || {
        ossia_engine::Engine::verify(&root, &scratch, &ossia_engine::RunContext::default())
    })
    .await
    .map_err(storage)??;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, config.port))
        .await
        .map_err(|e| {
            AppError::new(
                ErrorCode::RuntimeUnavailable,
                format!("cannot bind requested loopback port {}: {e}", config.port),
            )
        })?;
    let url = format!("http://{}", listener.local_addr().map_err(storage)?);
    let runtime = Runtime::new(library, Arc::new(engine), url);
    runtime.initialize_ui_language(ui_preferences.language);
    watch_parent(runtime.clone());
    let endpoint = ipc::endpoint(runtime.library.home())?;
    let ipc_listener = ipc::listen(&endpoint)?;
    let discovery = ipc::Discovery {
        runtime: runtime.info.clone(),
        endpoint: endpoint.to_string_lossy().into_owned(),
    };
    let temp = run.join(format!("runtime-{}.tmp", runtime.info.instance_id));
    std::fs::write(&temp, serde_json::to_vec(&discovery).map_err(storage)?).map_err(storage)?;
    std::fs::File::open(&temp)
        .and_then(|f| f.sync_all())
        .map_err(storage)?;
    let discovery_path = run.join("runtime.json");
    #[cfg(windows)]
    if discovery_path.exists() {
        std::fs::remove_file(&discovery_path).map_err(storage)?;
    }
    std::fs::rename(&temp, &discovery_path).map_err(storage)?;
    eprintln!(
        "Ossia runtime {} at {}",
        runtime.info.instance_id, runtime.info.web_base_url
    );
    let scheduler = tokio::spawn(runtime.clone().schedule());
    let watch_runtime = runtime.clone();
    let watcher = tokio::spawn(async move {
        let result = watch_runtime.clone().watch().await;
        if let Err(error) = result {
            watch_runtime.emit("watch-error", None, None, json!({"error":error}));
        }
    });
    let ipc_runtime = runtime.clone();
    let ipc_task = tokio::spawn(async move { ipc::serve(ipc_runtime, ipc_listener).await });
    let web = crate::http::router(runtime.clone(), config.static_dir, config.dev_origin);
    let stop_runtime = runtime.clone();
    let http_result = axum::serve(listener, web)
        .with_graceful_shutdown(async move {
            let signal = shutdown_signal();
            tokio::pin!(signal);
            loop {
                if stop_runtime
                    .stopping
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    break;
                }
                tokio::select! {
                    _ = &mut signal => { let _ = stop_runtime.stop(); break; }
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        })
        .await;
    let _ = runtime.stop();
    let _ = scheduler.await;
    let _ = watcher.await;
    let ipc_result = ipc_task.await.map_err(storage)?;
    if ipc::read_discovery(runtime.library.home())
        .is_ok_and(|d| d.runtime.instance_id == runtime.info.instance_id)
    {
        let _ = std::fs::remove_file(discovery_path);
    }
    http_result.map_err(storage)?;
    ipc_result
}

pub async fn ensure_runtime(config: &Config, executable: &Path) -> Result<ipc::Discovery> {
    ensure_runtime_inner(config, executable, None).await
}

/// Start a runtime owned by the desktop process and stop it when that process disappears.
pub async fn ensure_runtime_owned(
    config: &Config,
    executable: &Path,
    parent_pid: u32,
) -> Result<ipc::Discovery> {
    ensure_runtime_inner(config, executable, Some(parent_pid)).await
}

async fn ensure_runtime_inner(
    config: &Config,
    executable: &Path,
    parent_pid: Option<u32>,
) -> Result<ipc::Discovery> {
    match ipc::connect(&config.home).await {
        Ok(info) => {
            watch_existing_parent(&info, parent_pid).await?;
            return Ok(info);
        }
        Err(error) if error.code == ErrorCode::RuntimeIncompatible => return Err(error),
        _ => {}
    }
    let startup = tempfile::NamedTempFile::new().map_err(storage)?;
    let mut command = tokio::process::Command::new(executable);
    command
        .arg("--home")
        .arg(&config.home)
        .arg("--port")
        .arg(config.port.to_string())
        .arg("--engine-root")
        .arg(&config.engine_root);
    if let Some(dir) = &config.static_dir {
        command.arg("--static-dir").arg(dir);
    }
    if let Some(origin) = &config.dev_origin {
        command.arg("--dev-origin").arg(origin);
    }
    if let Some(pid) = parent_pid {
        command.env("OSSIA_PARENT_PID", pid.to_string());
    }
    command
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .stdout(startup.as_file().try_clone().map_err(storage)?)
        .stderr(startup.as_file().try_clone().map_err(storage)?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    command.creation_flags(0x00000008 | 0x00000200 | 0x08000000);
    let mut child = command.spawn().map_err(storage)?;
    let started = Instant::now();
    let mut exit = None;
    loop {
        match ipc::connect(&config.home).await {
            Ok(info) => {
                watch_existing_parent(&info, parent_pid).await?;
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
                return Ok(info);
            }
            Err(error) if error.code == ErrorCode::RuntimeIncompatible => return Err(error),
            _ => {}
        }
        if exit.is_none() {
            exit = child.try_wait().map_err(storage)?;
        }
        if exit.is_some() {
            // A losing concurrent launcher may exit while the lock winner is still starting.
            let log = std::fs::read_to_string(startup.path()).unwrap_or_default();
            if let Some(error) = log
                .lines()
                .filter_map(|line| {
                    serde_json::from_str::<RpcResponse>(line)
                        .ok()
                        .and_then(|r| r.error)
                })
                .next_back()
            {
                if error.code != ErrorCode::LibraryBusy {
                    return Err(error);
                }
            }
        }
        if started.elapsed() > Duration::from_secs(15) {
            return Err(AppError::new(ErrorCode::RuntimeUnavailable,format!("runtime failed to start or another process holds the library; child exit: {exit:?}")));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn watch_parent(runtime: Arc<Runtime>) {
    if let Some(pid) = std::env::var("OSSIA_PARENT_PID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
    {
        runtime.set_parent_pid(pid);
    }
    tokio::spawn(async move {
        while !runtime.stopping.load(std::sync::atomic::Ordering::Acquire) {
            if runtime.parent_pid().is_some_and(|pid| !process_alive(pid)) {
                let _ = runtime.stop();
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn watch_existing_parent(discovery: &ipc::Discovery, parent_pid: Option<u32>) -> Result<()> {
    if let Some(pid) = parent_pid {
        ipc::call(&discovery.endpoint, "watch-parent", json!({"pid": pid})).await?;
    }
    Ok(())
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    if pid == 0 || pid > libc::pid_t::MAX as u32 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        Storage::FileSystem::SYNCHRONIZE,
        System::Threading::{OpenProcess, WaitForSingleObject},
    };
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return false;
        }
        let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        alive
    }
}

#[cfg(not(any(unix, windows)))]
fn process_alive(_: u32) -> bool {
    true
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _=tokio::signal::ctrl_c()=>{},_=signal.recv()=>{} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

/// Inspects configuration and resources without starting a service or opening the library.
pub async fn doctor(config: &Config) -> Result<DoctorReport> {
    let root = config.engine_root.clone();
    let resource = tokio::task::spawn_blocking(move || {
        let scratch = tempfile::tempdir().map_err(storage)?;
        ossia_engine::Engine::verify(&root, scratch.path(), &ossia_engine::RunContext::default())
            .map(|engine| engine.build_id().to_string())
    })
    .await
    .map_err(storage)?;
    let discovery = ipc::connect(&config.home).await;
    Ok(DoctorReport {
        home: config.home.to_string_lossy().into_owned(),
        engine: match resource {
            Ok(id) => EngineHealth {
                ok: true,
                build_id: Some(id),
                error: None,
            },
            Err(error) => EngineHealth {
                ok: false,
                build_id: None,
                error: Some(error),
            },
        },
        runtime: match discovery {
            Ok(d) => RuntimeHealth {
                ok: true,
                info: Some(d.runtime),
                error: None,
            },
            Err(error) => RuntimeHealth {
                ok: false,
                info: None,
                error: Some(error),
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::process_alive;

    #[test]
    fn parent_liveness_distinguishes_current_and_impossible_pids() {
        assert!(process_alive(std::process::id()));
        assert!(!process_alive(u32::MAX));
    }
}
