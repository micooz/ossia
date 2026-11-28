use crate::*;
use clap::{Parser, Subcommand};
use serde_json::json;
use std::{net::Ipv4Addr, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "ossia",
    version,
    about = "Local score backend, CLI and MCP",
    after_help = "Operation arguments are JSON; run `ossia schema <command>` for the MCP-compatible schema."
)]
pub struct Args {
    #[arg(long, global = true)]
    pub home: Option<PathBuf>,
    #[arg(long, global = true, default_value_t = DEFAULT_BACKEND_PORT)]
    pub port: u16,
    #[arg(long, global = true)]
    pub engine_root: Option<PathBuf>,
    #[arg(long, global = true)]
    pub static_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub docs_root: Option<PathBuf>,
    #[arg(long, global = true)]
    pub dev_origin: Option<String>,
    #[arg(long, global = true)]
    pub listen: Option<Ipv4Addr>,
    #[arg(long, global = true)]
    pub public_url: Option<String>,
    #[arg(long, global = true, hide = true)]
    pub no_main_window: bool,
    #[arg(long, global = true, hide = true)]
    pub startup_log: Option<PathBuf>,
    #[arg(long, global = true, hide = true)]
    pub parent_pid: Option<u32>,
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Subcommand)]
pub enum Command {
    Serve,
    Mcp,
    Doctor,
    Status,
    Stop,
    /// Print an Agent guide document and its materialized path.
    Guide {
        #[arg(long)]
        document: Option<String>,
    },
    /// Print the MCP-compatible JSON tool schema for one operation.
    Schema {
        command: String,
    },
    List {
        #[arg(default_value = "{}")]
        json: String,
    },
    UpdateMetadata {
        json: String,
    },
    Integrate {
        #[command(subcommand)]
        action: IntegrateAction,
    },
    /// Each operation accepts the same JSON object as its MCP tool.
    WriteSource {
        json: String,
    },
    ReadSource {
        json: String,
    },
    ReadDrawing {
        json: String,
    },
    WriteDrawing {
        json: String,
    },
    Layout {
        json: String,
    },
    Evidence {
        json: String,
    },
    Docs {
        json: String,
    },
    Convert {
        json: String,
    },
    Import {
        json: String,
    },
    Context {
        json: String,
    },
    Edit {
        json: String,
    },
    Check {
        json: String,
    },
    Commit {
        json: String,
    },
    History {
        json: String,
    },
    Delete {
        json: String,
    },
    Trash {
        #[arg(default_value = "{}")]
        json: String,
    },
    Untrash {
        json: String,
    },
    Purge {
        json: String,
    },
    Restore {
        json: String,
    },
    Output {
        json: String,
    },
    Job {
        json: String,
    },
}

#[derive(Subcommand)]
pub enum IntegrateAction {
    Detect,
    Status,
    Install { agent: String },
    Remove { agent: String },
    Sync,
}
impl Args {
    pub fn config(&self) -> Result<server::Config> {
        let executable = std::env::current_exe().map_err(storage)?;
        let executable = ossia_integrations::launcher::resource_executable(&executable)?;
        let resources = resources_dir(&executable)?;
        let engine_root = self
            .engine_root
            .clone()
            .or_else(|| std::env::var_os("OSSIA_ENGINE_ROOT").map(PathBuf::from))
            .or_else(|| Some(resources.join("engine")))
            .ok_or_else(|| invalid("supply --engine-root with prepared resources"))?;
        if !engine_root.is_absolute() {
            return Err(invalid("engine root must be absolute"));
        }
        let static_dir = self
            .static_dir
            .clone()
            .or_else(|| Some(resources.join("ui")));
        let docs_root = self
            .docs_root
            .clone()
            .or_else(|| std::env::var_os("OSSIA_DOCS_ROOT").map(PathBuf::from))
            .unwrap_or_else(|| resources.join("docs/lilypond/2.26.0"));
        if !docs_root.is_absolute() {
            return Err(invalid("docs root must be absolute"));
        }
        Ok(server::Config {
            docs_root,
            home: server::resolve_home(self.home.clone())?,
            port: self.port,
            engine_root,
            static_dir,
            dev_origin: self.dev_origin.clone(),
            listen: self.listen,
            public_url: self.public_url.clone(),
        })
    }
}

#[derive(Clone, Copy)]
enum ResourceLayout {
    MacOs,
    Windows,
    Linux,
}

fn resource_layout() -> ResourceLayout {
    if cfg!(target_os = "macos") {
        ResourceLayout::MacOs
    } else if cfg!(target_os = "windows") {
        ResourceLayout::Windows
    } else {
        ResourceLayout::Linux
    }
}

