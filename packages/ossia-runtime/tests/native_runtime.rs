use ossia_runtime::*;
use rmcp::{
    model::{CallToolRequestParam, ClientInfo},
    service::{RoleClient, RunningService},
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

type Client = RunningService<RoleClient, ClientInfo>;
fn command(executable: &Path, home: &Path, engine: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(executable);
    command
        .args(["--home"])
        .arg(home)
        .args(["--port", "0", "--engine-root"])
        .arg(engine);
    command
}
async fn call(client: &Client, name: &'static str, args: Value) -> Value {
    let result = client
        .call_tool(CallToolRequestParam {
            name: name.into(),
            arguments: args.as_object().cloned(),
        })
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{name}: {:?}", result.content);
    serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
}
async fn job(client: &Client, id: JobId) -> Job {
    let start = Instant::now();
    loop {
        let outcome: JobOutcome = serde_json::from_value(
            call(client, "ossia_job", json!({"jobId":id,"action":"get"})).await,
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
#[ignore = "requires OSSIA_TEST_EXECUTABLE (built headless desktop) and OSSIA_TEST_LILYPOND_ROOT"]
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
    assert_eq!(a.1["runtime"]["instanceId"], b.1["runtime"]["instanceId"]);
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
    assert_eq!(status["instanceId"], json!(discovery.runtime.instance_id));
    let (exit, guide) = cli(&published_cli, &home, &engine, "guide", None).await;
    assert_eq!(exit, 0);
    assert_eq!(guide.as_str(), Some(GUIDE));
    let mut cmd = command(&executable, &home, &engine);
    cmd.arg("mcp");
    let client = ClientInfo::default()
        .serve(TokioChildProcess::new(cmd).unwrap())
        .await
        .unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), mcp::tools().len());
    assert!(tools
        .iter()
        .all(|t| t.input_schema.get("type").and_then(Value::as_str) == Some("object")));
    assert!(call(&client, "ossia_guide", json!({}))
        .await
        .as_str()
        .unwrap()
        .contains("expectedSourceHash"));
    let web = reqwest::Client::new();
    let api = format!("{}/api/v1", discovery.runtime.web_base_url);
    let mut cases = vec![];
    for (file, format) in [
        ("inline-staff.ly", InputFormat::Ly),
        ("xml-clean.musicxml", InputFormat::MusicXml),
        ("midi-valid.mid", InputFormat::Midi),
    ] {
        let job_ref:Job=serde_json::from_value(call(&client,"ossia_import",json!({"path":fixtures.join(file).canonicalize().unwrap(),"title":file,"format":format,"author":"native-agent"})).await).unwrap();
        let imported: ImportOutcome =
            serde_json::from_value(job(&client, job_ref.job_id).await.result.unwrap()).unwrap();
        let doc = imported.document.unwrap();
        let initial = doc.head_version_id.unwrap();
        let open: AgentOpenOutcome = serde_json::from_value(
            call(&client, "ossia_open", json!({"documentId":doc.document_id})).await,
        )
        .unwrap();
        assert!(open.url.contains("/score.html#ticket="));
        let ticket = open.url.split("#ticket=").nth(1).unwrap();
        let response = web
            .post(format!("{api}/sessions/exchange"))
            .json(&ExchangeRequest {
                ticket: ticket.into(),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let session: ExchangeResponse = response.json().await.unwrap();
        let edit:EditSession=serde_json::from_value(call(&client,"ossia_edit",json!({"action":"begin","documentId":doc.document_id,"viewId":open.view_id,"baseVersionId":initial,"expectedSourceHash":doc.source_hash,"author":"native-agent"})).await).unwrap();
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
            let context: ViewSnapshot = web
                .get(format!("{api}/views/{}", open.view_id))
                .bearer_auth(&session.token)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if context
                .document
                .unwrap()
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
                json!({"viewId":open.view_id,"editId":edit.edit_id,"expectedSourceHash":hash}),
            )
            .await,
        )
        .unwrap();
        let checked = job(&client, job_ref.job_id).await;
        let result: CheckResult = serde_json::from_value(checked.result.unwrap()).unwrap();
        let render = result.render;
        assert_eq!(render.source_hash, hash);
        assert!(!render.pages.is_empty());
        let version:Version=serde_json::from_value(call(&client,"ossia_commit",json!({"viewId":open.view_id,"editId":edit.edit_id,"expectedSourceHash":hash,"summary":"native runtime edit"})).await).unwrap();
        assert_eq!(version.seq, 2);
        let context: ViewSnapshot = web
            .get(format!("{api}/views/{}", open.view_id))
            .bearer_auth(&session.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let key = context.render.unwrap().render_key;
        let response = web
            .get(format!("{api}/renders/{key}/pages/1"))
            .bearer_auth(&session.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.text().await.unwrap().contains("<svg"));
        for (format, extension) in [
            (OutputFormat::Ly, "ly"),
            (OutputFormat::Pdf, "pdf"),
            (OutputFormat::Midi, "mid"),
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
            match format {
                OutputFormat::Ly => assert_eq!(bytes, changed.as_bytes()),
                OutputFormat::Pdf => assert!(bytes.starts_with(b"%PDF-")),
                OutputFormat::Midi => {
                    ossia_score::parse_midi(&bytes).unwrap();
                }
                OutputFormat::Svg | OutputFormat::Png => unreachable!(),
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
        Some(json!({"viewId":ViewId::new()})),
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
    let restart = cli(&executable, &home, &engine, "status", None).await;
    assert_eq!(restart.0, 0);
    assert_ne!(
        restart.1["runtime"]["instanceId"],
        json!(discovery.runtime.instance_id)
    );
    let discovery2 = ipc::connect(&home.canonicalize().unwrap()).await.unwrap();
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
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"protocolVersion":PROTOCOL_VERSION,"tools":mcp::tools().len(),"concurrentStartup":true,"mcpDisconnectKeepsServe":true,"restart":true,"automaticPreview":true,"engineBuildId":a.1["engineBuildId"],"cases":cases})).unwrap()).unwrap();
    }
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
            ipc::call(endpoint, "job", json!({"jobId":id,"action":"get"}))
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
    let view: AgentOpenOutcome = serde_json::from_value(
        ipc::call(&old.endpoint, "open", json!({"documentId":doc.document_id}))
            .await
            .unwrap(),
    )
    .unwrap();
    let ticket = view.url.split("#ticket=").nth(1).unwrap();
    let web = reqwest::Client::new();
    let token: ExchangeResponse = web
        .post(format!(
            "{}/api/v1/sessions/exchange",
            old.runtime.web_base_url
        ))
        .json(&ExchangeRequest {
            ticket: ticket.into(),
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let edit:EditSession=serde_json::from_value(ipc::call(&old.endpoint,"edit",json!({"action":"begin","documentId":doc.document_id,"viewId":view.view_id,"baseVersionId":doc.head_version_id,"expectedSourceHash":doc.source_hash})).await.unwrap()).unwrap();
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
            .bearer_auth(&token.token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        ipc::call(&new.endpoint, "context", json!({"viewId":view.view_id}))
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let opened: AgentOpenOutcome = serde_json::from_value(
        ipc::call(&new.endpoint, "open", json!({"documentId":doc.document_id}))
            .await
            .unwrap(),
    )
    .unwrap();
    let context: AgentView = serde_json::from_value(
        ipc::call(&new.endpoint, "context", json!({"viewId":opened.view_id}))
            .await
            .unwrap(),
    )
    .unwrap();
    let current = context.document.unwrap();
    assert_eq!(current.edit_state, DocumentEditState::Interrupted);
    assert!(current.dirty);
    assert_eq!(current.source_hash, hash);
    assert_eq!(std::fs::read(&doc.source_path).unwrap(), bytes);
    let resumed:EditSession=serde_json::from_value(ipc::call(&new.endpoint,"edit",json!({"action":"resume","documentId":doc.document_id,"viewId":opened.view_id,"editId":edit.edit_id,"baseVersionId":doc.head_version_id,"expectedSourceHash":hash})).await.unwrap()).unwrap();
    assert_eq!(resumed.owner_view_id, opened.view_id);
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
