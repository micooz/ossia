fn main() {
    let value = ossia_runtime::mcp::tools();
    let mut text = serde_json::to_string_pretty(&value).unwrap();
    text.push('\n');
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/local/mcp-tools.json");
    if std::env::args().any(|arg| arg == "--check") {
        if std::fs::read_to_string(path).ok().as_deref() != Some(&text) {
            eprintln!("MCP tool schemas differ; run pnpm generate:contracts");
            std::process::exit(1);
        }
    } else {
        std::fs::write(path, text).unwrap();
    }
}
