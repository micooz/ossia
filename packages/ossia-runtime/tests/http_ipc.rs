mod common;

use common::{wait_job, Engine, SOURCE};
use ossia_library::Library;
use ossia_runtime::*;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_auth_upload_routes_events_and_cross_view_rejection() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = Arc::new(Engine::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let runtime = Runtime::new(library, engine, base.clone());
    let scheduler = tokio::spawn(runtime.clone().schedule());
    let app = http::router(runtime.clone(), None, Some("http://127.0.0.1:5173".into()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let api = format!("{base}/api/v1");
    let landing = client.get(&base).send().await.unwrap();
    assert_eq!(landing.status(), 200);
    assert!(landing.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/html"));
    let landing = landing.text().await.unwrap();
    assert!(landing.contains("ossia_open"));
    assert!(landing.contains("ossia open"));
    assert!(landing.contains("<svg"));
    let health: Value = client
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health, json!({"alive":true,"protocolVersion":1}));
    assert_eq!(
        client
            .get(format!("{base}/health"))
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
        401
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
    let opened = runtime.open_view(None).unwrap();
    let response = client
        .post(format!("{api}/sessions/exchange"))
        .json(&ExchangeRequest {
            ticket: opened.ticket.clone(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let session: ExchangeResponse = response.json().await.unwrap();
    assert_eq!(
        client
            .post(format!("{api}/sessions/exchange"))
            .json(&ExchangeRequest {
                ticket: opened.ticket
            })
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let other = runtime.open_view(None).unwrap();
    assert_eq!(
        client
            .get(format!("{api}/views/{}", other.view_id))
            .bearer_auth(&session.token)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let response = client
        .get(format!("{api}/status"))
        .header("Origin", "http://127.0.0.1:5173")
        .bearer_auth(&session.token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://127.0.0.1:5173"
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
        .bearer_auth(&session.token)
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
    let response = client
        .put(format!("{api}/views/{}/document", session.view.view_id))
        .bearer_auth(&session.token)
        .json(&OpenArgs {
            document_id: Some(document.document_id),
        })
        .send()
        .await
        .unwrap();
    let snapshot: ViewSnapshot = response.json().await.unwrap();
    assert_eq!(snapshot.document.unwrap().document_id, document.document_id);
    let key = ossia_library::render_key(
        document.document_id,
        document.last_successful_render.as_ref().unwrap(),
    );
    assert_eq!(
        client
            .get(format!("{api}/renders/{key}/pages/1"))
            .bearer_auth(&session.token)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let response = client
        .post(format!("{api}/views/{}/locate", session.view.view_id))
        .bearer_auth(&session.token)
        .json(&LocateArgs {
            source_hash: document.source_hash.clone(),
            page: 1,
            x: -1.0,
            y: -1.0,
        })
        .send()
        .await
        .unwrap();
    let located: LocateOutcome = response.json().await.unwrap();
    assert_eq!(located.capability, LocationCapability::None);
    let mut stream = client
        .get(format!(
            "{api}/events?instanceId={}&after={}",
            snapshot.instance_id, snapshot.seq
        ))
        .bearer_auth(&session.token)
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
                .bearer_auth(&session.token)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    let mut edit: EditSession=client.post(format!("{api}/documents/{}/edits",document.document_id)).bearer_auth(&session.token).json(&json!({"action":"begin","documentId":document.document_id,"viewId":session.view.view_id,"baseVersionId":document.head_version_id,"expectedSourceHash":document.source_hash})).send().await.unwrap().json().await.unwrap();
    let status = client
        .get(format!("{api}/status"))
        .bearer_auth(&session.token)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !status.contains(&edit.edit_id.to_string()),
        "status must not enumerate edit capabilities"
    );
    let changed = b"\\score { \\new Staff { e'1 | d'1 | } \\layout {} }";
    std::fs::write(&document.source_path, changed).unwrap();
    let hash = ContentHash::of(changed);
    let checked = client
        .post(format!("{api}/documents/{}/check", document.document_id))
        .bearer_auth(&session.token)
        .json(&CheckArgs {
            document_id: document.document_id,
            view_id: session.view.view_id,
            edit_id: Some(edit.edit_id),
            expected_source_hash: hash.clone(),
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
        .bearer_auth(&session.token)
        .json(&CommitArgs {
            document_id: document.document_id,
            view_id: session.view.view_id,
            edit_id: edit.edit_id,
            base_version_id: edit.base_version_id,
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
    let exported = client
        .post(format!("{api}/documents/{}/export", document.document_id))
        .bearer_auth(&session.token)
        .json(&ExportArgs {
            document_id: document.document_id,
            version_id: version.version_id,
            format: ExportFormat::Ly,
            output_path: Some(
                root.path()
                    .join("http-export.ly")
                    .to_string_lossy()
                    .into_owned(),
            ),
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
    let restored = client
        .post(format!("{api}/documents/{}/restore", document.document_id))
        .bearer_auth(&session.token)
        .json(&RestoreArgs {
            document_id: document.document_id,
            version_id: document.head_version_id.unwrap(),
            base_version_id: Some(version.version_id),
            expected_source_hash: hash,
            author: "http".into(),
            summary: "restore".into(),
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
    edit=client.post(format!("{api}/documents/{}/edits",document.document_id)).bearer_auth(&session.token).json(&json!({"action":"begin","documentId":document.document_id,"viewId":session.view.view_id,"baseVersionId":current.head_version_id,"expectedSourceHash":current.source_hash})).send().await.unwrap().json().await.unwrap();
    let ended=client.post(format!("{api}/documents/{}/edits",document.document_id)).bearer_auth(&session.token).json(&json!({"action":"end","documentId":document.document_id,"viewId":session.view.view_id,"editId":edit.edit_id,"baseVersionId":current.head_version_id,"expectedSourceHash":current.source_hash})).send().await.unwrap();
    assert_eq!(ended.status(), 200);
    let cancelled = client
        .post(format!("{api}/jobs/{}/cancel", checked.job_id))
        .bearer_auth(&session.token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(cancelled.status(), 200);
    let cancelled: Job = cancelled.json().await.unwrap();
    assert_eq!(cancelled.state, JobState::Done);
    let malformed = reqwest::multipart::Form::new()
        .text("metadata", "[]")
        .part("file", reqwest::multipart::Part::bytes(SOURCE.to_vec()));
    assert_eq!(
        client
            .post(format!("{api}/documents"))
            .bearer_auth(&session.token)
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

#[test]
fn ipc_handshake_rejects_wrong_home_schema_and_protocol() {
    let f = tempfile::tempdir().unwrap();
    let home = f.path().canonicalize().unwrap();
    let mut info = RuntimeInfo {
        runtime_version: "test".into(),
        protocol_version: 1,
        schema_version: 1,
        instance_id: "test".into(),
        home: home.to_string_lossy().into_owned(),
        web_base_url: "http://127.0.0.1:3211".into(),
        capabilities: vec![],
    };
    assert!(ipc::verify_handshake(&home, &info).is_ok());
    info.protocol_version = 2;
    assert_eq!(
        ipc::verify_handshake(&home, &info).unwrap_err().code,
        ErrorCode::RuntimeIncompatible
    );
    info.protocol_version = 1;
    info.schema_version = 2;
    assert!(ipc::verify_handshake(&home, &info).is_err());
}
