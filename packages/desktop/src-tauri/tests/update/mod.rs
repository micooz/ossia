use super::{
    feed, github, install, notes, target::PackageManagers, verify, version, UpdateService,
};
use ossia_types::{ReleaseNote, UpdateChannel, UpdateChannelPreference, UpdateSource, UpdateState};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

type Handler = Arc<dyn Fn(&str, Option<&str>) -> MockResponse + Send + Sync>;
type Hits = Arc<Mutex<Vec<(String, Option<String>)>>>;

struct MockResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl MockResponse {
    fn json(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.into(),
        }
    }

    fn text(body: &str) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            headers: vec![("location".into(), location.into())],
            body: Vec::new(),
        }
    }
}

struct Mock {
    base: String,
    hits: Hits,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Mock {
    fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_hits = hits.clone();
        let thread_stop = stop.clone();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => handle(stream, &handler, &thread_hits),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base: format!("http://{address}"),
            hits,
            stop,
            thread: Some(thread),
        }
    }

    fn hits(&self) -> Vec<(String, Option<String>)> {
        self.hits.lock().unwrap().clone()
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn handle(mut stream: TcpStream, handler: &Handler, hits: &Hits) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        let Ok(read) = stream.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    let request = String::from_utf8_lossy(&bytes);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let authorization = request
        .lines()
        .find_map(|line| line.strip_prefix("authorization: "))
        .map(str::to_string);
    hits.lock()
        .unwrap()
        .push((path.to_string(), authorization.clone()));
    let response = handler(path, authorization.as_deref());
    let reason = if response.status == 200 {
        "OK"
    } else if response.status == 302 {
        "Found"
    } else {
        "Error"
    };
    let mut head = format!("HTTP/1.1 {} {reason}\r\n", response.status);
    for (name, value) in response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "content-length: {}\r\nconnection: close\r\n\r\n",
        response.body.len()
    ));
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
}

#[test]
fn version_rules_only_allow_exact_release_tags() {
    assert!(version::is_released("v0.1.0"));
    assert!(version::is_released("v0.1.0-beta.4"));
    assert!(!version::is_released("0.1.0-5-gabc1234"));
    assert!(!version::is_released("0.1.0-dirty"));
    assert!(!version::is_released("0.1.0-untagged"));
    assert!(version::is_newer("v0.1.0", "v0.1.0-beta.4"));
    assert!(!version::is_newer("v0.1.0-beta.4", "v0.1.0"));
}

#[test]
fn auto_channel_follows_release_build_shape() {
    assert_eq!(
        version::effective_channel(UpdateChannelPreference::Auto, "v0.1.0-beta.4"),
        UpdateChannel::Beta
    );
    assert_eq!(
        version::effective_channel(UpdateChannelPreference::Auto, "v0.1.0"),
        UpdateChannel::Stable
    );
    assert_eq!(
        version::effective_channel(UpdateChannelPreference::Stable, "v0.1.0-beta.4"),
        UpdateChannel::Stable
    );
}

#[test]
fn targets_match_published_asset_names() {
    assert_eq!(
        super::target::target_for("macos", "aarch64", PackageManagers::default(),)
            .unwrap()
            .asset_name(),
        "ossia-darwin-arm64.zip"
    );
    let intel = super::target::target_for("macos", "x86_64", PackageManagers::default()).unwrap();
    assert_eq!(intel.asset_name(), "ossia-darwin-amd64.zip");
    assert_eq!(intel.feed_slug(), "darwin-amd64");
    assert_eq!(
        super::target::target_for("windows", "x86_64", PackageManagers::default(),)
            .unwrap()
            .asset_name(),
        "ossia-windows-amd64.exe"
    );
    assert_eq!(
        super::target::target_for(
            "linux",
            "x86_64",
            PackageManagers {
                dpkg: false,
                rpm: true,
            },
        )
        .unwrap()
        .asset_name(),
        "ossia-linux-amd64.rpm"
    );
    assert!(super::target::target_for(
        "linux",
        "aarch64",
        PackageManagers {
            dpkg: true,
            rpm: true,
        },
    )
    .is_none());
}

