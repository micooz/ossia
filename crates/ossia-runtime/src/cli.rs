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
    pub dev_origin: Option<String>,
    #[arg(long, global = true)]
    pub integration_root: Option<PathBuf>,
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
    List,
    Integrate {
        #[command(subcommand)]
        action: IntegrateAction,
    },
    /// Each operation accepts the same JSON object as its MCP tool.
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
    Restore {
        json: String,
    },
    Export {
        json: String,
    },
    Job {
        json: String,
    },
}

#[derive(Subcommand)]
pub enum IntegrateAction {
    Detect,
    Plan { json: String },
    Apply { json: String },
    Status { json: String },
    Repair { json: String },
    Remove { json: String },
}
impl Args {
    pub fn config(&self) -> Result<server::Config> {
        let executable = std::env::current_exe().map_err(storage)?;
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
        let integration_root = self
            .integration_root
            .clone()
            .or_else(|| std::env::var_os("OSSIA_INTEGRATION_ROOT").map(PathBuf::from))
            .unwrap_or_else(|| resources.join("integrations"));
        if !integration_root.is_absolute() {
            return Err(invalid("integration root must be absolute"));
        }
        let static_dir = self
            .static_dir
            .clone()
            .or_else(|| Some(resources.join("ui")));
        Ok(server::Config {
            home: server::resolve_home(self.home.clone())?,
            port: self.port,
            engine_root,
            static_dir,
            dev_origin: self.dev_origin.clone(),
            integration_root,
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
                Command::List => ("list", "{}".into()),
                Command::Import { json } => ("import", json),
                Command::Open { json } => ("open", json),
                Command::Context { json } => ("context", json),
                Command::Edit { json } => ("edit", json),
                Command::Check { json } => ("check", json),
                Command::Commit { json } => ("commit", json),
                Command::History { json } => ("history", json),
                Command::Restore { json } => ("restore", json),
                Command::Export { json } => ("export", json),
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
    let integrator = ossia_integrations::Integrator::with_paths(
        &config.home,
        executable,
        &config.integration_root,
        paths,
    )?;
    let (action_name, json) = match action {
        IntegrateAction::Detect => ("detect", "{}".to_string()),
        IntegrateAction::Plan { json } => ("plan", json),
        IntegrateAction::Apply { json } => ("apply", json),
        IntegrateAction::Status { json } => ("status", json),
        IntegrateAction::Repair { json } => ("repair", json),
        IntegrateAction::Remove { json } => ("remove", json),
    };
    let mut value: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| invalid(error.to_string()))?;
    if !value.is_object() {
        return Err(invalid("integration arguments must be a JSON object"));
    }
    value["action"] = json!(action_name);
    let request: IntegrationRequest = decode(value)?;
    Ok(Some(integrator.execute(request)?))
}
