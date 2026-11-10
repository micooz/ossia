#![cfg_attr(
    all(not(debug_assertions), feature = "gui"),
    windows_subsystem = "windows"
)]

fn main() {
    let args = ossia_runtime::cli::arguments();
    if args.command.is_some() {
        std::process::exit(ossia_runtime::cli::entry(args).unwrap_or(1));
    }
    #[cfg(feature = "gui")]
    ossia_desktop_lib::run(args.config());
    #[cfg(not(feature = "gui"))]
    {
        eprintln!("GUI feature disabled; use serve, mcp or another CLI command");
        std::process::exit(1);
    }
}
