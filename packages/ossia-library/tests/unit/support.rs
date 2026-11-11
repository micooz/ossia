use crate::*;
use std::{fs, path::PathBuf, sync::Mutex};

pub(crate) type CompileHook = Box<dyn FnOnce(&SourceSnapshot) + Send>;

#[derive(Default)]
pub(crate) struct TestEngine {
    pub(crate) hook: Mutex<Option<CompileHook>>,
    pub(crate) error: Mutex<Option<AppError>>,
}

impl EngineBackend for TestEngine {
    fn build_id(&self) -> &str {
        "test-engine-1"
    }

    fn convert(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport> {
        context.check()?;
        let refused = input == b"refused";
        let loss = input == b"loss";
        let token = ContentHash::of(&[input, b"loss:test-engine-1"].concat());
        if let Some(confirmation) = confirmation {
            if refused {
                return Err(AppError::new(ErrorCode::ImportRefused, "blocked"));
            }
            if confirmation != &token {
                return Err(conflict("confirmation changed"));
            }
        }
        let source = if refused {
            None
        } else {
            Some(match format {
                InputFormat::Ly => std::str::from_utf8(input)
                    .map_err(|_| AppError::new(ErrorCode::InvalidEncoding, "UTF-8 required"))?
                    .into(),
                _ => "converted c'1".into(),
            })
        };
        Ok(ImportReport {
            status: if refused {
                ImportStatus::Refused
            } else if loss {
                ImportStatus::NeedsConfirmation
            } else {
                ImportStatus::Clean
            },
            input_hash: ContentHash::of(input),
            engine_build_id: self.build_id().into(),
            source_hash: source
                .as_ref()
                .map(|s: &String| ContentHash::of(s.as_bytes())),
            source,
            confirmation: if loss { Some(token) } else { None },
            confirmed: confirmation.is_some(),
            measures: Some(1),
            measure_count_reason: None,
            voices: 1,
            note_events: 1,
            losses: if loss { vec!["loss".into()] } else { vec![] },
            blocking: if refused {
                vec!["Negative skip".into()]
            } else {
                vec![]
            },
            diagnostics: vec![],
        })
    }

    fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest> {
        context.check()?;
        let hook = self.hook.lock().unwrap().take();
        if let Some(hook) = hook {
            hook(snapshot);
        }
        if let Some(error) = self.error.lock().unwrap().take() {
            return Err(error);
        }
        let bytes = format!("<svg><!-- {} --></svg>", snapshot.source_hash()).into_bytes();
        let hash = ContentHash::of(&bytes);
        let path = format!("assets/{hash}.svg");
        atomic_write(&artifact_root.join(&path), &bytes)?;
        let broken = snapshot.source().contains("BROKEN");
        let mut errors = vec![];
        if broken {
            errors.push(FormatError {
                format: RenderFormat::Svg,
                error: AppError::new(ErrorCode::CompileFailed, "syntax error"),
            });
        }
        if options.formats.contains(&RenderFormat::Midi) {
            errors.push(FormatError {
                format: RenderFormat::Midi,
                error: AppError::unsupported("MIDI unavailable"),
            });
        }
        Ok(RenderManifest {
            source_hash: snapshot.source_hash().clone(),
            engine_build_id: self.build_id().into(),
            options_hash: ContentHash::of(if options.point_and_click {
                b"anchors"
            } else {
                b"plain"
            }),
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
            diagnostics: if broken {
                vec![Diagnostic {
                    severity: Severity::Error,
                    message: "syntax error".into(),
                    raw_position: None,
                    position: None,
                }]
            } else {
                vec![]
            },
            format_errors: errors,
        })
    }
}

pub(crate) struct Fixture {
    pub(crate) library: Library,
    pub(crate) engine: TestEngine,
    pub(crate) doc: DocumentState,
    pub(crate) _root: tempfile::TempDir,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        Self::source("A c'1")
    }

