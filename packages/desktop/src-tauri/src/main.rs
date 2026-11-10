#![cfg_attr(
    all(not(debug_assertions), feature = "gui"),
    windows_subsystem = "windows"
)]

fn main() {
    if let Some(code) = ossia_runtime::cli::entry() {
        std::process::exit(code);
    }
    #[cfg(feature = "gui")]
    ossia_desktop_lib::run();
    #[cfg(not(feature = "gui"))]
    {
        eprintln!("GUI feature disabled; use serve, mcp or another CLI command");
        std::process::exit(1);
    }
}