fn resources_dir(executable: &std::path::Path) -> Result<PathBuf> {
    resources_dir_for(executable, resource_layout())
}

/// Return CLI flags needed to preserve non-default resource locations.
pub fn resource_override_args(
    config: &server::Config,
    executable: &std::path::Path,
) -> Result<Vec<String>> {
    let executable = ossia_integrations::launcher::resource_executable(executable)?;
    Ok(resource_override_args_for(
        config,
        &resources_dir(&executable)?,
    ))
}

fn resource_override_args_for(config: &server::Config, resources: &std::path::Path) -> Vec<String> {
    let mut args = Vec::new();
    for (flag, actual, default) in [
        (
            "--engine-root",
            Some(&config.engine_root),
            resources.join("engine"),
        ),
        (
            "--docs-root",
            Some(&config.docs_root),
            resources.join("docs/lilypond/2.26.0"),
        ),
        (
            "--static-dir",
            config.static_dir.as_ref(),
            resources.join("ui"),
        ),
    ] {
        if let Some(path) = actual.filter(|path| *path != &default) {
            args.extend([flag.to_string(), path_text(path)]);
        }
    }
    args
}

fn resources_dir_for(executable: &std::path::Path, layout: ResourceLayout) -> Result<PathBuf> {
    let parent = executable
        .parent()
        .ok_or_else(|| invalid("executable has no parent"))?;
    match layout {
        ResourceLayout::MacOs => {
            if parent.file_name().is_some_and(|name| name == "MacOS")
                && parent
                    .parent()
                    .and_then(std::path::Path::file_name)
                    .is_some_and(|name| name == "Contents")
            {
                Ok(parent.parent().unwrap().join("Resources"))
            } else {
                Ok(parent.to_path_buf())
            }
        }
        ResourceLayout::Windows => Ok(parent.to_path_buf()),
        ResourceLayout::Linux => {
            if parent.join("engine/resources.json").is_file() {
                return Ok(parent.to_path_buf());
            }
            let stem = executable
                .file_stem()
                .ok_or_else(|| invalid("executable has no file name"))?;
            let lib = parent
                .parent()
                .ok_or_else(|| invalid("executable has no installation root"))?;
            let lib = lib.join("lib");
            let candidate = lib.join(stem);
            if candidate.join("engine/resources.json").is_file() {
                return Ok(candidate);
            }
            let stem_text = stem.to_string_lossy();
            if let Ok(entries) = std::fs::read_dir(&lib) {
                for entry in entries.flatten() {
                    if entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case(&stem_text))
                        && entry.path().join("engine/resources.json").is_file()
                    {
                        return Ok(entry.path());
                    }
                }
            }
            Ok(candidate)
        }
    }
}
/// Returns None for the GUI branch; server, CLI and MCP modes never initialize Tauri.
pub fn arguments() -> Args {
    Args::parse()
}

