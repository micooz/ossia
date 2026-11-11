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
fn command(bin: &Path, home: &Path, engine: &Path, docs: &Path) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(bin);
    c.arg("--home")
        .arg(home)
        .args(["--port", "0", "--engine-root"])
        .arg(engine)
        .arg("--docs-root")
        .arg(docs);
    c
}
async fn cli(
    bin: &Path,
    home: &Path,
    engine: &Path,
    docs: &Path,
    name: &str,
    args: Value,
) -> Value {
    let mut c = command(bin, home, engine, docs);
    c.arg(name);
    if !args.is_null() {
        c.arg(args.to_string());
    }
    let output = c.output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
async fn call(client: &Client, name: &str, args: Value) -> Value {
    let result = client
        .call_tool(CallToolRequestParam {
            name: name.to_string().into(),
            arguments: args.as_object().cloned(),
        })
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{name}: {:?}", result.content);
    serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
}
async fn wait(client: &Client, initial: Value) -> Job {
    let mut job: Job = serde_json::from_value(initial).unwrap();
    let start = Instant::now();
    while matches!(job.state, JobState::Queued | JobState::Running) {
        assert!(start.elapsed() < Duration::from_secs(60));
        tokio::time::sleep(Duration::from_millis(30)).await;
        let outcome: JobOutcome = serde_json::from_value(
            call(
                client,
                "ossia_job",
                json!({"jobId":job.job_id,"action":"get","detail":"full"}),
            )
            .await,
        )
        .unwrap();
        job = outcome.job;
    }
    assert_eq!(job.state, JobState::Done, "{job:?}");
    job
}
struct Stop(String);
impl Drop for Stop {
    fn drop(&mut self) {
        let endpoint = self.0.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(ipc::call(&endpoint, "stop", json!({})));
        })
        .join();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires native executable, LilyPond and prepared offline docs"]
async fn offline_docs_cli_and_mcp_complete_the_agent_music_workflow() {
    let bin =
        PathBuf::from(std::env::var_os("OSSIA_TEST_EXECUTABLE").expect("OSSIA_TEST_EXECUTABLE"));
    let engine = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("OSSIA_TEST_LILYPOND_ROOT"),
    );
    let docs =
        PathBuf::from(std::env::var_os("OSSIA_TEST_DOCS_ROOT").expect("OSSIA_TEST_DOCS_ROOT"));
    let work = tempfile::tempdir().unwrap();
    let home = work.path().join("library");
    cli(&bin, &home, &engine, &docs, "status", Value::Null).await;
    let discovery = ipc::connect(&home.canonicalize().unwrap()).await.unwrap();
    let _stop = Stop(discovery.endpoint.clone());
    let hits: DocsOutcome = serde_json::from_value(
        cli(
            &bin,
            &home,
            &engine,
            &docs,
            "docs",
            json!({"action":"search","query":"八度"}),
        )
        .await,
    )
    .unwrap();
    assert!(!hits.hits.is_empty());
    assert!(Path::new(&hits.root).starts_with(home.canonicalize().unwrap()));
    let mut c = command(&bin, &home, &engine, &docs);
    c.arg("mcp");
    let client = ClientInfo::default()
        .serve(TokioChildProcess::new(c).unwrap())
        .await
        .unwrap();
    let chapter = call(
        &client,
        "ossia_docs",
        json!({"action":"read","chapterId":"notation/writing-pitches#absolute-octave-entry"}),
    )
    .await;
    assert!(chapter["text"].as_str().unwrap().contains("c'4"));
    let source="\\version \"2.26.0\"\n\\header { title = \"Scale\" }\nMusic = { \\time 4/4 c4 d e f | g4 a b c' | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }";
    let input = work.path().join("scale.ly");
    std::fs::write(&input, source).unwrap();
    let imported = wait(
        &client,
        cli(
            &bin,
            &home,
            &engine,
            &docs,
            "import",
            json!({"path":input,"format":"ly","title":"Scale","author":"test"}),
        )
        .await,
    )
    .await;
    let outcome: ImportOutcome = serde_json::from_value(imported.result.unwrap()).unwrap();
    let doc = outcome.document.unwrap();
    let music = outcome.music.unwrap();
    assert_eq!(music.source.measures, Some(2));
    assert_eq!(music.performance.notes.len(), 8);
    let first = &music.performance.notes[0];
    let opened: AgentOpenOutcome = serde_json::from_value(
        call(&client, "ossia_open", json!({"documentId":doc.document_id})).await,
    )
    .unwrap();
    let edit:EditSession=serde_json::from_value(call(&client,"ossia_edit",json!({"action":"begin","documentId":doc.document_id,"viewId":opened.view_id,"baseVersionId":doc.head_version_id,"expectedSourceHash":doc.source_hash})).await).unwrap();
    let page:ReadSourceOutcome=serde_json::from_value(cli(&bin,&home,&engine,&docs,"read-source",json!({"documentId":doc.document_id,"viewId":opened.view_id,"expectedSourceHash":doc.source_hash,"start":{"kind":"start"},"maxLines":200,"maxBytes":262144})).await).unwrap();
    assert_eq!(page.content, source);
    assert!(page.complete);
    assert_eq!(page.source_hash, doc.source_hash);
    let mcp_page:ReadSourceOutcome=serde_json::from_value(call(&client,"ossia_read_source",json!({"documentId":doc.document_id,"viewId":opened.view_id,"expectedSourceHash":doc.source_hash,"start":{"kind":"span","span":page.span},"maxLines":200,"maxBytes":262144})).await).unwrap();
    assert_eq!(mcp_page.content, source);
    assert_eq!(mcp_page.span, page.span);
    let expected_notes = [60, 62, 64, 65, 67, 69, 71, 72]
        .into_iter()
        .enumerate()
        .map(|(index, pitch)| {
            json!({
                "pitch": pitch,
                "start": {"numerator": index, "denominator": 1},
                "duration": {"numerator": 1, "denominator": 1}
            })
        })
        .collect::<Vec<_>>();
    let expected = json!({
        "noteCount": 8,
        "tracks": [{
            "track": first.track,
            "channel": first.channel,
            "pitches": [60, 62, 64, 65, 67, 69, 71, 72],
            "notes": expected_notes
        }]
    });
    let wrong=wait(&client,call(&client,"ossia_check",json!({"viewId":opened.view_id,"editId":edit.edit_id,"expectedSourceHash":doc.source_hash,"expect":expected})).await).await;
    assert_eq!(wrong.result.unwrap()["verification"]["status"], "failed");
    let rejected=client.call_tool(CallToolRequestParam {name:"ossia_commit".into(),arguments:json!({"viewId":opened.view_id,"editId":edit.edit_id,"expectedSourceHash":doc.source_hash,"summary":"wrong octave"}).as_object().cloned()}).await.unwrap();
    assert_eq!(rejected.is_error, Some(true));
    let original_notes = "c4 d e f | g4 a b c'";
    let map = ossia_score::SourceMap::new(source);
    let start = source.find(original_notes).unwrap();
    let span = map.span(start, start + original_notes.len()).unwrap();
    let written=cli(&bin,&home,&engine,&docs,"write-source",json!({"viewId":opened.view_id,"editId":edit.edit_id,"expectedSourceHash":doc.source_hash,"change":{"kind":"edit","edits":[{"span":span,"expectedText":original_notes,"replacement":"c'4 d' e' f' | g'4 a' b' c''"}]}})).await;
    let hash = written["sourceHash"].clone();
    let good=wait(&client,call(&client,"ossia_check",json!({"viewId":opened.view_id,"editId":edit.edit_id,"expectedSourceHash":hash,"expect":expected})).await).await;
    assert_eq!(
        good.result.as_ref().unwrap()["verification"]["status"],
        "passed"
    );
    let mut png_path = None;
    for format in ["png", "midi", "pdf"] {
        let artifact = wait(
            &client,
            call(
                &client,
                "ossia_output",
                json!({
                    "source":{"kind":"job","jobId":good.job_id},
                    "format":format,
                    "outputPath":work.path().join(format!("scale.{format}"))
                }),
            )
            .await,
        )
        .await;
        let result: OutputOutcome = serde_json::from_value(artifact.result.unwrap()).unwrap();
        assert_eq!(serde_json::to_value(result.source_hash).unwrap(), hash);
        assert!(Path::new(&result.path).is_file());
        if format == "png" {
            png_path = Some(result.path);
        }
    }
    let version=call(&client,"ossia_commit",json!({"viewId":opened.view_id,"editId":edit.edit_id,"expectedSourceHash":hash,"summary":"verified C4–C5 scale"})).await;
    wait(&client,cli(&bin,&home,&engine,&docs,"output",json!({"source":{"kind":"version","versionId":version["versionId"]},"format":"pdf","outputPath":work.path().join("scale-final.pdf")})).await).await;
    let piano="\\version \"2.26.0\"\n\\score { \\new PianoStaff << \\new Staff { c'2 e'2 | } \\new Staff { \\clef bass c2 g2 | } >> \\layout {} \\midi {} }";
    std::fs::write(&input, piano).unwrap();
    let piano = wait(
        &client,
        cli(
            &bin,
            &home,
            &engine,
            &docs,
            "import",
            json!({"path":input,"format":"ly","title":"Piano","author":"test"}),
        )
        .await,
    )
    .await;
    let outcome: ImportOutcome = serde_json::from_value(piano.result.unwrap()).unwrap();
    let music = outcome.music.unwrap();
    assert_eq!(music.performance.notes.len(), 4);
    assert!(music
        .performance
        .notes
        .iter()
        .all(|n| n.duration == Some(QuarterTime::new(2, 1).unwrap())));
    assert!(
        music
            .performance
            .notes
            .iter()
            .map(|n| n.track)
            .collect::<std::collections::HashSet<_>>()
            .len()
            >= 2
    );
    let doctor = cli(&bin, &home, &engine, &docs, "doctor", Value::Null).await;
    assert_eq!(doctor["documentation"]["available"], true);
    if let Some(evidence) = std::env::var_os("OSSIA_AGENT_EVIDENCE_DIR") {
        let dir = PathBuf::from(evidence);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(png_path.unwrap(), dir.join("scale.png")).unwrap();
        std::fs::write(dir.join("workflow.json"),serde_json::to_vec_pretty(&json!({"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"version":version,"documentation":doctor["documentation"],"piano":music})).unwrap()).unwrap();
    }
    client.cancel().await.unwrap();
}