#[test]
fn checksums_accept_digest_or_sums() {
    let asset = feed::Asset {
        name: "ossia-linux-amd64.deb".into(),
        url: "https://api.github.com/assets/1".into(),
        size: 10,
        digest: Some(format!("sha256:{}", "a".repeat(64))),
    };
    assert_eq!(verify::expected(&asset, None).unwrap(), "a".repeat(64));
    let asset = feed::Asset {
        digest: None,
        ..asset
    };
    let sums = format!("{}  ossia-linux-amd64.deb\n", "b".repeat(64));
    assert_eq!(
        verify::expected(&asset, Some(&sums)).unwrap(),
        "b".repeat(64)
    );
    assert!(verify::expected(&asset, None).is_err());
}

#[test]
fn release_feed_filters_by_channel_and_token() {
    let body = serde_json::json!([
        {
            "tag_name": "v0.2.0-beta.1",
            "draft": false,
            "prerelease": true,
            "body": "beta",
            "assets": []
        },
        {
            "tag_name": "v0.1.0",
            "draft": false,
            "prerelease": false,
            "body": "stable",
            "assets": []
        },
        {
            "tag_name": "v9.0.0",
            "draft": true,
            "prerelease": false,
            "body": "draft",
            "assets": []
        }
    ]);
    let mock = Mock::start(Arc::new(move |_, _| MockResponse::json(body.to_string())));
    let stable = github::releases(
        &mock.base,
        "micooz/ossia",
        Some("secret"),
        Some("v0.1.0-beta.4"),
        UpdateChannel::Stable,
    )
    .unwrap();
    assert_eq!(stable.len(), 1);
    assert_eq!(stable[0].version, "0.1.0");
    assert_eq!(mock.hits()[0].1.as_deref(), Some("Bearer secret"));

    let beta = github::releases(
        &mock.base,
        "micooz/ossia",
        Some("secret"),
        Some("v0.1.0-beta.4"),
        UpdateChannel::Beta,
    )
    .unwrap();
    assert_eq!(beta.len(), 2);
    assert_eq!(beta[0].version, "0.2.0-beta.1");
}

#[test]
fn asset_requests_accept_only_binary_data() {
    let client = super::fetch::no_redirect_client().unwrap();
    let request = super::fetch::asset_get(
        &client,
        "https://api.github.com/repos/micooz/ossia/releases/assets/1",
        Some("secret"),
    )
    .unwrap()
    .build()
    .unwrap();
    let accepts: Vec<_> = request
        .headers()
        .get_all(reqwest::header::ACCEPT)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect();
    assert_eq!(accepts, ["application/octet-stream"]);
}

#[tokio::test]
async fn signed_asset_redirect_does_not_forward_the_token() {
    let mock = Mock::start(Arc::new(|path, _| {
        if path == "/asset" {
            MockResponse::redirect("/signed")
        } else {
            MockResponse::text("sha256 data")
        }
    }));
    let text = super::download::text(&format!("{}/asset", mock.base), Some("secret"))
        .await
        .unwrap();
    assert_eq!(text, "sha256 data");
    let hits = mock.hits();
    assert_eq!(hits[0].1.as_deref(), Some("Bearer secret"));
    assert_eq!(hits[1].1, None);
}

#[tokio::test]
async fn anonymous_asset_requests_have_no_authorization() {
    let mock = Mock::start(Arc::new(|_, _| MockResponse::text("sha256 data")));
    let text = super::download::text(&format!("{}/asset", mock.base), None)
        .await
        .unwrap();
    assert_eq!(text, "sha256 data");
    assert_eq!(mock.hits()[0].1, None);
}

#[test]
fn notes_filter_internal_only_and_keep_range() {
    let cache = notes::NotesCache {
        schema_version: 2,
        source: Some(UpdateSource::Github),
        channel: Some(UpdateChannel::Beta),
        releases: vec![
            ReleaseNote {
                version: "0.2.0-beta.2".into(),
                body: "## Features\n- newer".into(),
            },
            ReleaseNote {
                version: "0.2.0-beta.1".into(),
                body: format!("{}\n\nmaintenance", notes::INTERNAL_ONLY_MARKER),
            },
            ReleaseNote {
                version: "0.1.0-beta.4".into(),
                body: "## Bug Fixes\n- old".into(),
            },
        ],
    };
    let selected = notes::select(&cache, Some("0.1.0-beta.4"), Some("0.2.0-beta.2")).unwrap();
    assert_eq!(selected.releases.len(), 1);
    assert_eq!(selected.releases[0].version, "0.2.0-beta.2");
    assert!(!selected.maintenance_only);

    let latest = notes::select(&cache, Some("0.2.0-beta.2"), Some("0.2.0-beta.2")).unwrap();
    assert_eq!(latest.releases.len(), 1);
    assert_eq!(latest.releases[0].version, "0.2.0-beta.2");
}

