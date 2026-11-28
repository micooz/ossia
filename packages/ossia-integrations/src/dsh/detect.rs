use super::*;

pub(super) fn detect_with_env(
    paths: &HostPaths,
    resolver: &command::AgentEnvironment,
) -> Result<Option<DetectedAgent>, AppError> {
    let Some((executable, shell_discovered)) =
        resolver.find("dsh", paths.dsh_bin.as_deref(), candidates())
    else {
        return Ok(None);
    };
    let env = command::host_env("DSH_HOME", &paths.dsh_home);
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
        config_root: paths.dsh_home.clone(),
        profile: "desktop",
        env,
    }))
}

#[cfg(all(test, windows))]
fn detect(paths: &HostPaths) -> Option<DetectedAgent> {
    detect_with_env(paths, &command::AgentEnvironment::new())
        .ok()
        .flatten()
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.extend(scan::mac_app(
            "DeepSeek Harness.app/Contents/Resources/runtime/cli/bin/dsh",
        ));
    }
    paths.extend(scan::common_bin("dsh"));
    #[cfg(windows)]
    {
        for root in scan::program_files("DeepSeek Harness") {
            paths.extend(installation_candidates(root));
        }
    }
    paths
}

#[cfg(windows)]
fn installation_candidates(root: PathBuf) -> Vec<PathBuf> {
    vec![
        root.join("resources/runtime/cli/bin/dsh.exe"),
        root.join("resources/runtime/cli/bin/dsh.cmd"),
        root.join("runtime/cli/bin/dsh.exe"),
        root.join("runtime/cli/bin/dsh.cmd"),
        root.join("dsh.exe"),
    ]
}

#[cfg(test)]
#[path = "../../tests/dsh/detect.rs"]
mod tests;
