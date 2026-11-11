use super::*;

#[test]
fn quit_request_is_single_flight_and_waits_for_approval() {
    let quit = QuitState::new();
    assert!(quit.request());
    assert!(!quit.request());
    assert!(quit.stopping());
    assert!(!quit.approved());
    quit.approve();
    assert!(quit.approved());
}

#[tokio::test]
async fn status_before_runtime_discovery_is_not_an_error() {
    let root = tempfile::tempdir().unwrap();
    let config = server::Config {
        home: root.path().into(),
        port: 0,
        engine_root: root.path().join("engine"),
        static_dir: None,
        dev_origin: None,
    };
    let status = snapshot(&config).await;
    assert!(status.runtime.is_none());
    assert!(status.error.is_none(), "{:?}", status.error);

    // Invalid discovery data must still surface as a real configuration error.
    std::fs::create_dir(root.path().join("run")).unwrap();
    std::fs::write(root.path().join("run/runtime.json"), b"invalid").unwrap();
    let status = snapshot(&config).await;
    assert_eq!(status.error.unwrap().code, ErrorCode::StorageError);
}
