use super::*;

#[cfg(unix)]
#[test]
fn shared_environment_recovers_cli_and_reports_failures() {
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let cli = bin.join("ossia-test-shell-agent");
    std::fs::write(&cli, "#!/bin/sh\necho 1.0\n").unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let resolver = AgentEnvironment::with_path_loader(move || {
        count.fetch_add(1, Ordering::SeqCst);
        Some(bin.to_string_lossy().into_owned())
    });
    let found = resolver
        .find("ossia-test-shell-agent", None, Vec::<PathBuf>::new())
        .unwrap();
    assert_eq!(found, (cli.clone(), true));
    assert_eq!(
        resolver.find("ossia-test-shell-agent", None, Vec::<PathBuf>::new()),
        Some((cli.clone(), true))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let (_, version) = resolver.probe(&cli, BTreeMap::new(), &[], false).unwrap();
    assert_eq!(version.trim(), "1.0");
    let (env, _) = resolver.probe(&cli, BTreeMap::new(), &[], true).unwrap();
    assert_eq!(
        env.get("PATH").map(String::as_str),
        cli.parent().unwrap().to_str()
    );
    std::fs::write(&cli, "#!/bin/sh\necho 'diagnostic failure' >&2\nexit 1\n").unwrap();
    assert!(resolver
        .probe(&cli, BTreeMap::new(), &[], false)
        .unwrap_err()
        .message
        .contains("diagnostic failure"));
}

#[cfg(unix)]
#[test]
fn pending_shell_lookup_does_not_delay_an_independent_detected_agent() {
    use std::{os::unix::fs::PermissionsExt, sync::mpsc};
    let root = tempfile::tempdir().unwrap();
    let existing = root.path().join("existing");
    std::fs::write(&existing, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&existing, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (started, received) = mpsc::sync_channel(0);
    let resolver = AgentEnvironment::with_path_loader(move || {
        started.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        None
    });
    std::thread::scope(|scope| {
        scope.spawn(|| {
            resolver.find("ossia-absent-agent", None, Vec::<PathBuf>::new());
        });
        received.recv().unwrap();
        let start = Instant::now();
        assert_eq!(
            resolver.find(
                "ossia-existing-agent",
                Some(&existing),
                Vec::<PathBuf>::new()
            ),
            Some((existing.clone(), false))
        );
        assert!(start.elapsed() < Duration::from_millis(200));
    });
}

#[cfg(unix)]
#[test]
fn shell_path_is_loaded_once_for_concurrent_discovery() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let resolver = AgentEnvironment::with_path_loader(move || {
        count.fetch_add(1, Ordering::SeqCst);
        Some("/does-not-exist".into())
    });
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                assert!(resolver
                    .find("ossia-test-absent-agent", None, Vec::<PathBuf>::new())
                    .is_none());
            });
        }
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[test]
fn explicit_and_known_executables_take_precedence_over_shell_discovery() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let explicit = root.path().join("explicit");
    let known = root.path().join("known");
    for path in [&explicit, &known] {
        std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let resolver = AgentEnvironment::with_path_loader(|| panic!("shell should not run"));
    assert_eq!(
        resolver.find(
            "ossia-absent-priority-test",
            Some(&explicit),
            vec![known.clone()]
        ),
        Some((explicit, false))
    );
    assert_eq!(
        resolver.find("ossia-absent-priority-test", None, vec![known.clone()]),
        Some((known, false))
    );
    assert!(resolver
        .find(
            "ossia-absent-priority-test",
            Some(&root.path().join("missing")),
            Vec::<PathBuf>::new()
        )
        .is_none());
}

#[cfg(unix)]
#[test]
fn retry_failure_reports_both_command_errors() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let cli = root.path().join("agent");
    std::fs::write(&cli, "#!/bin/sh\nif [ \"$PATH\" = /shell ]; then echo 'retry failure' >&2; else echo 'initial failure' >&2; fi\nexit 1\n").unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let resolver = AgentEnvironment::with_path_loader(|| Some("/shell".into()));
    let error = resolver
        .probe(
            &cli,
            BTreeMap::from([("PATH".into(), "/initial".into())]),
            &[],
            false,
        )
        .unwrap_err();
    assert!(error.message.contains("initial failure"));
    assert!(error.message.contains("retry failure"));
}

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
