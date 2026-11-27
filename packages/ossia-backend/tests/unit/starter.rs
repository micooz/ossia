use super::*;
use ossia_engine::{CompileOptions, RunContext};
use ossia_library::{EngineBackend, Library};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct TestEngine {
    fail: AtomicBool,
}

impl EngineBackend for TestEngine {
    fn build_id(&self) -> &str {
        "starter-test-engine"
    }

    fn convert(
        &self,
        input: &[u8],
        _: InputFormat,
        _: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport> {
        context.check()?;
        let source = String::from_utf8(input.to_vec())
            .map_err(|error| AppError::new(ErrorCode::InvalidEncoding, error.to_string()))?;
        Ok(ImportReport {
            status: ImportStatus::Clean,
            input_hash: ContentHash::of(input),
            engine_build_id: self.build_id().into(),
            omr_source: None,
            lilypond_version: None,
            playback_error: None,
            source_hash: Some(ContentHash::of(input)),
            source: Some(source),
            confirmation: None,
            confirmed: false,
            measures: Some(1),
            measure_count_reason: None,
            voices: 1,
            note_events: 1,
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
        context.check()?;
        if self.fail.load(Ordering::Acquire) {
            return Err(AppError::new(
                ErrorCode::CompileFailed,
                "starter test compile failure",
            ));
        }
        let bytes = format!("<svg><!-- {} --></svg>", snapshot.source_hash()).into_bytes();
        let hash = ContentHash::of(&bytes);
        let path = format!("assets/{hash}.svg");
        fs::create_dir_all(root.join("assets")).map_err(storage)?;
        fs::write(root.join(&path), &bytes).map_err(storage)?;
        let midi = b"MThd\0\0\0\x06\0\0\0\x01\0\x60MTrk\0\0\0\x0c\0\x90\x3c\x40\x60\x80\x3c\x40\0\xff\x2f\0";
        let midi_hash = ContentHash::of(midi);
        let midi_path = format!("assets/{midi_hash}.midi");
        fs::write(root.join(&midi_path), midi).map_err(storage)?;
        Ok(RenderManifest {
            source_hash: snapshot.source_hash().clone(),
            engine_build_id: self.build_id().into(),
            options_hash: ossia_engine::render_options_hash(options.point_and_click),
            pages: vec![RenderPage {
                page: 1,
                artifact: ArtifactRef {
                    path,
                    sha256: hash,
                    bytes: bytes.len() as u64,
                },
            }],
            pdf: None,
            midi: Some(ArtifactRef {
                path: midi_path,
                sha256: midi_hash,
                bytes: midi.len() as u64,
            }),
            diagnostics: vec![],
            format_errors: vec![],
        })
    }
}

fn starter(id: &str, title: &str, input: &'static [u8]) -> StarterScore {
    StarterScore {
        id: id.into(),
        title: title.into(),
        file: format!("{id}.ly"),
        format: InputFormat::Ly,
        author: DEFAULT_AUTHOR.into(),
        input,
    }
}

async fn runtime(root: &Path) -> (Arc<Runtime>, Arc<TestEngine>, tokio::task::JoinHandle<()>) {
    let library = Library::open(root).unwrap();
    let engine = Arc::new(TestEngine::default());
    let runtime = Runtime::new(library, engine.clone(), "http://127.0.0.1:3211".into());
    let scheduler = tokio::spawn(runtime.clone().schedule());
    (runtime, engine, scheduler)
}

async fn finish(runtime: Arc<Runtime>, scheduler: tokio::task::JoinHandle<()>) {
    runtime.stop().unwrap();
    scheduler.await.unwrap();
}

#[test]
fn embedded_manifest_and_resources_are_valid() {
    let scores = manifest().unwrap();
    assert_eq!(scores.len(), 5);
    assert_eq!(scores[0].id, "praeludium-i");
    assert_eq!(scores[0].title, "Piano: Praeludium I (BWV 846)");
    assert!(scores[0].input.starts_with(b"\\version \"2.26.0\""));
    assert_eq!(
        scores[4].input,
        include_bytes!("../../../ossia-integrations/resources/skill/ossia/references/jianpu.ly")
    );
}

#[tokio::test]
async fn embedded_starters_populate_a_new_library() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _engine, scheduler) = runtime(root.path()).await;
    let scores = manifest().unwrap();

    run(runtime.clone()).await.unwrap();

    let documents = runtime.library.list_documents().unwrap();
    assert_eq!(documents.len(), scores.len());
    for score in &scores {
        assert!(documents
            .iter()
            .any(|document| document.title == score.title));
    }
    let mut documents = runtime
        .library
        .management_score_page(false, 100, 0)
        .unwrap()
        .items;
    documents.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    assert_eq!(
        documents
            .iter()
            .map(|document| &document.title)
            .collect::<Vec<_>>(),
        scores.iter().map(|score| &score.title).collect::<Vec<_>>()
    );
    assert!(!runtime.library.initial_documents_pending().unwrap());
    finish(runtime, scheduler).await;
}

#[tokio::test]
async fn multiple_starter_scores_import_once_and_skip_existing_content() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _engine, scheduler) = runtime(root.path()).await;
    let scores = [
        starter("first", "First starter", b"\\score { c'1 }"),
        starter("second", "Second starter", b"\\score { d'1 }"),
    ];

    import_scores(&runtime, &scores).await.unwrap();
    runtime.library.finish_initial_documents().unwrap();
    let documents = runtime.library.list_documents().unwrap();
    assert_eq!(documents.len(), 2);
    assert!(documents
        .iter()
        .any(|document| document.title == "First starter"));
    assert!(documents
        .iter()
        .any(|document| document.title == "Second starter"));

    import_scores(&runtime, &scores).await.unwrap();
    assert_eq!(runtime.library.list_documents().unwrap().len(), 2);
    finish(runtime, scheduler).await;
}

#[tokio::test]
async fn failed_starter_import_keeps_pending_marker_for_retry() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, engine, scheduler) = runtime(root.path()).await;
    let scores = [starter("retry", "Retry starter", b"\\score { e'1 }")];

    engine.fail.store(true, Ordering::Release);
    assert!(import_scores(&runtime, &scores).await.is_err());
    assert!(runtime.library.initial_documents_pending().unwrap());
    assert_eq!(runtime.library.document_count().unwrap(), 0);

    engine.fail.store(false, Ordering::Release);
    import_scores(&runtime, &scores).await.unwrap();
    runtime.library.finish_initial_documents().unwrap();
    assert!(!runtime.library.initial_documents_pending().unwrap());
    assert_eq!(runtime.library.document_count().unwrap(), 1);
    finish(runtime, scheduler).await;
}
