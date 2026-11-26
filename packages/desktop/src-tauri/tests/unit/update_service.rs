use super::{Staged, UpdateService};
use ossia_types::{UpdateChannel, UpdateChannelPreference, UpdateSource, UpdateState};
use std::fs;
use std::sync::Arc;

#[tokio::test]
async fn debug_build_cannot_install_a_staged_download() {
    let service = UpdateService::for_debug_build("http://127.0.0.1:9", "micooz/ossia", "v0.1.0");
    let dir = tempfile::tempdir().unwrap();
    let staged = dir.path().join("update.zip");
    fs::write(&staged, b"download").unwrap();
    *service.staged.lock().unwrap() = Some(Staged {
        path: staged.clone(),
        version: "0.2.0".into(),
    });
    let result = service.install().await;
    match result {
        super::InstallOutcome::Failed(snapshot) => {
            assert!(snapshot.error.unwrap().contains("cannot install"))
        }
        _ => panic!("debug install should be rejected"),
    }
    assert!(staged.exists());
}

#[test]
fn finish_on_quit_discards_an_unused_download() {
    let service = UpdateService::for_build("http://127.0.0.1:9", "micooz/ossia", "v0.1.0");
    let dir = tempfile::tempdir().unwrap();
    let staged = dir.path().join("ossia-update.zip");
    fs::write(&staged, b"downloaded update").unwrap();
    *service.staged.lock().unwrap() = Some(Staged {
        path: staged,
        version: "0.2.0".into(),
    });

    service.finish_on_quit();

    assert!(service.staged().is_none());
    assert!(!dir.path().exists());
}

#[tokio::test]
async fn update_checks_publish_status_and_configuration_changes_clear_it() {
    let home = tempfile::tempdir().unwrap();
    let mut service = UpdateService::for_build("http://127.0.0.1:9", "micooz/ossia", "v0.1.0");
    service.status_home = Some(Arc::new(home.path().to_owned()));
    service
        .check(UpdateSource::Github, None, UpdateChannel::Stable, false)
        .await;
    let status = ossia_backend::update_status::read(home.path())
        .unwrap()
        .unwrap();
    assert_eq!(status.state, UpdateState::Error);
    assert_eq!(status.source, UpdateSource::Github);
    service.set_channel(UpdateChannelPreference::Beta);
    assert!(ossia_backend::update_status::read(home.path())
        .unwrap()
        .is_none());

    let mut source = UpdateService::for_build("http://127.0.0.1:9", "micooz/ossia", "0.1.0-5-gabc");
    source.status_home = Some(Arc::new(home.path().to_owned()));
    source
        .check(UpdateSource::Ossia, None, UpdateChannel::Stable, false)
        .await;
    assert_eq!(
        ossia_backend::update_status::read(home.path())
            .unwrap()
            .unwrap()
            .state,
        UpdateState::Source
    );
    source.set_source(UpdateSource::Github);
    assert!(ossia_backend::update_status::read(home.path())
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn successful_check_publishes_available_version() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        let body = r#"[{"tag_name":"v0.2.0","draft":false,"prerelease":false,"body":"stable","assets":[]}]"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let home = tempfile::tempdir().unwrap();
    let mut service =
        UpdateService::for_build(&format!("http://{address}"), "micooz/ossia", "v0.1.0");
    service.status_home = Some(Arc::new(home.path().to_owned()));
    service
        .check(UpdateSource::Github, None, UpdateChannel::Stable, false)
        .await;
    server.join().unwrap();
    let status = ossia_backend::update_status::read(home.path())
        .unwrap()
        .unwrap();
    assert_eq!(status.state, UpdateState::Found);
    assert_eq!(status.latest.as_deref(), Some("0.2.0"));
    assert!(!status.stale);
}

#[tokio::test]
async fn cancelling_stalled_downloads_cleans_up_and_allows_switching_source() {
    use super::super::{feed::Asset, stage::Download};
    use std::io::{Read, Write};
    use std::time::Duration;

    for (checksum, send_headers) in [(false, false), (false, true), (true, true)] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (accepted, received) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            if send_headers {
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\npartial")
                    .unwrap();
            }
            accepted.send(()).unwrap();
            let _ = stream.read(&mut request);
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset.zip");
        let url = format!("http://{address}/asset");
        let prepared = Download {
            asset: Asset {
                name: "asset.zip".into(),
                url: url.clone(),
                size: 1024,
                digest: (!checksum).then(|| format!("sha256:{}", "0".repeat(64))),
            },
            version: "0.2.0".into(),
            sums_url: checksum.then_some(url),
            path: path.clone(),
            token: None,
        };
        let service = UpdateService::for_build("http://127.0.0.1:9", "micooz/ossia", "v0.1.0");
        let task_service = service.clone();
        let task = tokio::spawn(async move { task_service.finish_download(prepared).await });
        tokio::time::timeout(Duration::from_secs(3), async {
            while received.try_recv().is_err() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(service.snapshot().state, UpdateState::Downloading);
        assert_eq!(
            service.set_channel(UpdateChannelPreference::Beta).state,
            UpdateState::Downloading
        );
        let cancelled = tokio::time::timeout(Duration::from_secs(2), service.cancel_download())
            .await
            .unwrap();
        assert_eq!(cancelled.state, UpdateState::Idle);
        assert!(cancelled.error.is_none());
        assert!(!path.exists());
        assert!(!dir.path().exists());
        assert_eq!(task.await.unwrap().state, UpdateState::Idle);
        assert_eq!(
            service.set_source(UpdateSource::Github).source,
            UpdateSource::Github
        );
        server.join().unwrap();
    }
}
