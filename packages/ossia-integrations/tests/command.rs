use super::*;

#[cfg(unix)]
#[test]
fn scan_command_times_out_without_blocking_on_output() {
    let start = std::time::Instant::now();
    let error = run_with_timeout(
        Path::new("/bin/sh"),
        &["-c".into(), "exec sleep 5".into()],
        &BTreeMap::new(),
        std::time::Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(error.message.contains("timed out"));
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}

#[cfg(unix)]
#[test]
fn scan_command_reads_large_output_without_deadlocking() {
    let output = run_with_timeout(
        Path::new("/bin/sh"),
        &["-c".into(), "yes x | head -c 200000".into()],
        &BTreeMap::new(),
        std::time::Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(output.len(), 200000);
}

#[cfg(windows)]
#[test]
fn windows_command_scripts_run_through_the_command_interpreter() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("Probe Agent");
    std::fs::create_dir_all(&directory).unwrap();
    let script = directory.join("probe.cmd");
    std::fs::write(&script, "@echo off\r\necho %~1\r\n").unwrap();

    let output = run(&script, &["value with spaces".into()], &BTreeMap::new()).unwrap();

    assert_eq!(output.trim(), "value with spaces");
}

#[cfg(windows)]
#[test]
fn windows_path_candidates_prefer_native_executables() {
    let root = tempfile::tempdir().unwrap();

    assert_eq!(
        candidates(root.path(), "probe")
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["probe.exe", "probe.com", "probe.cmd", "probe.bat"]
    );
}

#[cfg(windows)]
#[test]
fn windows_path_search_skips_extensionless_shims() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("Agent Bin");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("probe"), "#!/bin/sh\n").unwrap();
    std::fs::write(directory.join("probe.cmd"), "@echo off\r\n").unwrap();

    assert_eq!(
        find_program_on_path("probe", directory.as_os_str()),
        Some(directory.join("probe.cmd"))
    );
}

#[test]
fn host_environment_paths_are_normalized() {
    let env = host_env(
        "OSSIA_ENGINE_ROOT",
        Path::new(r"\\?\E:\Projects\ossia\engine"),
    );
    assert_eq!(
        env.get("OSSIA_ENGINE_ROOT").map(String::as_str),
        Some(r"E:\Projects\ossia\engine")
    );
}
