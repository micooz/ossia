use super::*;

#[cfg(unix)]
#[test]
fn published_cli_executes_and_updates_after_app_moves() {
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile::tempdir().unwrap();
    let home = work.path().join("中文 library");
    let original = work.path().join("original app");
    std::fs::write(&original, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o755)).unwrap();
    publish(&home, &original).unwrap();
    publish(&home, &original).unwrap();
    let output = std::process::Command::new(cli_path(&home))
        .args(["--home", "a b", "guide"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "--home\na b\nguide\n"
    );
    let moved = work.path().join("moved app");
    std::fs::rename(original, &moved).unwrap();
    publish(&home, &moved).unwrap();
    assert_eq!(
        std::fs::read_link(cli_path(&home)).unwrap(),
        moved.canonicalize().unwrap()
    );
    publish(&home, &cli_path(&home)).unwrap();
}

#[test]
fn publishing_preserves_foreign_files() {
    let work = tempfile::tempdir().unwrap();
    let entry = cli_path(work.path());
    std::fs::write(&entry, "user content").unwrap();
    assert!(publish(work.path(), &std::env::current_exe().unwrap()).is_err());
    assert_eq!(std::fs::read_to_string(entry).unwrap(), "user content");
}

#[cfg(unix)]
#[test]
fn bound_guide_command_preserves_paths_and_resource_options() {
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile::tempdir().unwrap();
    let home = work.path().join("score's 中文 $library");
    std::fs::create_dir(&home).unwrap();
    let cli = cli_path(&home);
    std::fs::write(&cli, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = BTreeMap::from([
        ("OSSIA_ENGINE_ROOT".into(), "/engine's root".into()),
        ("OSSIA_STATIC_DIR".into(), "/ui root".into()),
    ]);
    let output = std::process::Command::new("sh")
        .args(["-c", &guide_command(&home, &env)])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "--home\n{}\n--engine-root\n/engine's root\n--static-dir\n/ui root\nguide\n",
            home.display()
        )
    );
}

#[test]
fn executable_copy_tracks_resources_updates_and_detects_modified_copies() {
    let work = tempfile::tempdir().unwrap();
    let home = work.path().join("library");
    std::fs::create_dir(&home).unwrap();
    let source = work.path().join("original executable");
    std::fs::write(&source, b"version one").unwrap();
    publish_copy(&home, &source).unwrap();
    assert_eq!(resource_executable(&cli_path(&home)).unwrap(), source);
    publish_copy(&home, &source).unwrap();
    std::fs::write(&source, b"version two").unwrap();
    publish_copy(&home, &source).unwrap();
    assert_eq!(std::fs::read(cli_path(&home)).unwrap(), b"version two");
    std::fs::write(cli_path(&home), b"foreign executable").unwrap();
    assert!(publish_copy(&home, &source).is_err());
    assert!(resource_executable(&cli_path(&home)).is_err());
    assert_eq!(
        std::fs::read(cli_path(&home)).unwrap(),
        b"foreign executable"
    );
}
