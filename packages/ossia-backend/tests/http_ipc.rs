mod common;

use common::{wait_job, Engine, SOURCE};
use ossia_backend::*;
use ossia_library::Library;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn language_route_persists_and_broadcasts_only_valid_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut initial = preferences::load(root.path()).unwrap();
    initial.theme = UiTheme::Dark;
    preferences::save(root.path(), &initial).unwrap();
    let library = Library::open(root.path()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let runtime = Runtime::new(library, Arc::new(Engine::default()), base.clone());
    let app = http::router(runtime.clone(), None, None, Vec::new());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let before = runtime.agent_status().unwrap();
    assert_eq!(
        client
            .put(format!("{base}/api/v1/ui-language"))
            .json(&json!({"language":"zh-CN"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let saved = preferences::load(root.path()).unwrap();
    assert_eq!(saved.language, Some(UiLanguage::ZhCn));
    assert_eq!(saved.theme, UiTheme::Dark);
    assert_eq!(runtime.ui_language(), saved.language);
    let events = runtime
        .events(None, &before.instance_id, before.seq)
        .unwrap();
    assert!(events
        .iter()
        .any(|event| event.r#type == "ui-language-changed"));
    assert!(!client
        .put(format!("{base}/api/v1/ui-language"))
        .json(&json!({"language":"invalid"}))
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    assert_eq!(runtime.ui_language(), Some(UiLanguage::ZhCn));
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_document_routes_support_localhost_without_tokens_or_views() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = Arc::new(Engine::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://localhost:{port}");
    let ipv4 = format!("http://127.0.0.1:{port}");
    let runtime = Runtime::new(library, engine, base.clone());
    let scheduler = tokio::spawn(runtime.clone().schedule());
    let app = http::router(
        runtime.clone(),
        None,
        Some("http://127.0.0.1:5173".into()),
        Vec::new(),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let api = format!("{base}/api/v1");
    for origin in [&base, &ipv4] {
        assert_eq!(client.get(origin).send().await.unwrap().status(), 404);
    }
    let health: Value = client
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        health,
        json!({"alive":true,"protocolVersion":PROTOCOL_VERSION})
    );
    assert_eq!(
        client
            .get(format!("{ipv4}/health"))
            .header("Host", "evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(format!("{api}/status"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{api}/status"))
            .header("Origin", "https://evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let response = client
        .get(format!("{api}/status"))
        .header("Origin", "http://127.0.0.1:5173")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://127.0.0.1:5173"
    );
    assert_eq!(
        client
            .post(format!("{api}/sessions/exchange"))
            .json(&json!({"ticket":"old"}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .get(format!("{api}/views/old"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let multipart = reqwest::multipart::Form::new()
        .text(
            "metadata",
            json!({"title":"upload","format":"ly","author":"test"}).to_string(),
        )
        .part(
            "file",
            reqwest::multipart::Part::bytes(SOURCE.to_vec()).file_name("ignored.ly"),
        );
    let response = client
        .post(format!("{api}/documents"))
        .multipart(multipart)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let job: Job = response.json().await.unwrap();
    let imported = wait_job(&runtime, job.job_id).await;
    assert_eq!(imported.state, JobState::Done);
    let imported: ImportOutcome = serde_json::from_value(imported.result.unwrap()).unwrap();
    let document = imported.document.unwrap();
    assert!(!document.source_path.contains(r"\\?\"));
    assert_eq!(
        runtime
            .agent_context(document.document_id)
            .unwrap()
            .document
            .url,
        format!("{base}/scores?documentId={}", document.document_id)
    );
    let context: DocumentContext = client
        .get(format!("{api}/documents/{}/context", document.document_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(context.document.document_id, document.document_id);
    let settings_url = format!("{api}/documents/{}/settings", document.document_id);
    let empty: ScoreSettings = client
        .get(&settings_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(empty.revision, 0);
    let mut settings = empty.clone();
    settings
        .instruments
        .insert("staff:guitar".into(), SampleInstrument::Guitar);
    let saved: ScoreSettings = client
        .put(&settings_url)
        .json(&settings)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(
        client
            .get(&settings_url)
            .send()
            .await
            .unwrap()
            .json::<ScoreSettings>()
            .await
            .unwrap(),
        saved
    );
    assert_eq!(
        client
            .put(&settings_url)
            .json(&settings)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    let drawing_url = format!("{api}/documents/{}/drawing", document.document_id);
    let empty: Drawing = client
        .get(&drawing_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(empty.revision, 0);
    let drawing = Drawing {
        revision: 0,
        strokes: vec![DrawingStroke {
            page: 1,
            width: 0.01,
            color: DrawingColor::Black,
            points: vec![DrawingPoint { x: 0.25, y: 0.5 }],
        }],
    };
    let saved: Drawing = client
        .put(&drawing_url)
        .json(&drawing)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(
        client
            .put(&drawing_url)
            .json(&drawing)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    let events = runtime
        .events(
            Some(document.document_id),
            &context.instance_id,
            context.seq,
        )
        .unwrap();
    assert!(events
        .iter()
        .any(|event| event.r#type == "drawing-changed" && event.payload["revision"] == 1));
    let agent_saved = runtime
        .write_drawing(WriteDrawingArgs {
            document_id: document.document_id,
            expected_revision: 1,
            change: DrawingChange::Edit {
                edits: vec![DrawingEdit::Add {
                    stroke: drawing.strokes[0].clone(),
                }],
            },
        })
        .unwrap();
    assert_eq!(agent_saved.revision, 2);
    let via_http: Drawing = client
        .get(&drawing_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(via_http.revision, 2);
    assert_eq!(via_http.strokes.len(), 2);
    let agent_events = runtime
        .events(
            Some(document.document_id),
            &context.instance_id,
            events.last().unwrap().seq,
        )
        .unwrap();
    assert!(agent_events
        .iter()
        .any(|event| event.r#type == "drawing-changed" && event.payload["revision"] == 2));
    let drawing_cursor = agent_events.last().unwrap().seq;
    let key = ossia_library::render_key(
        document.document_id,
        document.last_successful_render.as_ref().unwrap(),
    );
    let page = client
        .get(format!("{api}/renders/{key}/pages/1"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert_eq!(page.headers()["cache-control"], "private, no-cache");
    let etag = page.headers()["etag"].clone();
    let cached = client
        .get(format!("{api}/renders/{key}/pages/1"))
        .header("If-None-Match", etag)
        .send()
        .await
        .unwrap();
    assert_eq!(cached.status(), 304);
    assert!(cached.bytes().await.unwrap().is_empty());
    assert_eq!(
        client
            .get(format!("{api}/renders/{key}/pages/999"))
            .header("If-None-Match", "*")
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let located = client
        .post(format!("{api}/documents/{}/locate", document.document_id))
        .json(&LocateArgs {
            source_hash: document.source_hash.clone(),
            page: 1,
            x: -1.0,
            y: -1.0,
        })
        .send()
        .await
        .unwrap();
    let located: LocateOutcome = located.json().await.unwrap();
    assert_eq!(located.capability, LocationCapability::None);
    let mut stream = client
        .get(format!(
            "{api}/events?instanceId={}&after={}&documentId={}",
            context.instance_id, drawing_cursor, document.document_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), 200);
    runtime.emit("test-event", Some(document.document_id), None, json!({}));
    let bytes = tokio::time::timeout(Duration::from_secs(2), stream.chunk())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("test-event"));
    drop(stream);
    for route in [
        format!("documents/{}", document.document_id),
        format!("documents/{}/versions", document.document_id),
        format!("jobs/{}", job.job_id),
        "documents".into(),
    ] {
        assert_eq!(
            client
                .get(format!("{api}/{route}"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    let documents: DocumentPage = client
        .get(format!("{api}/documents?limit=1&offset=0"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(documents.items.len() <= 1);
    let scores: ManagementScorePage = client
        .get(format!("{api}/library/scores?limit=1&offset=0"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(scores.items[0].document_id, document.document_id);
    let preview: ManagementScorePreview = client
        .post(format!(
            "{api}/library/scores/{}/preview",
            document.document_id
        ))
        .json(&json!({"retry": false}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(matches!(
        preview,
        ManagementScorePreview::Ready { .. }
            | ManagementScorePreview::Pending { .. }
            | ManagementScorePreview::Missing
    ));
    let versions: VersionPage = client
        .get(format!(
            "{api}/documents/{}/versions?limit=1&offset=0",
            document.document_id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(versions.items.len(), 1);
    let mut edit: EditSession = client
        .post(format!("{api}/documents/{}/edits", document.document_id))
        .json(&json!({"action":"begin","documentId":document.document_id,"baseVersionId":document.head_version_id,"expectedSourceHash":document.source_hash}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let status = client
        .get(format!("{api}/status"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!status.contains(&edit.edit_id.to_string()));
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert!(!status["runtime"]["home"]
        .as_str()
        .unwrap()
        .contains(r"\\?\"));
    let changed = b"\\score { \\new Staff { e'1 | d'1 | } \\layout {} }";
    std::fs::write(&document.source_path, changed).unwrap();
    let hash = ContentHash::of(changed);
    let checked = client
        .post(format!("{api}/documents/{}/check", document.document_id))
        .json(&CheckArgs {
            expect: None,
            reference: None,
            document_id: Some(document.document_id),
            edit_id: Some(edit.edit_id),
            expected_source_hash: hash.clone(),
            wait_ms: None,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(checked.status(), 202);
    let checked: Job = checked.json().await.unwrap();
    assert_eq!(
        wait_job(&runtime, checked.job_id).await.state,
        JobState::Done
    );
    let version: Version = client
        .post(format!("{api}/documents/{}/commit", document.document_id))
        .json(&CommitArgs {
            edit_id: edit.edit_id,
            expected_source_hash: hash.clone(),
            summary: "HTTP commit".into(),
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(version.seq, 2);
    let output_path = root.path().join("http-export.ly");
    let exported = client
        .post(format!("{api}/outputs"))
        .json(&OutputArgs {
            source: OutputSource::Version {
                version_id: version.version_id,
            },
            format: OutputFormat::Ly,
            pages: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: Some(output_path.to_string_lossy().into_owned()),
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
            wait_ms: None,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(exported.status(), 202);
    let exported: Job = exported.json().await.unwrap();
    assert_eq!(
        wait_job(&runtime, exported.job_id).await.state,
        JobState::Done
    );
    let downloaded = client
        .get(format!(
            "{api}/jobs/{}/output?filename={}",
            exported.job_id,
            utf8_percent_encode("export.ly", NON_ALPHANUMERIC)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(downloaded.status(), 200);
    assert_eq!(
        downloaded.headers()["content-disposition"],
        "attachment; filename=\"export.ly\"; filename*=UTF-8''export%2Ely"
    );
    assert!(downloaded.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/plain"));
    assert_eq!(downloaded.bytes().await.unwrap().as_ref(), changed);
    let encoded = client
        .get(format!(
            "{api}/jobs/{}/output?filename={}",
            exported.job_id,
            utf8_percent_encode("心湖.pdf", NON_ALPHANUMERIC)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(encoded.status(), 200);
    assert!(encoded.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("filename*=UTF-8''%E5%BF%83%E6%B9%96%2Epdf"));
    let spaced = client
        .get(format!(
            "{api}/jobs/{}/output?filename=My+Score.pdf",
            exported.job_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(spaced.status(), 200);
    assert!(spaced.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("filename*=UTF-8''My%20Score%2Epdf"));
    assert_eq!(
        client
            .get(format!(
                "{api}/jobs/{}/output?filename={}",
                exported.job_id,
                utf8_percent_encode("bad\nname", NON_ALPHANUMERIC)
            ))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        client
            .get(format!("{api}/jobs/{}/output", job.job_id))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    std::fs::write(&output_path, b"tampered").unwrap();
    assert_eq!(
        client
            .get(format!("{api}/jobs/{}/output", exported.job_id))
            .send()
            .await
            .unwrap()
            .status(),
        500
    );
    let batch_directory = root.path().join("batch");
    std::fs::create_dir(&batch_directory).unwrap();
    let batch_path = batch_directory.join("page-001.svg");
    let batch_bytes = b"<svg viewBox=\"0 0 1 1\"/>";
    std::fs::write(&batch_path, batch_bytes).unwrap();
    let batch_job = runtime
        .library
        .create_job("output", Some(document.document_id), None)
        .unwrap();
    runtime
        .library
        .transition_job(
            batch_job.job_id,
            JobState::Queued,
            JobState::Running,
            "execute",
            None,
            None,
        )
        .unwrap();
    runtime
        .library
        .transition_job(
            batch_job.job_id,
            JobState::Running,
            JobState::Done,
            "finished",
            Some(
                serde_json::to_value(OutputBatchOutcome {
                    source: OutputSource::Version {
                        version_id: version.version_id,
                    },
                    document_id: document.document_id,
                    source_hash: hash.clone(),
                    engine_build_id: "test".into(),
                    format: OutputFormat::Svg,
                    directory: batch_directory.to_string_lossy().into_owned(),
                    files: vec![OutputFile {
                        page: 1,
                        bounds: GeometryBounds {
                            x0: 0.0,
                            y0: 0.0,
                            x1: 1.0,
                            y1: 1.0,
                        },
                        path: batch_path.to_string_lossy().into_owned(),
                        bytes: batch_bytes.len() as u64,
                        sha256: ContentHash::of(batch_bytes),
                        media_type: "image/svg+xml".into(),
                    }],
                })
                .unwrap(),
            ),
            None,
        )
        .unwrap();
    let indexed = client
        .get(format!("{api}/jobs/{}/outputs/0", batch_job.job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(indexed.status(), 200);
    assert_eq!(indexed.bytes().await.unwrap().as_ref(), batch_bytes);
    std::fs::write(&batch_path, b"changed").unwrap();
    assert_eq!(
        client
            .get(format!("{api}/jobs/{}/outputs/0", batch_job.job_id))
            .send()
            .await
            .unwrap()
            .status(),
        500
    );
    let restored = client
        .post(format!("{api}/documents/{}/restore", document.document_id))
        .json(&RestoreArgs {
            version_id: document.head_version_id.unwrap(),
            base_version_id: version.version_id,
            expected_source_hash: hash,
            discard_current_edit: false,
            author: Some("http".into()),
            summary: "restore".into(),
            wait_ms: None,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(restored.status(), 202);
    let restored: Job = restored.json().await.unwrap();
    assert_eq!(
        wait_job(&runtime, restored.job_id).await.state,
        JobState::Done
    );
    let current = runtime.library.get_document(document.document_id).unwrap();
    edit = client
        .post(format!("{api}/documents/{}/edits", document.document_id))
        .json(&json!({"action":"begin","documentId":document.document_id,"baseVersionId":current.head_version_id,"expectedSourceHash":current.source_hash}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ended = client
        .post(format!("{api}/documents/{}/edits", document.document_id))
        .json(&json!({"action":"end","documentId":document.document_id,"editId":edit.edit_id,"baseVersionId":current.head_version_id,"expectedSourceHash":current.source_hash}))
        .send()
        .await
        .unwrap();
    assert_eq!(ended.status(), 200);
    let malformed = reqwest::multipart::Form::new()
        .text("metadata", "[]")
        .part("file", reqwest::multipart::Part::bytes(SOURCE.to_vec()));
    assert_eq!(
        client
            .post(format!("{api}/documents"))
            .multipart(malformed)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    runtime.stop().unwrap();
    scheduler.await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_boundary_accepts_configured_lan_hosts_and_origins() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = Arc::new(Engine::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://192.168.1.20:{port}");
    let runtime = Runtime::new(library, engine, base.clone());
    let scheduler = tokio::spawn(runtime.clone().schedule());
    let app = http::router(runtime.clone(), None, None, vec!["192.168.1.20".into()]);
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let local = format!("http://127.0.0.1:{port}");
    assert_eq!(
        client
            .get(format!("{local}/health"))
            .header("Host", format!("192.168.1.20:{port}"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let response = client
        .get(format!("{local}/api/v1/status"))
        .header("Host", format!("192.168.1.20:{port}"))
        .header("Origin", &base)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["access-control-allow-origin"], base);
    for (host, origin) in [
        (format!("evil.example:{port}"), None),
        (format!("192.168.1.20:{port}"), Some("https://evil.example")),
    ] {
        let mut request = client
            .get(format!("{local}/api/v1/status"))
            .header("Host", host);
        if let Some(origin) = origin {
            request = request.header("Origin", origin);
        }
        assert_eq!(request.send().await.unwrap().status(), 403);
    }
    runtime.stop().unwrap();
    scheduler.await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dev_scores_route_redirects_to_vite_origin() {
    let root = tempfile::tempdir().unwrap();
    let web = tempfile::tempdir().unwrap();
    std::fs::write(
        web.path().join("scores.html"),
        "<!doctype html><title>scores-entry</title>",
    )
    .unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = Arc::new(Engine::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://localhost:{port}");
    let runtime = Runtime::new(library, engine, base.clone());
    let scheduler = tokio::spawn(runtime.clone().schedule());
    let app = http::router(
        runtime.clone(),
        Some(web.path().to_owned()),
        Some("http://127.0.0.1:5173".into()),
        Vec::new(),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let scores = client
        .get(format!("{base}/scores?documentId=missing"))
        .send()
        .await
        .unwrap();
    assert_eq!(scores.status(), 307);
    assert_eq!(
        scores.headers()["location"],
        "http://127.0.0.1:5173/scores?documentId=missing"
    );

    runtime.stop().unwrap();
    scheduler.await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scores_route_serves_the_spa_and_old_score_route_is_absent() {
    let root = tempfile::tempdir().unwrap();
    let web = tempfile::tempdir().unwrap();
    std::fs::write(
        web.path().join("scores.html"),
        "<!doctype html><title>scores-entry</title>",
    )
    .unwrap();
    std::fs::write(
        web.path().join("index.html"),
        "<!doctype html><title>library-entry</title>",
    )
    .unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = Arc::new(Engine::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://localhost:{port}");
    let runtime = Runtime::new(library, engine, base.clone());
    let scheduler = tokio::spawn(runtime.clone().schedule());
    let app = http::router(
        runtime.clone(),
        Some(web.path().to_owned()),
        None,
        Vec::new(),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let home = client.get(&base).send().await.unwrap();
    assert_eq!(home.status(), 200);
    assert!(home.text().await.unwrap().contains("library-entry"));
    let scores = client
        .get(format!("{base}/scores?documentId=missing"))
        .send()
        .await
        .unwrap();
    assert_eq!(scores.status(), 200);
    assert!(scores.text().await.unwrap().contains("scores-entry"));
    assert_eq!(
        client
            .get(format!("{base}/score.html"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    runtime.stop().unwrap();
    scheduler.await.unwrap();
    server.abort();
}

#[test]
fn ipc_handshake_rejects_wrong_home_schema_and_protocol() {
    let f = tempfile::tempdir().unwrap();
    let home = f.path().canonicalize().unwrap();
    let home_text = path_text(&home);
    let mut info = RuntimeInfo {
        runtime_version: "test".into(),
        protocol_version: PROTOCOL_VERSION,
        schema_version: SCHEMA_VERSION,
        instance_id: "test".into(),
        home: home_text,
        web_base_url: "http://localhost:3211".into(),
        capabilities: vec![],
    };
    assert!(ipc::verify_handshake(&home, &info).is_ok());
    info.home = home.to_string_lossy().into_owned();
    assert!(ipc::verify_handshake(&home, &info).is_ok());
    info.home = path_text(&home);
    info.protocol_version = PROTOCOL_VERSION + 1;
    assert_eq!(
        ipc::verify_handshake(&home, &info).unwrap_err().code,
        ErrorCode::RuntimeIncompatible
    );
    info.protocol_version = PROTOCOL_VERSION;
    info.schema_version = SCHEMA_VERSION + 1;
    assert!(ipc::verify_handshake(&home, &info).is_err());
}
