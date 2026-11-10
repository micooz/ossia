use ossia_engine::{CompileOptions, RunContext};
use ossia_library::{EngineBackend, ImportRequest, Library};
use ossia_runtime::*;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const SOURCE: &[u8] =
    b"\\version \"2.26.0\"\n\\score { \\new Staff { c'1 | d'1 | } \\layout {} }\n";
#[derive(Default)]
struct Engine {
    hold: AtomicBool,
    active: AtomicUsize,
    peak: AtomicUsize,
    documents: Mutex<Vec<DocumentId>>,
    seen: Mutex<Vec<ContentHash>>,
}
struct Active<'a>(&'a Engine, DocumentId);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        self.0.documents.lock().unwrap().retain(|id| *id != self.1);
    }
}
impl EngineBackend for Engine {
    fn build_id(&self) -> &str {
        "runtime-test-engine"
    }
    fn convert(
        &self,
        input: &[u8],
        _: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport> {
        context.check()?;
        let source = String::from_utf8(input.to_vec())
            .map_err(|e| AppError::new(ErrorCode::InvalidEncoding, e.to_string()))?;
        Ok(ImportReport {
            status: ImportStatus::Clean,
            input_hash: ContentHash::of(input),
            engine_build_id: self.build_id().into(),
            source_hash: Some(ContentHash::of(input)),
            source: Some(source),
            confirmation: None,
            confirmed: confirmation.is_some(),
            measures: 2,
            voices: 1,
            note_events: 2,
            losses: vec![],
            blocking: vec![],
            diagnostics: vec![],
        })
    }
    fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest> {
        {
            let mut documents = self.documents.lock().unwrap();
            assert!(
                !documents.contains(&snapshot.document_id()),
                "same document compiled concurrently"
            );
            documents.push(snapshot.document_id());
        }
        let _active = Active(self, snapshot.document_id());
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        self.seen
            .lock()
            .unwrap()
            .push(snapshot.source_hash().clone());
        while self.hold.load(Ordering::Acquire) {
            context.check()?;
            std::thread::sleep(Duration::from_millis(5));
        }
        context.check()?;
        let bytes = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><!-- {} --></svg>",
            snapshot.source_hash()
        )
        .into_bytes();
        let hash = ContentHash::of(&bytes);
        let path = format!("assets/{hash}.svg");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join(&path), &bytes).unwrap();
        let broken = snapshot.source().contains("BROKEN");
        Ok(RenderManifest {
            source_hash: snapshot.source_hash().clone(),
            engine_build_id: self.build_id().into(),
            options_hash: ContentHash::of(&serde_json::to_vec(options).unwrap()),
            pages: if broken {
                vec![]
            } else {
                vec![RenderPage {
                    page: 1,
                    artifact: ArtifactRef {
                        path,
                        sha256: hash,
                        bytes: bytes.len() as u64,
                    },
                }]
            },
            pdf: None,
            midi: None,
            diagnostics: vec![],
            format_errors: if broken {
                vec![FormatError {
                    format: RenderFormat::Svg,
                    error: AppError::new(ErrorCode::CompileFailed, "syntax error"),
                }]
            } else {
                vec![]
            },
        })
    }
}
struct Fixture {
    runtime: Arc<Runtime>,
    engine: Arc<Engine>,
    doc: DocumentState,
    view: ViewId,
    root: tempfile::TempDir,
    scheduler: Option<tokio::task::JoinHandle<()>>,
}
impl Fixture {
    async fn new(schedule: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let library = Library::open(root.path()).unwrap();
        let engine = Arc::new(Engine::default());
        let doc = library
            .import(
                engine.as_ref(),
                ImportRequest {
                    title: "synthetic",
                    input: SOURCE,
                    format: InputFormat::Ly,
                    confirmation: None,
                    author: "test",
                },
                &RunContext::default(),
            )
            .unwrap()
            .document
            .unwrap();
        let runtime = Runtime::new(library, engine.clone(), "http://127.0.0.1:3211".into());
        let view = runtime.open_view(Some(doc.document_id)).unwrap().view_id;
        runtime.context(view).unwrap();
        let scheduler = schedule.then(|| tokio::spawn(runtime.clone().schedule()));
        Self {
            runtime,
            engine,
            doc,
            view,
            root,
            scheduler,
        }
    }
    fn check(&self, hash: ContentHash) -> Job {
        self.runtime
            .check(
                CheckArgs {
                    document_id: self.doc.document_id,
                    view_id: self.view,
                    edit_id: None,
                    expected_source_hash: hash,
                },
                false,
            )
            .unwrap()
    }
    fn save(&self, source: &[u8]) -> ContentHash {
        std::fs::write(&self.doc.source_path, source).unwrap();
        ContentHash::of(source)
    }
    fn begin(&self) -> EditSession {
        serde_json::from_value(
            self.runtime
                .edit(
                    EditArgs {
                        action: EditAction::Begin,
                        document_id: self.doc.document_id,
                        view_id: self.view,
                        base_version_id: self.doc.head_version_id,
                        expected_source_hash: self.doc.source_hash.clone(),
                        edit_id: None,
                        adopt_workspace: false,
                        author: "test-agent".into(),
                    },
                    true,
                )
                .unwrap(),
        )
        .unwrap()
    }
    async fn finish(mut self) {
        self.runtime.stop().unwrap();
        if let Some(scheduler) = self.scheduler.take() {
            tokio::time::timeout(Duration::from_secs(5), scheduler)
                .await
                .unwrap()
                .unwrap();
        }
    }
}
async fn wait_job(runtime: &Runtime, id: JobId) -> Job {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let job = runtime.library.get_job(id).unwrap();
        if !matches!(job.state, JobState::Queued | JobState::Running) {
            return job;
        }
        assert!(Instant::now() < deadline, "job stuck: {job:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "condition timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_snapshots_previews_and_explicit_checks_are_distinct() {
    let mut f = Fixture::new(false).await;
    let old = f.save(b"\\score { { e'1 } }");
    let explicit = f.check(old.clone());
    let preview = f
        .runtime
        .check(
            CheckArgs {
                document_id: f.doc.document_id,
                view_id: f.view,
                edit_id: None,
                expected_source_hash: old.clone(),
            },
            true,
        )
        .unwrap();
    let latest = f.save(b"\\score { { f'1 } }");
    let next = f
        .runtime
        .check(
            CheckArgs {
                document_id: f.doc.document_id,
                view_id: f.view,
                edit_id: None,
                expected_source_hash: latest.clone(),
            },
            true,
        )
        .unwrap();
    assert_eq!(
        f.runtime.library.get_job(preview.job_id).unwrap().state,
        JobState::Cancelled
    );
    assert_eq!(
        f.runtime.library.get_job(explicit.job_id).unwrap().state,
        JobState::Queued
    );
    f.scheduler = Some(tokio::spawn(f.runtime.clone().schedule()));
    assert_eq!(
        wait_job(&f.runtime, explicit.job_id).await.state,
        JobState::Done
    );
    assert_eq!(
        wait_job(&f.runtime, next.job_id).await.state,
        JobState::Done
    );
    assert!(f.engine.seen.lock().unwrap().contains(&old));
    assert_eq!(
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .unwrap()
            .source_hash,
        latest
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn global_limit_same_document_serialization_and_cancel_cleanup() {
    let f = Fixture::new(true).await;
    let mut docs = vec![f.doc.clone()];
    for i in 0..2 {
        docs.push(
            f.runtime
                .library
                .import(
                    f.engine.as_ref(),
                    ImportRequest {
                        title: &format!("doc{i}"),
                        input: SOURCE,
                        format: InputFormat::Ly,
                        confirmation: None,
                        author: "test",
                    },
                    &RunContext::default(),
                )
                .unwrap()
                .document
                .unwrap(),
        );
    }
    f.engine.peak.store(0, Ordering::SeqCst);
    f.engine.hold.store(true, Ordering::Release);
    let first = f.check(f.doc.source_hash.clone());
    let second = f.check(f.doc.source_hash.clone());
    let mut jobs = vec![first.clone(), second.clone()];
    for doc in docs.iter().skip(1) {
        let view = f.runtime.open_view(Some(doc.document_id)).unwrap().view_id;
        jobs.push(
            f.runtime
                .check(
                    CheckArgs {
                        document_id: doc.document_id,
                        view_id: view,
                        edit_id: None,
                        expected_source_hash: doc.source_hash.clone(),
                    },
                    false,
                )
                .unwrap(),
        );
    }
    until(|| f.engine.active.load(Ordering::SeqCst) == 2).await;
    assert_eq!(
        f.runtime.library.get_job(second.job_id).unwrap().state,
        JobState::Queued
    );
    assert_eq!(f.engine.peak.load(Ordering::SeqCst), 2);
    f.runtime.cancel(first.job_id).unwrap();
    f.runtime.cancel(first.job_id).unwrap();
    assert_eq!(
        wait_job(&f.runtime, first.job_id).await.state,
        JobState::Cancelled
    );
    f.engine.hold.store(false, Ordering::Release);
    for job in jobs.into_iter().skip(1) {
        assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    }
    until(|| f.engine.active.load(Ordering::SeqCst) == 0).await;
    assert_eq!(
        f.runtime.library.get_job(first.job_id).unwrap().state,
        JobState::Cancelled
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn view_ownership_stale_selections_and_explicit_commit() {
    let f = Fixture::new(true).await;
    let other = f
        .runtime
        .open_view(Some(f.doc.document_id))
        .unwrap()
        .view_id;
    let selection = Selection {
        view_id: f.view,
        document_id: f.doc.document_id,
        source_hash: f.doc.source_hash.clone(),
        locators: vec![Locator {
            staff_index: 0,
            measure_start: 1,
            measure_end: 1,
            voice: None,
        }],
    };
    f.runtime.selection(selection.clone()).unwrap();
    assert!(f.runtime.context(other).unwrap().selection.is_none());
    let edit = f.begin();
    let changed = f.save(b"\\score { \\new Staff { e'1 | d'1 | } \\layout {} }");
    assert!(f.runtime.context(f.view).unwrap().selection.is_none());
    assert_eq!(
        f.runtime.selection(selection).unwrap_err().code,
        ErrorCode::StaleSelection
    );
    let args = CheckArgs {
        document_id: f.doc.document_id,
        view_id: other,
        edit_id: Some(edit.edit_id),
        expected_source_hash: changed.clone(),
    };
    assert_eq!(
        f.runtime.check(args, false).unwrap_err().code,
        ErrorCode::Forbidden
    );
    let job = f
        .runtime
        .check(
            CheckArgs {
                document_id: f.doc.document_id,
                view_id: f.view,
                edit_id: Some(edit.edit_id),
                expected_source_hash: changed.clone(),
            },
            false,
        )
        .unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    let version = f
        .runtime
        .commit(CommitArgs {
            document_id: f.doc.document_id,
            view_id: f.view,
            edit_id: edit.edit_id,
            base_version_id: f.doc.head_version_id,
            expected_source_hash: changed,
            summary: "raise note".into(),
        })
        .unwrap();
    assert_eq!(version.seq, 2);
    assert!(!f.runtime.context(f.view).unwrap().document.unwrap().dirty);
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_watcher_events_are_reconciled_and_failed_checks_keep_old_render() {
    let f = Fixture::new(true).await;
    let broken = f.save(b"BROKEN");
    let context = f.runtime.context(f.view).unwrap();
    assert_eq!(context.document.unwrap().source_hash, broken);
    let job = f.check(broken);
    assert_eq!(
        wait_job(&f.runtime, job.job_id).await.state,
        JobState::Error
    );
    assert_eq!(
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .unwrap()
            .source_hash,
        f.doc.source_hash
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watcher_debounces_atomic_saves_and_recovers_deleted_file() {
    let f = Fixture::new(true).await;
    let watcher = tokio::spawn(f.runtime.clone().watch());
    tokio::time::sleep(Duration::from_millis(100)).await;
    if watcher.is_finished() {
        panic!("watcher exited: {:?}", watcher.await);
    }
    for i in 0..4 {
        let path = Path::new(&f.doc.source_path).with_file_name("save.tmp");
        std::fs::write(&path, format!("\\score {{ {{ c'1 }} }} % {i}")).unwrap();
        std::fs::rename(path, &f.doc.source_path).unwrap();
    }
    let hash = ContentHash::of(&std::fs::read(&f.doc.source_path).unwrap());
    until(|| {
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .is_some_and(|r| r.source_hash == hash)
    })
    .await;
    std::fs::remove_file(&f.doc.source_path).unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let hash = f.save(b"\\score { { g'1 } }");
    until(|| {
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .is_some_and(|r| r.source_hash == hash)
    })
    .await;
    f.runtime.stop().unwrap();
    watcher.await.unwrap().unwrap();
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn snapshots_replay_events_and_detect_expired_instance_or_cursor() {
    let f = Fixture::new(false).await;
    let snapshot = f.runtime.context(f.view).unwrap();
    let other = f.runtime.open_view(None).unwrap().view_id;
    f.runtime
        .select_document(other, Some(f.doc.document_id))
        .unwrap();
    f.runtime.select_document(f.view, None).unwrap();
    let events = f
        .runtime
        .events(f.view, &snapshot.instance_id, snapshot.seq)
        .unwrap();
    assert!(events.iter().all(|e| e.view_id != Some(other)));
    assert!(events.iter().any(|e| e.r#type == "view-changed"));
    assert_eq!(
        f.runtime
            .events(f.view, "previous-instance", snapshot.seq)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    for _ in 0..520 {
        f.runtime.emit("tick", None, None, json!({}));
    }
    assert!(f
        .runtime
        .events(f.view, &snapshot.instance_id, snapshot.seq)
        .is_err());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn export_and_render_keys_enforce_version_ownership_and_do_not_overwrite() {
    let f = Fixture::new(true).await;
    let key = ossia_library::render_key(
        f.doc.document_id,
        f.doc.last_successful_render.as_ref().unwrap(),
    );
    assert!(!f
        .runtime
        .library
        .read_render_page(&key, 1)
        .unwrap()
        .is_empty());
    assert!(f.runtime.library.read_render_page(&key, 2).is_err());
    let target = f.root.path().join("export.ly");
    let args = ExportArgs {
        document_id: f.doc.document_id,
        version_id: f.doc.head_version_id.unwrap(),
        format: ExportFormat::Ly,
        output_path: Some(target.to_string_lossy().into_owned()),
    };
    let job = f.runtime.export(args.clone()).unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert_eq!(std::fs::read(&target).unwrap(), SOURCE);
    let job = f.runtime.export(args.clone()).unwrap();
    assert_eq!(
        wait_job(&f.runtime, job.job_id).await.error.unwrap().code,
        ErrorCode::Conflict
    );
    assert_eq!(std::fs::read(&target).unwrap(), SOURCE);
    let bad = ExportArgs {
        document_id: DocumentId::new(),
        ..args
    };
    assert_eq!(f.runtime.export(bad).unwrap_err().code, ErrorCode::NotFound);
    f.finish().await;
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn running_import_restore_export_cancellation_finishes_before_publishing() {
    let f = Fixture::new(true).await;
    f.engine.hold.store(true, Ordering::Release);
    let imported = f
        .runtime
        .import(
            ImportArgs {
                title: "cancel import".into(),
                format: InputFormat::Ly,
                path: String::new(),
                confirmation: None,
                author: "test".into(),
            },
            SOURCE.to_vec(),
        )
        .unwrap();
    until(|| f.engine.active.load(Ordering::SeqCst) == 1).await;
    f.runtime.cancel(imported.job_id).unwrap();
    assert_eq!(
        wait_job(&f.runtime, imported.job_id).await.state,
        JobState::Cancelled
    );
    until(|| f.engine.active.load(Ordering::SeqCst) == 0).await;
    assert_eq!(f.runtime.library.list_documents().unwrap().len(), 1);
    let restored = f
        .runtime
        .restore(RestoreArgs {
            document_id: f.doc.document_id,
            version_id: f.doc.head_version_id.unwrap(),
            base_version_id: f.doc.head_version_id,
            expected_source_hash: f.doc.source_hash.clone(),
            author: "test".into(),
            summary: "cancel restore".into(),
        })
        .unwrap();
    until(|| f.engine.active.load(Ordering::SeqCst) == 1).await;
    f.runtime.cancel(restored.job_id).unwrap();
    assert_eq!(
        wait_job(&f.runtime, restored.job_id).await.state,
        JobState::Cancelled
    );
    until(|| f.engine.active.load(Ordering::SeqCst) == 0).await;
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap().len(),
        1
    );
    let path = f.root.path().join("cancelled.pdf");
    let exported = f
        .runtime
        .export(ExportArgs {
            document_id: f.doc.document_id,
            version_id: f.doc.head_version_id.unwrap(),
            format: ExportFormat::Pdf,
            output_path: Some(path.to_string_lossy().into_owned()),
        })
        .unwrap();
    until(|| f.engine.active.load(Ordering::SeqCst) == 1).await;
    f.runtime.stop().unwrap();
    assert_eq!(
        wait_job(&f.runtime, exported.job_id).await.state,
        JobState::Cancelled
    );
    assert!(!path.exists());
    assert_eq!(
        f.runtime.open_view(None).unwrap_err().code,
        ErrorCode::RuntimeUnavailable
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn browser_version_renders_are_immutable_and_edit_summaries_omit_credentials() {
    let f = Fixture::new(true).await;
    let edit = f.begin();
    let snapshot = f.runtime.context(f.view).unwrap();
    let activity = snapshot.edit_activity.unwrap();
    assert_eq!(activity.author, "test-agent");
    assert!(activity.owned_by_view);
    let other = f
        .runtime
        .open_view(Some(f.doc.document_id))
        .unwrap()
        .view_id;
    let public = serde_json::to_string(&f.runtime.context(other).unwrap()).unwrap();
    assert!(!public.contains(&edit.edit_id.to_string()));
    assert!(
        !f.runtime
            .context(other)
            .unwrap()
            .edit_activity
            .unwrap()
            .owned_by_view
    );
    let changed = f.save(b"\\score { { g'1 } }");
    let before = f.runtime.context(f.view).unwrap().document.unwrap();
    let job = f
        .runtime
        .render_version(f.doc.document_id, f.doc.head_version_id.unwrap())
        .unwrap();
    assert_eq!(job.source_hash, Some(f.doc.source_hash.clone()));
    let job = wait_job(&f.runtime, job.job_id).await;
    assert_eq!(job.state, JobState::Done);
    let reference: RenderReference = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(reference.manifest.source_hash, f.doc.source_hash);
    let geometry = f.runtime.geometry(&reference.render_key).unwrap();
    assert_eq!(geometry.source_hash, f.doc.source_hash);
    assert!(geometry.pages.iter().all(|p| p.regions.is_empty()));
    let after = f.runtime.context(f.view).unwrap().document.unwrap();
    assert_eq!(after, before);
    assert_eq!(after.source_hash, changed);
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap().len(),
        1
    );
    assert!(f
        .runtime
        .render_version(DocumentId::new(), f.doc.head_version_id.unwrap())
        .is_err());
    assert!(f.runtime.geometry(&ContentHash::of(b"missing")).is_err());
    assert!(f.runtime.export_location(job.job_id).is_err());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reveal_only_resolves_intact_completed_export_results() {
    let f = Fixture::new(true).await;
    let target = f.root.path().join("result.ly");
    let job = f
        .runtime
        .export(ExportArgs {
            document_id: f.doc.document_id,
            version_id: f.doc.head_version_id.unwrap(),
            format: ExportFormat::Ly,
            output_path: Some(target.to_string_lossy().into_owned()),
        })
        .unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert_eq!(f.runtime.export_location(job.job_id).unwrap(), target);
    std::fs::write(&target, b"changed").unwrap();
    assert_eq!(
        f.runtime.export_location(job.job_id).unwrap_err().code,
        ErrorCode::StorageError
    );
    assert!(f.runtime.export_location(JobId::new()).is_err());
    f.finish().await;
}
