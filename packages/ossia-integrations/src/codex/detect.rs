use super::*;

pub(super) fn detect(paths: &HostPaths) -> Option<DetectedAgent> {
    let executable = scan::find("codex", paths.codex_bin.as_deref(), candidates())?;
    let env = command::host_env("CODEX_HOME", &paths.codex_home);
    let version = command::version(&executable, &env).ok()?;
    command::run(
        &executable,
        &command::arguments(&["plugin", "--help"]),
        &env,
    )
    .ok()?;
    Some(DetectedAgent {
        executable,
        version,
        config_root: paths.codex_home.clone(),
        profile: "default",
        env,
    })
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
