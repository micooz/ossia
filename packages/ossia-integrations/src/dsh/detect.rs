use super::*;

pub(super) fn detect(paths: &HostPaths) -> Option<DetectedAgent> {
    let executable = scan::find("dsh", paths.dsh_bin.as_deref(), candidates())?;
    let env = command::host_env("DSH_HOME", &paths.dsh_home);
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
        config_root: paths.dsh_home.clone(),
        profile: "desktop",
        env,
    })
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
            paths.push(root.join("runtime/cli/bin/dsh.exe"));
            paths.push(root.join("dsh.exe"));
        }
    }
    paths
}
