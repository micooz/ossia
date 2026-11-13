use crate::*;
use clap::{Parser, Subcommand};
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ossia", version, about = "Local score runtime, CLI and MCP")]
pub struct Args {
    #[arg(long, global = true)]
    pub home: Option<PathBuf>,
    #[arg(long, global = true, default_value_t = 3211)]
    pub port: u16,
    #[arg(long, global = true)]
    pub engine_root: Option<PathBuf>,
    #[arg(long, global = true)]
    pub static_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub docs_root: Option<PathBuf>,
    #[arg(long, global = true)]
    pub dev_origin: Option<String>,
    #[arg(long, global = true, hide = true)]
    pub no_main_window: bool,
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
    Guide,
    List {
        #[arg(default_value = "{}")]
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
    Layout {
        json: String,
    },
    Evidence {
        json: String,
    },
    Docs {
        json: String,
    },
    Import {
        json: String,
    },
    Open {
        #[arg(default_value = "{}")]
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
        })
    }
}

fn resources_dir(executable: &std::path::Path) -> Result<PathBuf> {
    let parent = executable
        .parent()
        .ok_or_else(|| invalid("executable has no parent"))?;
    if parent.file_name().is_some_and(|name| name == "MacOS")
        && parent
            .parent()
            .and_then(std::path::Path::file_name)
            .is_some_and(|name| name == "Contents")
    {
        Ok(parent.parent().unwrap().join("Resources"))
    } else {
        Ok(parent.join("resources"))
    }
}
/// Returns None for the GUI branch; headless modes never initialize Tauri.
pub fn arguments() -> Args {
    Args::parse()
}

pub fn entry(args: Args) -> Option<i32> {
    #[cfg(windows)]
    if std::env::args_os().len() > 1 {
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
            eprintln!("{e}");
            return Some(1);
        }
    };
    let result = runtime.block_on(run(args));
    match result {
        Ok(Some(data)) => {
            println!("{}", data);
            Some(0)
        }
        Ok(None) => Some(0),
        Err(error) => {
            if mcp_mode {
                eprintln!("{}", json!({"error":error}));
            } else {
                println!("{}", json!({"error":error}));
            }
            Some(1)
        }
    }
}
pub async fn run(args: Args) -> Result<Option<serde_json::Value>> {
    let config = args.config()?;
    let command = args.command.ok_or_else(|| invalid("no CLI command"))?;
    match command {
        Command::Serve => {
            server::serve(config).await?;
            Ok(None)
        }
        Command::Guide => Ok(Some(json!(GUIDE))),
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
                Command::WriteSource { json } => ("write_source", json),
                Command::ReadSource { json } => ("read_source", json),
                Command::Layout { json } => ("layout", json),
                Command::Evidence { json } => ("evidence", json),
                Command::Docs { json } => ("docs", json),
                Command::Import { json } => ("import", json),
                Command::Open { json } => ("open", json),
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
            let params = serde_json::from_str(&args).map_err(|e| invalid(e.to_string()))?;
            Ok(Some(ipc::call(&discovery.endpoint, method, params).await?))
        }
    }
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
    let integrator = ossia_integrations::Integrator::with_paths(&config.home, executable, paths)?;
    let value = match action {
        IntegrateAction::Detect | IntegrateAction::Status => {
            serde_json::to_value(integrator.agents()?).map_err(storage)?
        }
        IntegrateAction::Install { agent } => {
            serde_json::to_value(integrator.set_agents(ossia_types::AgentsChange {
                agent_ids: vec![agent],
                enabled: true,
            })?)
            .map_err(storage)?
        }
        IntegrateAction::Remove { agent } => {
            serde_json::to_value(integrator.set_agents(ossia_types::AgentsChange {
                agent_ids: vec![agent],
                enabled: false,
            })?)
            .map_err(storage)?
        }
        IntegrateAction::Sync => {
            serde_json::to_value(integrator.sync_managed()?).map_err(storage)?
        }
    };
    Ok(Some(value))
}
