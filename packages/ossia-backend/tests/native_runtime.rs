use ossia_backend::*;
use rmcp::{
    model::{CallToolRequestParams, ClientConfig},
    service::{RoleClient, RunningService},
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

type Client = RunningService<RoleClient, ClientConfig>;
fn command(executable: &Path, home: &Path, engine: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(executable);
    command
        .args(["--home"])
        .arg(home)
        .args(["--port", "0", "--engine-root"])
        .arg(engine)
        .arg("--static-dir")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/dist"));
    command
}
async fn call(client: &Client, name: &'static str, args: Value) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(name)
                .with_arguments(args.as_object().cloned().unwrap_or_default()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{name}: {:?}", result.content);
    serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
}
async fn job(client: &Client, id: JobId) -> Job {
    let start = Instant::now();
    loop {
        let outcome: JobOutcome = serde_json::from_value(
            call(
                client,
                "ossia_job",
                json!({"jobId":id,"action":"get","detail":"full"}),
            )
            .await,
        )
        .unwrap();
        let job = outcome.job;
        if !matches!(job.state, JobState::Queued | JobState::Running) {
            assert_eq!(job.state, JobState::Done, "{job:?}");
            return job;
        }
        assert!(start.elapsed() < Duration::from_secs(60));
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}
async fn cli(
    executable: &Path,
    home: &Path,
    engine: &Path,
    name: &str,
    args: Option<Value>,
) -> (i32, Value) {
    let mut command = command(executable, home, engine);
    command.arg(name);
    if let Some(args) = args {
        command.arg(args.to_string());
    }
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("CLI output timed out")
        .unwrap();
    if !output.status.success() {
        eprintln!(
            "CLI {name} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    (
        output.status.code().unwrap_or(-1),
        serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
            panic!(
                "invalid CLI stdout: {}",
                String::from_utf8_lossy(&output.stdout)
            )
        }),
    )
}
struct Cleanup {
    endpoint: String,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let endpoint = self.endpoint.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = ipc::call(&endpoint, "stop", json!({})).await;
            });
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires OSSIA_TEST_EXECUTABLE (built with pnpm build:backend) and OSSIA_TEST_LILYPOND_ROOT"]
async fn real_mcp_cli_http_native_loop_and_runtime_lifecycle() {
    let executable = PathBuf::from(
        std::env::var_os("OSSIA_TEST_EXECUTABLE").expect("set OSSIA_TEST_EXECUTABLE"),
    );
    let engine = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    assert!(executable.is_file());
    assert!(engine.is_dir());
    let work = tempfile::tempdir().unwrap();
    let home = work.path().join("中文 library with spaces");
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/inputs");
    // Both clients may launch serve, but only the library lock winner may publish discovery.
    let (a, b) = tokio::join!(
        cli(&executable, &home, &engine, "status", None),
        cli(&executable, &home, &engine, "status", None)
    );
    assert_eq!(a.0, 0, "{:?}", a.1);
    assert_eq!(b.0, 0, "{:?}", b.1);
    assert_eq!(a.1["runtime"]["webBaseUrl"], b.1["runtime"]["webBaseUrl"]);
    let discovery = ipc::connect(&home.canonicalize().unwrap()).await.unwrap();
    let _cleanup = Cleanup {
        endpoint: discovery.endpoint.clone(),
    };
    assert_eq!(discovery.runtime.protocol_version, PROTOCOL_VERSION);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&discovery.endpoint)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(Path::new(&discovery.endpoint).parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    assert_eq!(
        ossia_library::Library::open(&home).err().unwrap().code,
        ErrorCode::LibraryBusy
    );
    let published_cli = ossia_integrations::launcher::cli_path(&home);
    let (exit, status) = cli(&published_cli, &home, &engine, "status", None).await;
    assert_eq!(exit, 0);
    assert_eq!(
        status["runtime"]["webBaseUrl"],
        json!(discovery.runtime.web_base_url)
    );
    assert!(status.get("instanceId").is_none());
    let doctor = ipc::call(&discovery.endpoint, "doctor", json!({}))
        .await
        .unwrap();
    let (exit, guide) = cli(&published_cli, &home, &engine, "guide", None).await;
    assert_eq!(exit, 0);
    let guide_path = home.canonicalize().unwrap().join("docs/guide.md");
    assert_eq!(guide["path"], json!(path_text(&guide_path)));
    assert_eq!(guide["bytes"], json!(GUIDE.len()));
    assert_eq!(guide["lines"], json!(GUIDE.lines().count()));
    assert_eq!(std::fs::read_to_string(&guide_path).unwrap(), GUIDE);
    for name in [
        "editing.md",
        "import.md",
        "evidence.md",
        "library.md",
        "output.md",
        "operations.md",
    ] {
        let child = format!("guide/{name}");
        assert!(GUIDE.contains(&format!("]({child})")));
        assert!(
            !std::fs::read_to_string(guide_path.parent().unwrap().join(child))
                .unwrap()
                .is_empty()
        );
    }
    let mut cmd = command(&executable, &home, &engine);
    cmd.arg("mcp");
    let client = ClientConfig::default()
        .serve(TokioChildProcess::new(cmd).unwrap())
        .await
        .unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), mcp::tools().len());
    assert!(tools
        .iter()
        .all(|t| t.input_schema.get("type").and_then(Value::as_str) == Some("object")));
    let mcp_guide = call(&client, "ossia_guide", json!({})).await;
    assert_eq!(mcp_guide, guide);
    let web = reqwest::Client::new();
    let api = format!("{}/api/v1", discovery.runtime.web_base_url);
    let mut cases = vec![];
    for (file, format) in [
        ("inline-staff.ly", InputFormat::Ly),
        ("xml-clean.musicxml", InputFormat::MusicXml),
        ("xml-clean.mxl", InputFormat::Mxl),
        ("midi-valid.mid", InputFormat::Midi),
    ] {
        let job_ref:Job=serde_json::from_value(call(&client,"ossia_import",json!({"path":fixtures.join(file).canonicalize().unwrap(),"title":file,"format":format,"author":"native-agent"})).await).unwrap();
        let imported: ImportOutcome =
            serde_json::from_value(job(&client, job_ref.job_id).await.result.unwrap()).unwrap();
        let doc = imported.document.unwrap();
        let initial = doc.head_version_id.unwrap();
        let context = call(
            &client,
            "ossia_context",
            json!({"documentId":doc.document_id}),
        )
        .await;
        assert!(context["document"]["url"]
            .as_str()
            .unwrap()
            .contains(&format!("/scores?documentId={}", doc.document_id)));
        let edit:EditSession=serde_json::from_value(call(&client,"ossia_edit",json!({"action":"begin","documentId":doc.document_id,"baseVersionId":initial,"expectedSourceHash":doc.source_hash,"author":"native-agent"})).await).unwrap();
        let original = std::fs::read(&doc.source_path).unwrap();
        let mut changed = String::from_utf8(original.clone()).unwrap();
        if format == InputFormat::Ly {
            assert!(changed.contains("c'1"));
            changed = changed.replacen("c'1", "e'1", 1);
        } else {
            changed.push_str("\n% Native runtime edit 中文\n");
        }
        let replacement = Path::new(&doc.source_path).with_file_name("save.tmp");
        std::fs::write(&replacement, changed.as_bytes()).unwrap();
        std::fs::rename(replacement, &doc.source_path).unwrap();
        let hash = ContentHash::of(changed.as_bytes());
        let preview_start = Instant::now();
        loop {
            let context: DocumentContext = web
                .get(format!("{api}/documents/{}/context", doc.document_id))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if context
                .document
                .last_successful_render
                .is_some_and(|render| render.source_hash == hash)
            {
                break;
            }
            assert!(
                preview_start.elapsed() < Duration::from_secs(10),
                "automatic native preview did not update"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let job_ref: Job = serde_json::from_value(
            call(
                &client,
                "ossia_check",
                json!({"editId":edit.edit_id,"expectedSourceHash":hash}),
            )
            .await,
        )
        .unwrap();
        let checked = job(&client, job_ref.job_id).await;
        let result: CheckResult = serde_json::from_value(checked.result.unwrap()).unwrap();
        let render = result.render;
        assert_eq!(render.source_hash, hash);
        assert!(!render.pages.is_empty());
        let version:Version=serde_json::from_value(call(&client,"ossia_commit",json!({"editId":edit.edit_id,"expectedSourceHash":hash,"summary":"native runtime edit"})).await).unwrap();
        assert_eq!(version.seq, 2);
        let context: DocumentContext = web
            .get(format!("{api}/documents/{}/context", doc.document_id))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let key = context.render.unwrap().render_key;
        let response = web
            .get(format!("{api}/renders/{key}/pages/1"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.text().await.unwrap().contains("<svg"));
        for (format, extension) in [
            (OutputFormat::Ly, "ly"),
            (OutputFormat::Pdf, "pdf"),
            (OutputFormat::Midi, "mid"),
            (OutputFormat::Wav, "wav"),
        ] {
            let path = work
                .path()
                .join(format!("{}-export.{extension}", doc.document_id));
            let queued:Job=serde_json::from_value(call(&client,"ossia_output",json!({"source":{"kind":"version","versionId":version.version_id},"format":format,"outputPath":path})).await).unwrap();
            let exported: OutputOutcome =
                serde_json::from_value(job(&client, queued.job_id).await.result.unwrap()).unwrap();
            let bytes = std::fs::read(&exported.path).unwrap();
            assert!(!bytes.is_empty());
            assert_eq!(ContentHash::of(&bytes), exported.sha256);
            if format == OutputFormat::Wav {
                let response = web
                    .get(format!(
                        "{api}/jobs/{}/output?filename=export.wav",
                        queued.job_id
                    ))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
                assert!(response.headers()["content-type"]
                    .to_str()
                    .unwrap()
                    .starts_with("audio/wav"));
                assert!(response.bytes().await.unwrap().starts_with(b"RIFF"));
            }
            match format {
                OutputFormat::Ly => assert_eq!(bytes, changed.as_bytes()),
                OutputFormat::Pdf => assert!(bytes.starts_with(b"%PDF-")),
                OutputFormat::Midi => {
                    ossia_score::parse_midi(&bytes).unwrap();
                }
                OutputFormat::Wav => {
                    assert!(bytes.starts_with(b"RIFF"));
                    assert_eq!(&bytes[8..12], b"WAVE");
                    assert_eq!(exported.media_type, "audio/wav");
                }
                OutputFormat::Svg | OutputFormat::Png | OutputFormat::MusicXml => unreachable!(),
            }
        }
        if format == InputFormat::Ly {
            for (format, extension) in [(OutputFormat::Midi, "mid"), (OutputFormat::Wav, "wav")] {
                let path = work
                    .path()
                    .join(format!("{}-range.{extension}", doc.document_id));
                let queued:Job=serde_json::from_value(call(&client,"ossia_output",json!({"source":{"kind":"version","versionId":version.version_id},"format":format,"audioRange":{"start":1,"end":1},"outputPath":path})).await).unwrap();
                let exported: OutputOutcome =
                    serde_json::from_value(job(&client, queued.job_id).await.result.unwrap())
                        .unwrap();
                let bytes = std::fs::read(&exported.path).unwrap();
                assert!(!bytes.is_empty());
                assert_eq!(ContentHash::of(&bytes), exported.sha256);
                match format {
                    OutputFormat::Midi => {
                        let midi = ossia_score::parse_midi(&bytes).unwrap();
                        assert!(midi.tracks.iter().any(|track| !track.notes.is_empty()));
                    }
                    OutputFormat::Wav => {
                        assert!(bytes.starts_with(b"RIFF"));
                        assert_eq!(&bytes[8..12], b"WAVE");
                        assert_eq!(exported.media_type, "audio/wav");
                    }
                    _ => unreachable!(),
                }
            }
        }
        let queued:Job=serde_json::from_value(call(&client,"ossia_restore",json!({"versionId":initial,"baseVersionId":version.version_id,"expectedSourceHash":hash,"author":"native-agent","summary":"restore original"})).await).unwrap();
        let restored: Version =
            serde_json::from_value(job(&client, queued.job_id).await.result.unwrap()).unwrap();
        assert_eq!(restored.seq, 3);
        assert_eq!(std::fs::read(&doc.source_path).unwrap(), original);
        let history: VersionPage = serde_json::from_value(
            call(
                &client,
                "ossia_history",
                json!({"documentId":doc.document_id}),
            )
            .await,
        )
        .unwrap();
        assert_eq!(history.items.len(), 3);
        let (code, history_cli) = cli(
            &executable,
            &home,
            &engine,
            "history",
            Some(json!({"documentId":doc.document_id})),
        )
        .await;
        assert_eq!(code, 0);
        assert_eq!(history_cli["items"].as_array().unwrap().len(), 3);
        cases.push(json!({"input":file,"versions":3,"exports":["ly","pdf","midi"],"svgPages":render.pages.len()}));
    }
    client.cancel().await.unwrap();
    assert!(
        ipc::connect(&home.canonicalize().unwrap()).await.is_ok(),
        "MCP disconnect must not stop serve"
    );
    let (code, error) = cli(
        &executable,
        &home,
        &engine,
        "context",
        Some(json!({"documentId":DocumentId::new()})),
    )
    .await;
    assert_ne!(code, 0);
    assert_eq!(error["error"]["code"], "not-found");
    let endpoint = discovery.endpoint.clone();
    let stopped = cli(&executable, &home, &engine, "stop", None).await;
    assert_eq!(stopped.0, 0);
    let start = Instant::now();
    while ipc::connect(&home.canonicalize().unwrap()).await.is_ok() {
        assert!(start.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let child_guide = guide_path.parent().unwrap().join("guide/editing.md");
    std::fs::write(&child_guide, "stale").unwrap();
    let restart = cli(&executable, &home, &engine, "status", None).await;
    assert_eq!(restart.0, 0, "{:?}", restart.1);
    assert_ne!(std::fs::read_to_string(child_guide).unwrap(), "stale");
    let (exit, restarted_guide) = cli(&published_cli, &home, &engine, "guide", None).await;
    assert_eq!(exit, 0);
    assert_eq!(restarted_guide, guide);
    let mut restarted_mcp = command(&executable, &home, &engine);
    restarted_mcp.arg("mcp");
    let restarted_client = ClientConfig::default()
        .serve(TokioChildProcess::new(restarted_mcp).unwrap())
        .await
        .unwrap();
    assert_eq!(
        call(&restarted_client, "ossia_guide", json!({})).await,
        guide
    );
    restarted_client.cancel().await.unwrap();
    let discovery2 = ipc::connect(&home.canonicalize().unwrap()).await.unwrap();
    assert_ne!(
        discovery2.runtime.instance_id,
        discovery.runtime.instance_id
    );
    let response = web
        .get(format!("{}/health", discovery2.runtime.web_base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    ipc::call(&discovery2.endpoint, "stop", json!({}))
        .await
        .unwrap();
    let _ = endpoint;
    if let Some(path) = std::env::var_os("OSSIA_NATIVE_EVIDENCE") {
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"protocolVersion":PROTOCOL_VERSION,"tools":mcp::tools().len(),"concurrentStartup":true,"mcpDisconnectKeepsServe":true,"restart":true,"automaticPreview":true,"engineBuildId":doctor["engine"]["buildId"],"cases":cases})).unwrap()).unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires OSSIA_TEST_EXECUTABLE and OSSIA_TEST_LILYPOND_ROOT"]
async fn native_musicxml_cli_mcp_http() {
    let executable = PathBuf::from(std::env::var_os("OSSIA_TEST_EXECUTABLE").unwrap());
    let engine = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let home = work.path().join("MusicXML library 中文");
    let mut mcp = command(&executable, &home, &engine);
    mcp.arg("mcp");
    let client = ClientConfig::default()
        .serve(TokioChildProcess::new(mcp).unwrap())
        .await
        .unwrap();

    let input = work.path().join("seed.ly");
    let source = r#"\version "2.26.0"
\score { \new Staff { c'1 } \layout {} }"#;
    std::fs::write(&input, source).unwrap();
    let imported: Job = serde_json::from_value(
        call(
            &client,
            "ossia_import",
            json!({
                "path": input, "title": "Versioned MusicXML"
            }),
        )
        .await,
    )
    .unwrap();
    let imported: ImportOutcome =
        serde_json::from_value(job(&client, imported.job_id).await.result.unwrap()).unwrap();
    let doc = imported.document.unwrap();
    let initial = doc.head_version_id.unwrap();
    let discovery = ipc::connect(&home.canonicalize().unwrap()).await.unwrap();
    let _cleanup = Cleanup {
        endpoint: discovery.endpoint.clone(),
    };
    let edit: EditSession = serde_json::from_value(
        call(
            &client,
            "ossia_edit",
            json!({
                "action": "begin", "documentId": doc.document_id, "baseVersionId": initial,
                "expectedSourceHash": doc.source_hash, "author": "native-agent"
            }),
        )
        .await,
    )
    .unwrap();
    let changed = source.replace("c'1", "e'1");
    std::fs::write(&doc.source_path, &changed).unwrap();
    let hash = ContentHash::of(changed.as_bytes());
    let checked: Job = serde_json::from_value(
        call(
            &client,
            "ossia_check",
            json!({
                "editId": edit.edit_id, "expectedSourceHash": hash
            }),
        )
        .await,
    )
    .unwrap();
    job(&client, checked.job_id).await;

    let drafted: Job = serde_json::from_value(
        call(
            &client,
            "ossia_output",
            json!({
                "source": {"kind":"job", "jobId": checked.job_id},
                "format": "musicxml", "outputPath": work.path().join("draft.musicxml")
            }),
        )
        .await,
    )
    .unwrap();
    let drafted_id = drafted.job_id;
    let drafted: OutputOutcome =
        serde_json::from_value(job(&client, drafted_id).await.result.unwrap()).unwrap();
    assert_eq!(drafted.source_hash, hash);
    assert_eq!(drafted.media_type, "application/vnd.recordare.musicxml+xml");
    let xml = std::fs::read_to_string(&drafted.path).unwrap();
    assert!(xml.contains("<step>E</step>"));

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/jobs/{drafted_id}/output?filename=export.musicxml",
            discovery.runtime.web_base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], drafted.media_type);
    assert_eq!(response.text().await.unwrap(), xml);

    let (exit, old) = cli(
        &executable,
        &home,
        &engine,
        "output",
        Some(json!({
            "source": {"kind":"version", "versionId": initial},
            "format": "musicxml", "outputPath": work.path().join("historical.musicxml")
        })),
    )
    .await;
    assert_eq!(exit, 0, "{old:?}");
    let old: Job = serde_json::from_value(old).unwrap();
    let old: OutputOutcome =
        serde_json::from_value(job(&client, old.job_id).await.result.unwrap()).unwrap();
    assert_eq!(old.source_hash, doc.source_hash);
    let old_xml = std::fs::read_to_string(old.path).unwrap();
    assert!(old_xml.contains("<step>C</step>"));
    assert!(!old_xml.contains("<step>E</step>"));

    client.cancel().await.unwrap();
    ipc::call(&discovery.endpoint, "stop", json!({}))
        .await
        .unwrap();
}

async fn wait_runtime(home: &Path, child: &mut tokio::process::Child) -> ipc::Discovery {
    let canonical = server::canonical_home(home).unwrap();
    let start = Instant::now();
    loop {
        if let Ok(discovery) = ipc::connect(&canonical).await {
            return discovery;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "serve exited before readiness"
        );
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "serve readiness timed out"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
async fn ipc_job(endpoint: &str, id: JobId) -> Job {
    let start = Instant::now();
    loop {
        let outcome: JobOutcome = serde_json::from_value(
            ipc::call(
                endpoint,
                "job",
                json!({"jobId":id,"action":"get","detail":"full"}),
            )
            .await
            .unwrap(),
        )
        .unwrap();
        let job = outcome.job;
        if !matches!(job.state, JobState::Queued | JobState::Running) {
            return job;
        }
        assert!(start.elapsed() < Duration::from_secs(30));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires OSSIA_TEST_EXECUTABLE and OSSIA_TEST_LILYPOND_ROOT"]
async fn native_crash_stale_endpoint_edit_recovery_port_conflict() {
    let executable = PathBuf::from(
        std::env::var_os("OSSIA_TEST_EXECUTABLE").expect("set OSSIA_TEST_EXECUTABLE"),
    );
    let engine = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let output = command(
        &executable,
        &work.path().join("mcp-start-error"),
        &work.path().join("missing-engine"),
    )
    .arg("mcp")
    .output()
    .await
    .unwrap();
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "MCP startup errors must not pollute protocol stdout"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("error"));
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    let conflict = work.path().join("port-conflict");
    let mut cmd = tokio::process::Command::new(&executable);
    cmd.arg("--home")
        .arg(&conflict)
        .arg("--port")
        .arg(port.to_string())
        .arg("--engine-root")
        .arg(&engine)
        .arg("serve");
    let output = cmd.output().await.unwrap();
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["error"]["code"], "runtime-unavailable");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains(&port.to_string()));
    assert!(!conflict.join("run/runtime.json").exists());
    drop(occupied);
    let home = work.path().join("crash-recovery");
    let mut cmd = command(&executable, &home, &engine);
    cmd.arg("serve")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = cmd.spawn().unwrap();
    let old = wait_runtime(&home, &mut child).await;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/inputs/inline-staff.ly")
        .canonicalize()
        .unwrap();
    let queued: Job = serde_json::from_value(
        ipc::call(
            &old.endpoint,
            "import",
            json!({"path":fixture,"title":"crash fixture","format":"ly","author":"test"}),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let imported: ImportOutcome =
        serde_json::from_value(ipc_job(&old.endpoint, queued.job_id).await.result.unwrap())
            .unwrap();
    let doc = imported.document.unwrap();
    let context: AgentDocumentContext = serde_json::from_value(
        ipc::call(
            &old.endpoint,
            "context",
            json!({"documentId":doc.document_id}),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let web = reqwest::Client::new();
    assert!(context
        .document
        .url
        .contains(&format!("/scores?documentId={}", doc.document_id)));
    let edit:EditSession=serde_json::from_value(ipc::call(&old.endpoint,"edit",json!({"action":"begin","documentId":doc.document_id,"baseVersionId":doc.head_version_id,"expectedSourceHash":doc.source_hash})).await.unwrap()).unwrap();
    let mut bytes = std::fs::read(&doc.source_path).unwrap();
    bytes.extend_from_slice(b"\n% work survives runtime crash\n");
    std::fs::write(&doc.source_path, &bytes).unwrap();
    let hash = ContentHash::of(&bytes);
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    assert!(
        home.join("run/runtime.json").is_file(),
        "SIGKILL leaves stale discovery for lock-owner recovery"
    );
    let mut cmd = command(&executable, &home, &engine);
    cmd.arg("serve")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = cmd.spawn().unwrap();
    let new = wait_runtime(&home, &mut child).await;
    assert_ne!(old.runtime.instance_id, new.runtime.instance_id);
    assert_eq!(
        web.get(format!("{}/api/v1/status", new.runtime.web_base_url))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let context: AgentDocumentContext = serde_json::from_value(
        ipc::call(
            &new.endpoint,
            "context",
            json!({"documentId":doc.document_id}),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let current = context.document;
    assert_eq!(current.edit_state, DocumentEditState::Interrupted);
    assert!(current.dirty);
    assert_eq!(current.source_hash, hash);
    assert_eq!(std::fs::read(&doc.source_path).unwrap(), bytes);
    let resumed:EditSession=serde_json::from_value(ipc::call(&new.endpoint,"edit",json!({"action":"resume","documentId":doc.document_id,"editId":edit.edit_id,"baseVersionId":doc.head_version_id,"expectedSourceHash":hash})).await.unwrap()).unwrap();
    assert_eq!(resumed.edit_id, edit.edit_id);
    ipc::call(&new.endpoint, "stop", json!({})).await.unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap()
        .success());
    assert!(!home.join("run/runtime.json").exists());
    if let Some(path) = std::env::var_os("OSSIA_NATIVE_EVIDENCE") {
        let path = PathBuf::from(path).with_file_name("p04-lifecycle.json");
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"sigkillRecovery":true,"staleEndpointReplaced":true,"sourceBytesPreserved":true,"editCapabilityResume":true,"oldViewAndTokenInvalidated":true,"occupiedPortRejected":true,"explicitStopRemovesDiscovery":true})).unwrap()).unwrap();
    }
}
