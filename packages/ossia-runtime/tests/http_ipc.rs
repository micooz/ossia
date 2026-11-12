mod common;

use common::{wait_job, Engine, SOURCE};
use ossia_library::Library;
use ossia_runtime::*;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

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
    let app = http::router(runtime.clone(), None, Some("http://127.0.0.1:5173".into()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let api = format!("{base}/api/v1");
    for origin in [&base, &ipv4] {
        let landing = client.get(origin).send().await.unwrap();
        assert_eq!(landing.status(), 200);
        assert!(landing.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html"));
        let landing = landing.text().await.unwrap();
        assert!(landing.contains("ossia_open"));
        assert!(landing.contains("<svg"));
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
    assert_eq!(
        runtime.open_url(Some(document.document_id)).unwrap(),
        format!("{base}/scores?documentId={}", document.document_id)
    );
    assert_eq!(runtime.open_url(None).unwrap(), format!("{base}/"));
    let context: DocumentContext = client
        .get(format!("{api}/documents/{}/context", document.document_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(context.document.document_id, document.document_id);
    let key = ossia_library::render_key(
        document.document_id,
        document.last_successful_render.as_ref().unwrap(),
    );
    assert_eq!(
        client
            .get(format!("{api}/renders/{key}/pages/1"))
            .send()
            .await
            .unwrap()
            .status(),
        200
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
            context.instance_id, context.seq, document.document_id
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
            page: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: Some(output_path.to_string_lossy().into_owned()),
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
        .get(format!("{api}/jobs/{}/output", exported.job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(downloaded.status(), 200);
    assert!(downloaded.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/plain"));
    assert_eq!(downloaded.bytes().await.unwrap().as_ref(), changed);
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
    let restored = client
        .post(format!("{api}/documents/{}/restore", document.document_id))
        .json(&RestoreArgs {
            version_id: document.head_version_id.unwrap(),
            base_version_id: Some(version.version_id),
            expected_source_hash: hash,
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
async fn scores_route_serves_the_spa_and_old_score_route_is_absent() {
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
    let app = http::router(runtime.clone(), Some(web.path().to_owned()), None);
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
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
    let mut info = RuntimeInfo {
        runtime_version: "test".into(),
        protocol_version: PROTOCOL_VERSION,
        schema_version: SCHEMA_VERSION,
        instance_id: "test".into(),
        home: home.to_string_lossy().into_owned(),
        web_base_url: "http://localhost:3211".into(),
        capabilities: vec![],
    };
    assert!(ipc::verify_handshake(&home, &info).is_ok());
    info.protocol_version = PROTOCOL_VERSION + 1;
    assert_eq!(
        ipc::verify_handshake(&home, &info).unwrap_err().code,
        ErrorCode::RuntimeIncompatible
    );
    info.protocol_version = PROTOCOL_VERSION;
    info.schema_version = SCHEMA_VERSION + 1;
    assert!(ipc::verify_handshake(&home, &info).is_err());
}