pub fn entry(args: Args) -> Option<i32> {
    let startup_log = args.startup_log.clone();
    if let Some(path) = &startup_log {
        let _ = STARTUP_LOG.set(path.clone());
    }
    if let Some(pid) = args.parent_pid {
        PARENT_PID.store(pid, std::sync::atomic::Ordering::Release);
    }
    #[cfg(windows)]
    if startup_log.is_none() && std::env::args_os().len() > 1 {
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            System::Console::{
                AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
            },
        };
        // Keep inherited MCP pipes; attach a parent console only when no stdout handle exists.
        unsafe {
            let output = GetStdHandle(STD_OUTPUT_HANDLE);
            if output.is_null() || output == INVALID_HANDLE_VALUE {
                AttachConsole(ATTACH_PARENT_PROCESS);
            }
        }
    }
    args.command.as_ref()?;
    let mcp_mode = matches!(args.command, Some(Command::Mcp));
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            if let Some(path) = &startup_log {
                append_startup_line(
                    path,
                    &json!({"error":AppError::new(ErrorCode::RuntimeUnavailable,e.to_string())})
                        .to_string(),
                );
            } else {
                eprintln!("{e}");
            }
            return Some(1);
        }
    };
    let result = runtime.block_on(run(args));
    match result {
        Ok(Some(data)) => {
            if startup_log.is_none() {
                println!("{}", data);
            }
            Some(0)
        }
        Ok(None) => Some(0),
        Err(error) => {
            if let Some(path) = &startup_log {
                append_startup_line(path, &json!({"error":error}).to_string());
            } else if mcp_mode {
                eprintln!("{}", json!({"error":error}));
            } else {
                println!("{}", json!({"error":error}));
            }
            Some(1)
        }
    }
}
pub async fn run(args: Args) -> Result<Option<serde_json::Value>> {
    if let Some(Command::Schema { command }) = &args.command {
        return Ok(Some(tool_schema(command)?));
    }
    let config = args.config()?;
    let command = args.command.ok_or_else(|| invalid("no CLI command"))?;
    match command {
        Command::Serve => {
            server::serve(config).await?;
            Ok(None)
        }
        Command::Guide { document } => Ok(Some(value(crate::guide::document(
            &config.home,
            document.as_deref(),
        )?)?)),
        Command::Integrate { action } => run_integrate(&config, action).await,
        Command::Doctor => Ok(Some(value(server::doctor(&config).await?)?)),
        Command::Stop => {
            let discovery = ipc::connect(&config.home).await?;
            Ok(Some(
                ipc::call(&discovery.endpoint, "stop", json!({})).await?,
            ))
        }
        command => {
            let executable = std::env::current_exe().map_err(storage)?;
            let discovery = server::ensure_runtime(&config, &executable).await?;
            if matches!(command, Command::Mcp) {
                mcp::serve(discovery).await?;
                return Ok(None);
            }
            let (method, args) = match command {
                Command::Status => ("status", "{}".into()),
                Command::List { json } => ("list", json),
                Command::UpdateMetadata { json } => ("update_metadata", json),
                Command::WriteSource { json } => ("write_source", json),
                Command::ReadSource { json } => ("read_source", json),
                Command::ReadDrawing { json } => ("read_drawing", json),
                Command::WriteDrawing { json } => ("write_drawing", json),
                Command::Layout { json } => ("layout", json),
                Command::Evidence { json } => ("evidence", json),
                Command::Docs { json } => ("docs", json),
                Command::Convert { json } => ("convert", json),
                Command::Import { json } => ("import", json),
                Command::Context { json } => ("context", json),
                Command::Edit { json } => ("edit", json),
                Command::Check { json } => ("check", json),
                Command::Commit { json } => ("commit", json),
                Command::History { json } => ("history", json),
                Command::Delete { json } => ("delete", json),
                Command::Trash { json } => ("trash", json),
                Command::Untrash { json } => ("untrash", json),
                Command::Purge { json } => ("purge", json),
                Command::Restore { json } => ("restore", json),
                Command::Output { json } => ("output", json),
                Command::Job { json } => ("job", json),
                _ => unreachable!(),
            };
            let params: Value = serde_json::from_str(&args).map_err(|e| invalid(e.to_string()))?;
            let data = ipc::call(&discovery.endpoint, method, params.clone()).await?;
            Ok(Some(crate::agent_response::compact(method, &params, data)?))
        }
    }
}

fn tool_schema(command: &str) -> Result<serde_json::Value> {
    let tool_name = if command.starts_with("ossia_") {
        command.to_owned()
    } else {
        format!("ossia_{command}")
    };
    let tool = mcp::tools()
        .into_iter()
        .find(|tool| tool.name == tool_name)
        .ok_or_else(|| invalid(format!("unknown Ossia operation: {command}")))?;
    serde_json::to_value(tool).map_err(storage)
}

async fn run_integrate(
    config: &server::Config,
    action: IntegrateAction,
) -> Result<Option<serde_json::Value>> {
    let executable = std::env::current_exe().map_err(storage)?;
    if matches!(
        action,
        IntegrateAction::Install { .. } | IntegrateAction::Sync
    ) {
        ossia_integrations::launcher::publish(&config.home, &executable)?;
    }
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
    let integrator =
        ossia_integrations::Integrator::with_paths(&config.home, executable, paths, config.port)?;
    let value = match action {
        IntegrateAction::Detect | IntegrateAction::Status => {
            serde_json::to_value(integrator.agents()?).map_err(storage)?
        }
        IntegrateAction::Install { agent } => {
            serde_json::to_value(integrator.set_agents(ossia_types::AgentsChange {
                agent_ids: vec![agent],
                enabled: true,
                overwrite: false,
            })?)
            .map_err(storage)?
        }
        IntegrateAction::Remove { agent } => {
            serde_json::to_value(integrator.set_agents(ossia_types::AgentsChange {
                agent_ids: vec![agent],
                enabled: false,
                overwrite: false,
            })?)
            .map_err(storage)?
        }
        IntegrateAction::Sync => {
            serde_json::to_value(integrator.sync_managed()?).map_err(storage)?
        }
    };
    Ok(Some(value))
}

#[cfg(test)]
#[path = "../tests/unit/cli.rs"]
mod tests;