#[tokio::test]
async fn debug_build_notes_include_all_channels_and_keep_the_newest_five() {
    let body = serde_json::json!([
        {
            "tag_name": "v0.6.0-beta.1",
            "draft": false,
            "prerelease": true,
            "body": "six",
            "assets": []
        },
        {
            "tag_name": "v0.5.0",
            "draft": false,
            "prerelease": false,
            "body": "five",
            "assets": []
        },
        {
            "tag_name": "v0.4.0-beta.1",
            "draft": false,
            "prerelease": true,
            "body": "four",
            "assets": []
        },
        {
            "tag_name": "v0.3.0",
            "draft": false,
            "prerelease": false,
            "body": "three",
            "assets": []
        },
        {
            "tag_name": "v0.2.0-beta.1",
            "draft": false,
            "prerelease": true,
            "body": "two",
            "assets": []
        },
        {
            "tag_name": "v0.1.5",
            "draft": false,
            "prerelease": false,
            "body": "one",
            "assets": []
        }
    ]);
    let mock = Mock::start(Arc::new(move |_, _| MockResponse::json(body.to_string())));
    let service = UpdateService::for_debug_build(&mock.base, "micooz/ossia", "0.1.0");

    let snapshot = service
        .check_notes(UpdateSource::Github, None, UpdateChannel::Stable)
        .await;
    let notes = snapshot.notes.expect("release notes");

    assert_eq!(notes.to, "0.6.0-beta.1");
    assert_eq!(
        notes
            .releases
            .iter()
            .map(|release| release.version.as_str())
            .collect::<Vec<_>>(),
        [
            "0.6.0-beta.1",
            "0.5.0",
            "0.4.0-beta.1",
            "0.3.0",
            "0.2.0-beta.1"
        ]
    );
}

#[tokio::test]
async fn debug_build_downloads_and_verifies_latest_even_at_same_version() {
    let asset = Mock::start(Arc::new(|_, _| MockResponse::text("test")));
    let asset_url = format!("{}/asset", asset.base);
    let name = super::target::current().unwrap().asset_name();
    let body = serde_json::json!([{ "tag_name": "v0.2.0", "body": "stable", "assets": [{
        "name": name, "url": asset_url, "size": 4,
        "digest": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    }] }]);
    let mock = Mock::start(Arc::new(move |_, _| MockResponse::json(body.to_string())));
    let service = UpdateService::for_debug_build(&mock.base, "micooz/ossia", "v0.2.0");
    let found = service
        .check(UpdateSource::Github, None, UpdateChannel::Stable, false)
        .await;
    assert_eq!(found.state, UpdateState::Found);
    let ready = service
        .check(UpdateSource::Github, None, UpdateChannel::Stable, true)
        .await;
    assert_eq!(ready.state, UpdateState::Ready, "{:?}", ready.error);
    assert!(ready.staged);
    service.finish_on_quit();
}

#[tokio::test]
async fn source_builds_do_not_request_the_feed() {
    let service = UpdateService::for_build("http://127.0.0.1:9", "micooz/ossia", "0.1.0-5-gabc");
    let snapshot = service
        .check(UpdateSource::Github, None, UpdateChannel::Stable, true)
        .await;
    assert_eq!(snapshot.state, UpdateState::Source);
}

#[tokio::test]
async fn release_builds_check_without_a_token() {
    let body = serde_json::json!([
        {
            "tag_name": "v0.2.0",
            "draft": false,
            "prerelease": false,
            "body": "stable",
            "assets": []
        }
    ]);
    let mock = Mock::start(Arc::new(move |_, _| MockResponse::json(body.to_string())));
    let service = UpdateService::for_build(&mock.base, "micooz/ossia", "v0.1.0");
    let snapshot = service
        .check(UpdateSource::Github, None, UpdateChannel::Stable, false)
        .await;
    assert_eq!(snapshot.state, UpdateState::Found);
    assert_eq!(snapshot.latest.as_deref(), Some("0.2.0"));
    assert_eq!(mock.hits()[0].1, None);
}

