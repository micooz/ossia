#![allow(dead_code)]

use ossia_engine::{CompileOptions, RunContext};
use ossia_library::{EngineBackend, ImportRequest, Library};
use ossia_runtime::*;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub const SOURCE: &[u8] =
    b"\\version \"2.26.0\"\n\\score { \\new Staff { c'1 | d'1 | } \\layout {} }\n";

pub fn measured_midi() -> Vec<u8> {
    let track = [
        0, 0xff, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08, 0, 0x90, 60, 80, 96, 0x80, 60, 0, 0x82, 0x20,
        0x90, 62, 80, 96, 0x80, 62, 0, 0, 0xff, 0x2f, 0,
    ];
    let mut midi = b"MThd\0\0\0\x06\0\0\0\x01\0\x60MTrk".to_vec();
    midi.extend((track.len() as u32).to_be_bytes());
    midi.extend(track);
    midi
}

#[derive(Default)]
pub struct Engine {
    pub hold: AtomicBool,
    pub midi: AtomicBool,
    pub interaction: AtomicBool,
    pub legacy_render: AtomicBool,
    pub active: AtomicUsize,
    pub peak: AtomicUsize,
    pub documents: Mutex<Vec<DocumentId>>,
    pub seen: Mutex<Vec<ContentHash>>,
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
            measures: Some(2),
            measure_count_reason: None,
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
        let objects = if self.interaction.load(Ordering::Acquire) {
            let mut objects = String::from("<a data-ossia-kind='Clef' data-ossia-staff='0'><rect x='5' y='20' width='3' height='6'/></a>");
            for (i, token) in ossia_score::tokenize(snapshot.source())
                .iter()
                .filter(|t| t.kind == ossia_score::TokenKind::Note)
                .enumerate()
            {
                for y in [20, 50] {
                    objects.push_str(&format!("<a data-ossia-kind='NoteHead' data-ossia-staff='0' data-ossia-id='n{i}-{y}' data-ossia-owners='n{i}-{y}' data-ossia-source='{}:{}'><rect x='{}' y='{y}' width='2' height='1'/></a>", token.byte_start, token.byte_end, 20 + i * 10));
                }
            }
            let tokens = ossia_score::tokenize(snapshot.source());
            let notes: Vec<_> = tokens
                .iter()
                .filter(|t| t.kind == ossia_score::TokenKind::Note)
                .collect();
            for tie in tokens.iter().filter(|t| t.raw == "~") {
                if let Some(left) = notes.iter().rposition(|n| n.byte_end <= tie.byte_start) {
                    if left + 1 < notes.len() {
                        let right = left + 1;
                        for y in [20, 50] {
                            objects.push_str(&format!("<a data-ossia-kind='Tie' data-ossia-staff='0' data-ossia-id='t{left}-{y}' data-ossia-owners='n{left}-{y},n{right}-{y}' data-ossia-source='{}:{}'><rect x='22' y='{}' width='8' height='1'/></a>", tie.byte_start, tie.byte_end, y + 2));
                        }
                    }
                }
            }
            objects
        } else {
            String::new()
        };
        let bytes = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><!-- {} -->{objects}</svg>",
            snapshot.source_hash(),
        )
        .into_bytes();
        let hash = ContentHash::of(&bytes);
        let path = format!("assets/{hash}.svg");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join(&path), &bytes).unwrap();
        let midi =
            if self.midi.load(Ordering::Acquire) && options.formats.contains(&RenderFormat::Midi) {
                let bytes = if snapshot.source().contains("MEASURED") {
                    measured_midi()
                } else {
                    include_bytes!("../../../../tests/fixtures/inputs/midi-valid.mid").to_vec()
                };
                let hash = ContentHash::of(&bytes);
                let path = format!("assets/{hash}.mid");
                std::fs::write(root.join(&path), &bytes).unwrap();
                Some(ArtifactRef {
                    path,
                    sha256: hash,
                    bytes: bytes.len() as u64,
                })
            } else {
                None
            };
        let broken = snapshot.source().contains("BROKEN");
        Ok(RenderManifest {
            source_hash: snapshot.source_hash().clone(),
            engine_build_id: self.build_id().into(),
            options_hash: if self.legacy_render.load(Ordering::Acquire) {
                ContentHash::of(b"legacy")
            } else {
                ossia_engine::render_options_hash(options.point_and_click)
            },
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
            midi,
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

pub struct Fixture {
    pub runtime: Arc<Runtime>,
    pub engine: Arc<Engine>,
    pub doc: DocumentState,
    pub root: tempfile::TempDir,
    pub scheduler: Option<tokio::task::JoinHandle<()>>,
}

impl Fixture {
    pub async fn new(schedule: bool) -> Self {
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
        runtime.context(doc.document_id).unwrap();
        let scheduler = schedule.then(|| tokio::spawn(runtime.clone().schedule()));
        Self {
            runtime,
            engine,
            doc,
            root,
            scheduler,
        }
    }

    pub fn check(&self, hash: ContentHash) -> Job {
        self.runtime
            .check(
                CheckArgs {
                    expect: None,
                    reference: None,
                    document_id: Some(self.doc.document_id),
                    edit_id: None,
                    expected_source_hash: hash,
                    wait_ms: None,
                },
                false,
            )
            .unwrap()
    }

    pub fn save(&self, source: &[u8]) -> ContentHash {
        std::fs::write(&self.doc.source_path, source).unwrap();
        ContentHash::of(source)
    }

    pub fn begin(&self) -> EditSession {
        serde_json::from_value(
            self.runtime
                .edit(EditArgs {
                    action: EditAction::Begin,
                    document_id: self.doc.document_id,
                    base_version_id: self.doc.head_version_id,
                    expected_source_hash: self.doc.source_hash.clone(),
                    edit_id: None,
                    adopt_workspace: false,
                    author: "test-agent".into(),
                })
                .unwrap(),
        )
        .unwrap()
    }

    pub async fn finish(mut self) {
        self.runtime.stop().unwrap();
        if let Some(scheduler) = self.scheduler.take() {
            tokio::time::timeout(Duration::from_secs(5), scheduler)
                .await
                .unwrap()
                .unwrap();
        }
    }
}

pub async fn wait_job(runtime: &Runtime, id: JobId) -> Job {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let job = runtime.library.get_job(id).unwrap();
        if !matches!(job.state, JobState::Queued | JobState::Running) {
            return job;
        }
        assert!(Instant::now() < deadline, "job stuck: {job:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub async fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !predicate() {
        assert!(Instant::now() < deadline, "condition timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
