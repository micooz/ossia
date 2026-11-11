use super::*;

#[test]
fn explicit_and_known_candidate_paths_are_both_accepted() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ossia-probe-agent");
    std::fs::write(&path, b"#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    assert_eq!(
        find("ossia-probe-agent", Some(&path), []),
        Some(path.clone())
    );
    assert_eq!(find("ossia-probe-agent", None, [path.clone()]), Some(path));
}