#[test]
fn ossia_feed_uses_the_target_manifest_and_resolves_relative_assets() {
    let body = serde_json::json!({
        "schemaVersion": 1,
        "target": "darwin-arm64",
        "releases": [
            {
                "version": "v0.3.0-beta.1",
                "prerelease": true,
                "body": "beta",
                "assets": []
            },
            {
                "version": "0.2.0",
                "prerelease": false,
                "body": "stable",
                "assets": [{
                    "name": "ossia-darwin-arm64.zip",
                    "url": "0.2.0/ossia-darwin-arm64.zip",
                    "size": 10,
                    "digest": format!("sha256:{}", "a".repeat(64))
                }]
            }
        ]
    });
    let mock = Mock::start(Arc::new(move |_, _| MockResponse::json(body.to_string())));
    let endpoints = feed::Endpoints {
        github_base: Arc::from("http://127.0.0.1:9"),
        github_repository: Arc::from("micooz/ossia"),
        ossia_base: Arc::from(mock.base.as_str()),
    };
    let target = super::target::target_for("macos", "aarch64", PackageManagers::default()).unwrap();
    let releases = feed::releases(
        &endpoints,
        UpdateSource::Ossia,
        Some("secret"),
        None,
        UpdateChannel::Stable,
        Some(target),
    )
    .unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].version, "0.2.0");
    assert_eq!(
        releases[0].assets[0].url,
        format!("{}/darwin-arm64/0.2.0/ossia-darwin-arm64.zip", mock.base)
    );
    assert_eq!(mock.hits()[0].0, "/darwin-arm64/index.json");
    assert_eq!(mock.hits()[0].1, None);
}

#[test]
fn ossia_feed_rejects_unknown_schema_and_wrong_target() {
    let bad_schema = serde_json::json!({
        "schemaVersion": 2,
        "target": "darwin-arm64",
        "releases": []
    });
    let mock = Mock::start(Arc::new(move |_, _| {
        MockResponse::json(bad_schema.to_string())
    }));
    let target = super::target::target_for("macos", "aarch64", PackageManagers::default()).unwrap();
    let error =
        super::ossia::releases(&mock.base, target, None, UpdateChannel::Stable).unwrap_err();
    assert!(error.to_string().contains("unsupported schema version"));

    let bad_target = serde_json::json!({
        "schemaVersion": 1,
        "target": "windows-amd64",
        "releases": []
    });
    let mock = Mock::start(Arc::new(move |_, _| {
        MockResponse::json(bad_target.to_string())
    }));
    let error =
        super::ossia::releases(&mock.base, target, None, UpdateChannel::Stable).unwrap_err();
    assert!(error.to_string().contains("expected target darwin-arm64"));
}

#[test]
fn platform_install_arguments_and_quotes_are_stable() {
    assert_eq!(
        install::windows::installer_args(true, &["--home".into(), "C:\\My Ossia".into()]),
        "/S /UPDATE /R /ARGS --home \"C:\\My Ossia\""
    );
    assert_eq!(install::sh_quote("a b'c"), "'a b'\\''c'");
    let script = install::relaunch_script(
        std::path::Path::new("/Applications/Ossia.app/Contents/MacOS/ossia"),
        42,
        std::path::Path::new("/tmp/relaunch.log"),
        &["--home".into(), "/tmp/a b".into()],
    );
    assert!(script.contains("kill -0 42"));
    assert!(script.contains("'/tmp/a b'"));
}

#[test]
fn invalid_release_tags_fail_the_check() {
    let body = serde_json::json!([
        {
            "tag_name": "not-a-version",
            "draft": false,
            "prerelease": false,
            "body": "",
            "assets": []
        }
    ]);
    let mock = Mock::start(Arc::new(move |_, _| MockResponse::json(body.to_string())));
    let error = github::releases(
        &mock.base,
        "micooz/ossia",
        Some("secret"),
        None,
        UpdateChannel::Stable,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid release tag"));
}