    pub(crate) fn source(source: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let library = Library::open(root.path()).unwrap();
        let engine = TestEngine::default();
        let doc = library
            .import(
                &engine,
                ImportRequest {
                    title: "合成谱",
                    input: source.as_bytes(),
                    format: InputFormat::Ly,
                    confirmation: None,
                    author: "test",
                },
                &RunContext::default(),
            )
            .unwrap()
            .document
            .unwrap();
        Self {
            library,
            engine,
            doc,
            _root: root,
        }
    }

    pub(crate) fn begin(&self) -> EditSession {
        let state = self.library.get_document(self.doc.document_id).unwrap();
        self.library
            .begin_edit(BeginEditRequest {
                document_id: state.document_id,
                base_version_id: state.head_version_id,
                expected_source_hash: &state.source_hash,
                author: "agent",
                adopt_workspace: false,
            })
            .unwrap()
    }

    pub(crate) fn write(&self, source: &str) -> ContentHash {
        fs::write(&self.doc.source_path, source).unwrap();
        ContentHash::of(source.as_bytes())
    }

    pub(crate) fn check(&self, edit: &EditSession, hash: &ContentHash) -> CheckOutcome {
        self.library
            .check(
                &self.engine,
                CheckRequest {
                    expect: None,
                    reference: None,
                    document_id: self.doc.document_id,
                    edit_id: Some(edit.edit_id),
                    expected_source_hash: hash,
                    options: &CompileOptions::default(),
                },
                &RunContext::default(),
            )
            .unwrap()
    }

    pub(crate) fn commit(&self, edit: &EditSession, hash: &ContentHash) -> Result<Version> {
        self.library.commit(CommitRequest {
            edit_id: edit.edit_id,
            base_version_id: edit.base_version_id,
            expected_source_hash: hash,
            summary: "edit",
        })
    }

    pub(crate) fn change(&self, source: &str) -> Version {
        let edit = self.begin();
        let hash = self.write(source);
        self.check(&edit, &hash);
        self.commit(&edit, &hash).unwrap()
    }

    pub(crate) fn restore(&self, target: VersionId) -> Result<Version> {
        let doc = self.library.get_document(self.doc.document_id)?;
        self.library.restore(
            &self.engine,
            RestoreRequest {
                target_version_id: target,
                base_version_id: doc.head_version_id,
                expected_source_hash: &doc.source_hash,
                author: "test",
                summary: "restore",
            },
            &RunContext::default(),
        )
    }

    pub(crate) fn hook(
        &self,
        point: &'static str,
        hook: impl FnOnce(&Library) -> Result<()> + Send + 'static,
    ) {
        self.library
            .hooks
            .lock()
            .unwrap()
            .insert(point, Box::new(hook));
    }
}

#[test]
#[ignore = "subprocess fixture invoked by lock and crash recovery regressions"]
fn process_fixture() {
    let home = PathBuf::from(std::env::var_os("OSSIA_LIBRARY_TEST_HOME").unwrap());
    let mode = std::env::var("OSSIA_LIBRARY_TEST_MODE").unwrap();
    if mode == "lock" {
        assert_eq!(
            Library::open(&home).err().unwrap().code,
            ErrorCode::LibraryBusy
        );
        return;
    }
    let library = Library::open(&home).unwrap();
    let engine = TestEngine::default();
    let doc = library.list_documents().unwrap().pop().unwrap();
    let point: &'static str = match mode.as_str() {
        "after-intent" => "after-intent",
        "after-replace" => "after-replace",
        "before-intent-finalize" => "before-intent-finalize",
        _ => panic!("unknown fixture"),
    };
    library
        .hooks
        .lock()
        .unwrap()
        .insert(point, Box::new(|_| std::process::exit(73)));
    let target = std::env::var("OSSIA_LIBRARY_TEST_TARGET")
        .unwrap()
        .parse()
        .unwrap();
    library
        .restore(
            &engine,
            RestoreRequest {
                target_version_id: target,
                base_version_id: doc.head_version_id,
                expected_source_hash: &doc.source_hash,
                author: "test",
                summary: "crash",
            },
            &RunContext::default(),
        )
        .unwrap();
    panic!("fault point was not reached");
}
