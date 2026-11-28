use super::*;

pub(super) fn detect_with_env(
    paths: &HostPaths,
    resolver: &command::AgentEnvironment,
) -> Result<Option<DetectedAgent>, AppError> {
    let Some((executable, shell_discovered)) =
        resolver.find("codex", paths.codex_bin.as_deref(), candidates())
    else {
        return Ok(None);
    };
    let env = command::host_env("CODEX_HOME", &paths.codex_home);
    let (env, text) = resolver.probe(&executable, env, &["plugin", "--help"], shell_discovered)?;
    let version = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| line.split_whitespace().last())
        .unwrap_or_default()
        .to_string();
    Ok(Some(DetectedAgent {
        command: executable.clone(),
        command_args: Vec::new(),
        executable,
        version,
        config_root: paths.codex_home.clone(),
        profile: "default",
        env,
    }))
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.extend(scan::mac_app(
            "ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
        ));
        paths.extend(scan::mac_app("Codex.app/Contents/MacOS/codex"));
    }
    paths.extend(scan::common_bin("codex"));
    #[cfg(windows)]
    {
        for root in scan::program_files("ChatGPT") {
            paths.push(root.join("resources/codex-cli/codex.exe"));
            paths.push(root.join("codex.exe"));
        }
    }
    paths
}
