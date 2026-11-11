use ossia_engine::{CompileOptions, RunContext};
use ossia_library::*;
use std::{
    fs,
    path::Path,
    sync::{Arc, Barrier},
    thread,
};

type CompileHook = Box<dyn FnOnce(&SourceSnapshot) + Send>;

#[derive(Default)]
struct TestEngine {
    hook: std::sync::Mutex<Option<CompileHook>>,
    error: std::sync::Mutex<Option<AppError>>,
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
                return Err(AppError::new(ErrorCode::Conflict, "confirmation changed"));
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
        let asset = artifact_root.join(&path);
        fs::create_dir_all(asset.parent().unwrap()).unwrap();
        fs::write(asset, &bytes).unwrap();
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

struct Fixture {
    library: Library,
    engine: TestEngine,
    doc: DocumentState,
    _root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self::source("A c'1")
    }

    fn source(source: &str) -> Self {
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

    fn begin(&self) -> EditSession {
        let state = self.library.get_document(self.doc.document_id).unwrap();
        self.library
            .begin_edit(BeginEditRequest {
                document_id: state.document_id,
                view_id: ViewId::new(),
                base_version_id: state.head_version_id,
                expected_source_hash: &state.source_hash,
                author: "agent",
                adopt_workspace: false,
            })
            .unwrap()
    }

    fn write(&self, source: &str) -> ContentHash {
        fs::write(&self.doc.source_path, source).unwrap();
        ContentHash::of(source.as_bytes())
    }

    fn check(&self, edit: &EditSession, hash: &ContentHash) -> CheckOutcome {
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

    fn commit(&self, edit: &EditSession, hash: &ContentHash) -> Result<Version> {
        self.library.commit(CommitRequest {
            edit_id: edit.edit_id,
            base_version_id: edit.base_version_id,
            expected_source_hash: hash,
            summary: "edit",
        })
    }

    fn change(&self, source: &str) -> Version {
        let edit = self.begin();
        let hash = self.write(source);
        self.check(&edit, &hash);
        self.commit(&edit, &hash).unwrap()
    }

    fn restore(&self, target: VersionId) -> Result<Version> {
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
}

#[test]
fn import_expectation_failure_does_not_publish_a_document() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = TestEngine::default();
    let expect = MusicExpectation {
        note_count: Some(1),
        tracks: vec![],
    };
    let error = library
        .import_with_expect(
            &engine,
            ImportRequest {
                title: "scale",
                input: b"c'1",
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            Some(&expect),
            &RunContext::default(),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert_eq!(
        error.detail.as_ref().unwrap()["verification"]["status"],
        "unavailable"
    );
    assert_eq!(library.document_count().unwrap(), 0);

    library
        .import(
            &engine,
            ImportRequest {
                title: "scale",
                input: b"c'1",
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(library.document_count().unwrap(), 1);
}

#[test]
fn three_import_formats_preserve_original_bytes_and_utf8_source() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = TestEngine::default();
    for (input, format, name) in [
        (b"\xef\xbb\xbfA\r\n".as_slice(), InputFormat::Ly, "input.ly"),
        (
            b"<xml/>".as_slice(),
            InputFormat::MusicXml,
            "input.musicxml",
        ),
        (b"midi".as_slice(), InputFormat::Midi, "input.mid"),
    ] {
        let result = library
            .import(
                &engine,
                ImportRequest {
                    title: "../untrusted",
                    input,
                    format,
                    confirmation: None,
                    author: "test",
                },
                &RunContext::default(),
            )
            .unwrap();
        let doc = result.document.unwrap();
        let original = Path::new(&doc.source_path)
            .parent()
            .unwrap()
            .join("original")
            .join(name);
        assert_eq!(fs::read(original).unwrap(), input);
        let version = library.history(doc.document_id).unwrap().pop().unwrap();
        let snapshot = library.version_snapshot(version.version_id).unwrap();
        assert_eq!(snapshot.bytes(), fs::read(&doc.source_path).unwrap());
        if format == InputFormat::Ly {
            assert_eq!(snapshot.bytes(), input);
        }
        assert!(!doc.dirty);
    }
    assert_eq!(library.list_documents().unwrap().len(), 3);
    assert_eq!(
        library
            .import(
                &engine,
                ImportRequest {
                    title: "invalid",
                    input: &[0xff],
                    format: InputFormat::Ly,
                    confirmation: None,
                    author: "test"
                },
                &RunContext::default()
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidEncoding
    );
}

#[test]
fn loss_confirmation_and_refusal_never_publish_unconfirmed_documents() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let engine = TestEngine::default();
    for input in [b"loss".as_slice(), b"refused".as_slice()] {
        let result = library
            .import(
                &engine,
                ImportRequest {
                    title: "test",
                    input,
                    format: InputFormat::MusicXml,
                    confirmation: None,
                    author: "test",
                },
                &RunContext::default(),
            )
            .unwrap();
        assert!(result.document.is_none());
        assert!(library.list_documents().unwrap().is_empty());
        assert!(!root.path().join("documents").exists());
        if let Some(token) = result.report.confirmation {
            assert_eq!(
                library
                    .import(
                        &engine,
                        ImportRequest {
                            title: "test",
                            input,
                            format: InputFormat::MusicXml,
                            confirmation: Some(&ContentHash::of(b"wrong")),
                            author: "test"
                        },
                        &RunContext::default()
                    )
                    .unwrap_err()
                    .code,
                ErrorCode::Conflict
            );
            assert!(library
                .import(
                    &engine,
                    ImportRequest {
                        title: "test",
                        input,
                        format: InputFormat::MusicXml,
                        confirmation: Some(&token),
                        author: "test"
                    },
                    &RunContext::default()
                )
                .unwrap()
                .document
                .is_some());
            break;
        }
    }
    let refused = library
        .import(
            &engine,
            ImportRequest {
                title: "refused",
                input: b"refused",
                format: InputFormat::MusicXml,
                confirmation: None,
                author: "test",
            },
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(refused.report.status, ImportStatus::Refused);
    assert!(refused.document.is_none());
    assert_eq!(library.list_documents().unwrap().len(), 1);
}

#[test]
fn invalid_handwritten_import_survives_restart_and_first_fix_is_import_version() {
    let f = Fixture::source("BROKEN");
    let home = f.library.home().to_owned();
    let id = f.doc.document_id;
    assert!(f.doc.head_version_id.is_none());
    assert!(f.doc.dirty);
    assert_eq!(f.doc.last_check.as_ref().unwrap().state, JobState::Error);
    let edit = f.begin();
    assert_eq!(
        f.library
            .discard(id, Some(edit.edit_id), None, &f.doc.source_hash)
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorkspace
    );
    f.library.end_edit(edit.edit_id).unwrap();
    drop(f.library);
    let library = Library::open(&home).unwrap();
    assert!(library
        .get_document(id)
        .unwrap()
        .last_check
        .unwrap()
        .result
        .unwrap()
        .to_string()
        .contains("syntax error"));
    let state = library.get_document(id).unwrap();
    let edit = library
        .begin_edit(BeginEditRequest {
            document_id: id,
            view_id: ViewId::new(),
            base_version_id: None,
            expected_source_hash: &state.source_hash,
            author: "fixer",
            adopt_workspace: false,
        })
        .unwrap();
    fs::write(&state.source_path, "fixed").unwrap();
    let hash = ContentHash::of(b"fixed");
    library
        .check(
            &f.engine,
            CheckRequest {
                expect: None,
                reference: None,
                document_id: id,
                edit_id: Some(edit.edit_id),
                expected_source_hash: &hash,
                options: &CompileOptions::default(),
            },
            &RunContext::default(),
        )
        .unwrap();
    let version = library
        .commit(CommitRequest {
            edit_id: edit.edit_id,
            base_version_id: None,
            expected_source_hash: &hash,
            summary: "fix",
        })
        .unwrap();
    assert_eq!(
        (version.seq, version.origin, version.parent_version_id),
        (1, VersionOrigin::Import, None)
    );
}

#[test]
fn ownership_unchanged_commits_and_explicit_workspace_adoption() {
    let f = Fixture::new();
    let edit = f.begin();
    assert_eq!(
        f.library
            .begin_edit(BeginEditRequest {
                document_id: f.doc.document_id,
                view_id: ViewId::new(),
                base_version_id: f.doc.head_version_id,
                expected_source_hash: &f.doc.source_hash,
                author: "other",
                adopt_workspace: false
            })
            .unwrap_err()
            .code,
        ErrorCode::EditBusy
    );
    assert!(
        !serde_json::to_string(&f.library.get_document(f.doc.document_id).unwrap())
            .unwrap()
            .contains(&edit.edit_id.to_string())
    );
    let version = f.commit(&edit, &f.doc.source_hash).unwrap();
    assert_eq!(Some(version.version_id), f.doc.head_version_id);
    assert_eq!(f.library.history(f.doc.document_id).unwrap().len(), 1);
    assert_eq!(
        f.commit(&edit, &f.doc.source_hash).unwrap_err().code,
        ErrorCode::Conflict
    );
    let hash = f.write("unknown");
    assert_eq!(
        f.library
            .begin_edit(BeginEditRequest {
                document_id: f.doc.document_id,
                view_id: ViewId::new(),
                base_version_id: f.doc.head_version_id,
                expected_source_hash: &hash,
                author: "agent",
                adopt_workspace: false
            })
            .unwrap_err()
            .code,
        ErrorCode::DirtyWorkspace
    );
    let edit = f
        .library
        .begin_edit(BeginEditRequest {
            document_id: f.doc.document_id,
            view_id: ViewId::new(),
            base_version_id: f.doc.head_version_id,
            expected_source_hash: &hash,
            author: "agent",
            adopt_workspace: true,
        })
        .unwrap();
    f.library.end_edit(edit.edit_id).unwrap();
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"unknown");
    assert!(f.library.get_document(f.doc.document_id).unwrap().dirty);
    assert_eq!(
        f.library
            .discard(
                f.doc.document_id,
                None,
                f.doc.head_version_id,
                &f.doc.source_hash
            )
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    f.library
        .discard(f.doc.document_id, None, f.doc.head_version_id, &hash)
        .unwrap();
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"A c'1");
}

#[test]
fn commit_requires_exact_checked_source_head_and_undamaged_svg() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("B");
    assert_eq!(
        f.commit(&edit, &hash).unwrap_err().code,
        ErrorCode::CompileFailed
    );
    let result = f.check(&edit, &hash);
    assert_eq!(result.job.state, JobState::Done);
    let changed = f.write("C");
    assert_eq!(
        f.commit(&edit, &hash).unwrap_err().code,
        ErrorCode::Conflict
    );
    assert_eq!(
        f.commit(&edit, &changed).unwrap_err().code,
        ErrorCode::CompileFailed
    );
    f.write("B");
    assert_eq!(
        f.library
            .commit(CommitRequest {
                edit_id: edit.edit_id,
                base_version_id: None,
                expected_source_hash: &hash,
                summary: "stale"
            })
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    let asset = f
        .library
        .artifact_root()
        .unwrap()
        .join(&result.render.pages[0].artifact.path);
    fs::write(asset, b"damaged").unwrap();
    assert_eq!(
        f.commit(&edit, &hash).unwrap_err().code,
        ErrorCode::StorageError
    );
    assert_eq!(f.library.history(f.doc.document_id).unwrap().len(), 1);
}

#[test]
fn failed_svg_and_partial_formats_have_independent_commit_gates() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("BROKEN");
    assert_eq!(f.check(&edit, &hash).job.state, JobState::Error);
    assert_eq!(
        f.commit(&edit, &hash).unwrap_err().code,
        ErrorCode::CompileFailed
    );
    let hash = f.write("valid opaque Scheme");
    let options = CompileOptions {
        formats: vec![RenderFormat::Svg, RenderFormat::Midi],
        point_and_click: true,
    };
    let result = f
        .library
        .check(
            &f.engine,
            CheckRequest {
                expect: None,
                reference: None,
                document_id: f.doc.document_id,
                edit_id: Some(edit.edit_id),
                expected_source_hash: &hash,
                options: &options,
            },
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(result.job.state, JobState::Done);
    assert_eq!(result.render.format_errors[0].format, RenderFormat::Midi);
    f.commit(&edit, &hash).unwrap();
    assert!(!f.library.get_document(f.doc.document_id).unwrap().dirty);
}

#[test]
fn restore_appends_distinct_nodes_even_when_content_repeats() {
    let f = Fixture::new();
    let a = f.doc.head_version_id.unwrap();
    let b = f.change("B");
    let restored = f.restore(a).unwrap();
    assert_eq!(restored.parent_version_id, Some(b.version_id));
    assert_eq!(restored.seq, 3);
    assert_eq!(restored.origin, VersionOrigin::Restore);
    assert_eq!(restored.source_hash, f.doc.source_hash);
    assert_ne!(restored.version_id, a);
    let c = f.change("C");
    assert_eq!((c.seq, c.parent_version_id), (4, Some(restored.version_id)));
    let same = f.restore(c.version_id).unwrap();
    assert_eq!(same.seq, 5);
    assert_ne!(same.version_id, c.version_id);
    assert_eq!(same.parent_version_id, Some(c.version_id));
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"C");
    assert_eq!(f.library.version_snapshot(a).unwrap().bytes(), b"A c'1");
}

#[test]
fn dirty_restore_and_wrong_discard_capability_never_overwrite_files() {
    let f = Fixture::new();
    let target = f.doc.head_version_id.unwrap();
    let edit = f.begin();
    let hash = f.write("B");
    assert_eq!(f.restore(target).unwrap_err().code, ErrorCode::EditBusy);
    assert_eq!(
        f.library
            .discard(f.doc.document_id, None, f.doc.head_version_id, &hash)
            .unwrap_err()
            .code,
        ErrorCode::EditBusy
    );
    assert_eq!(
        f.library
            .discard(
                f.doc.document_id,
                Some(EditId::new()),
                f.doc.head_version_id,
                &hash
            )
            .unwrap_err()
            .code,
        ErrorCode::EditBusy
    );
    f.library.end_edit(edit.edit_id).unwrap();
    assert_eq!(
        f.restore(target).unwrap_err().code,
        ErrorCode::DirtyWorkspace
    );
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"B");
}

#[test]
fn concurrent_checks_keep_newer_render_and_snapshot_when_old_result_arrives_last() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash_b = f.write("B");
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let e = entered.clone();
    let r = release.clone();
    *f.engine.hook.lock().unwrap() = Some(Box::new(move |_| {
        e.wait();
        r.wait();
    }));
    thread::scope(|scope| {
        let older = scope.spawn(|| f.check(&edit, &hash_b));
        entered.wait();
        let hash_c = f.write("C");
        let newer = f.check(&edit, &hash_c);
        assert!(newer.current);
        release.wait();
        let older = older.join().unwrap();
        assert!(!older.current);
        assert_eq!(
            f.library
                .job_snapshot(older.job.job_id)
                .unwrap()
                .unwrap()
                .bytes(),
            b"B"
        );
        let doc = f.library.get_document(f.doc.document_id).unwrap();
        assert_eq!(doc.last_successful_render.unwrap().source_hash, hash_c);
        f.write("D");
        assert_eq!(
            f.library
                .get_document(f.doc.document_id)
                .unwrap()
                .last_successful_render
                .unwrap()
                .source_hash,
            hash_c
        );
        f.write("C");
        f.commit(&edit, &hash_c).unwrap();
    });
}

#[test]
fn different_documents_can_compile_without_holding_database_lock() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("B");
    let other = f
        .library
        .import(
            &f.engine,
            ImportRequest {
                title: "other",
                input: b"other",
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            &RunContext::default(),
        )
        .unwrap()
        .document
        .unwrap();
    let other_edit = f
        .library
        .begin_edit(BeginEditRequest {
            document_id: other.document_id,
            view_id: ViewId::new(),
            base_version_id: other.head_version_id,
            expected_source_hash: &other.source_hash,
            author: "test",
            adopt_workspace: false,
        })
        .unwrap();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let e = entered.clone();
    let r = release.clone();
    *f.engine.hook.lock().unwrap() = Some(Box::new(move |_| {
        e.wait();
        r.wait();
    }));
    thread::scope(|scope| {
        let first = scope.spawn(|| f.check(&edit, &hash));
        entered.wait();
        let result = f
            .library
            .check(
                &f.engine,
                CheckRequest {
                    expect: None,
                    reference: None,
                    document_id: other.document_id,
                    edit_id: Some(other_edit.edit_id),
                    expected_source_hash: &other.source_hash,
                    options: &CompileOptions::default(),
                },
                &RunContext::default(),
            )
            .unwrap();
        assert!(result.current);
        release.wait();
        first.join().unwrap();
    });
}

#[test]
fn restart_preserves_edits_jobs_and_requires_explicit_capability_resume() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("B");
    let checked = f.check(&edit, &hash);
    let snapshot = SourceSnapshot::new(f.doc.document_id, b"B".to_vec()).unwrap();
    let queued = f
        .library
        .create_job("preview", Some(f.doc.document_id), Some(&snapshot))
        .unwrap();
    let running = f
        .library
        .create_job("preview", Some(f.doc.document_id), Some(&snapshot))
        .unwrap();
    f.library
        .transition_job(
            running.job_id,
            JobState::Queued,
            JobState::Running,
            "compile",
            None,
            None,
        )
        .unwrap();
    let home = f.library.home().to_owned();
    drop(f.library);
    let library = Library::open(&home).unwrap();
    assert_eq!(
        library.get_document(f.doc.document_id).unwrap().edit_state,
        DocumentEditState::Interrupted
    );
    assert_eq!(
        library.get_job(queued.job_id).unwrap().state,
        JobState::Interrupted
    );
    assert_eq!(
        library.get_job(running.job_id).unwrap().state,
        JobState::Interrupted
    );
    assert_eq!(
        library.get_job(checked.job.job_id).unwrap().state,
        JobState::Done
    );
    assert_eq!(
        library
            .job_snapshot(queued.job_id)
            .unwrap()
            .unwrap()
            .bytes(),
        b"B"
    );
    assert_eq!(
        library
            .commit(CommitRequest {
                edit_id: edit.edit_id,
                base_version_id: edit.base_version_id,
                expected_source_hash: &hash,
                summary: "bad"
            })
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    assert_eq!(
        library
            .resume_edit(EditId::new(), ViewId::new(), edit.base_version_id, &hash)
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let resumed = library
        .resume_edit(edit.edit_id, ViewId::new(), edit.base_version_id, &hash)
        .unwrap();
    assert_ne!(resumed.owner_view_id, edit.owner_view_id);
    assert_eq!(
        library
            .commit(CommitRequest {
                edit_id: edit.edit_id,
                base_version_id: edit.base_version_id,
                expected_source_hash: &hash,
                summary: "recheck"
            })
            .unwrap_err()
            .code,
        ErrorCode::CompileFailed
    );
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"B");
}

#[test]
fn job_state_changes_are_conditional_and_terminals_are_immutable() {
    let f = Fixture::new();
    let job = f.library.create_job("test", None, None).unwrap();
    let progress = f
        .library
        .transition_job(
            job.job_id,
            JobState::Queued,
            JobState::Queued,
            "waiting",
            None,
            None,
        )
        .unwrap();
    assert_eq!(progress.phase, "waiting");
    assert_eq!(
        f.library
            .transition_job(
                job.job_id,
                JobState::Running,
                JobState::Done,
                "done",
                None,
                None
            )
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    f.library
        .transition_job(
            job.job_id,
            JobState::Queued,
            JobState::Cancelled,
            "cancelled",
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        f.library
            .transition_job(
                job.job_id,
                JobState::Queued,
                JobState::Running,
                "run",
                None,
                None
            )
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    assert_eq!(
        f.library
            .transition_job(
                job.job_id,
                JobState::Cancelled,
                JobState::Running,
                "run",
                None,
                None
            )
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    let edit = f.begin();
    let hash = f.write("B");
    *f.engine.error.lock().unwrap() = Some(AppError::new(ErrorCode::Cancelled, "cancelled"));
    let error = f
        .library
        .check(
            &f.engine,
            CheckRequest {
                expect: None,
                reference: None,
                document_id: f.doc.document_id,
                edit_id: Some(edit.edit_id),
                expected_source_hash: &hash,
                options: &CompileOptions::default(),
            },
            &RunContext::default(),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    let id = serde_json::from_value(error.detail.unwrap()["jobId"].clone()).unwrap();
    assert_eq!(f.library.get_job(id).unwrap().state, JobState::Cancelled);
}

#[test]
fn queued_checks_execute_the_persisted_snapshot_after_later_saves() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("queued B");
    let job = f
        .library
        .queue_check(CheckRequest {
            expect: None,
            reference: None,
            document_id: f.doc.document_id,
            edit_id: Some(edit.edit_id),
            expected_source_hash: &hash,
            options: &CompileOptions::default(),
        })
        .unwrap();
    assert_eq!(job.state, JobState::Queued);
    assert!(!serde_json::to_string(&job)
        .unwrap()
        .contains(&edit.edit_id.to_string()));
    f.write("later C");
    let result = f
        .library
        .run_check(&f.engine, job.job_id, &RunContext::default())
        .unwrap();
    assert_eq!(result.job.job_id, job.job_id);
    assert_eq!(result.render.source_hash, hash);
    assert!(!result.current);
    assert_eq!(
        f.library.job_snapshot(job.job_id).unwrap().unwrap().bytes(),
        b"queued B"
    );
    assert_eq!(
        f.library
            .run_check(&f.engine, job.job_id, &RunContext::default())
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    let state = f.library.get_document(f.doc.document_id).unwrap();
    assert_eq!(
        state.last_successful_render.unwrap().source_hash,
        f.doc.source_hash
    );
}

#[test]
fn source_replacement_missing_file_and_symlink_escape_are_detected() {
    let f = Fixture::source("\u{feff}中文😀\r\nA\r\n");
    let bytes = fs::read(&f.doc.source_path).unwrap();
    assert_eq!(bytes, "\u{feff}中文😀\r\nA\r\n".as_bytes());
    let path = Path::new(&f.doc.source_path);
    fs::remove_file(path).unwrap();
    assert_eq!(
        f.library
            .observe_source(f.doc.document_id)
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    fs::write(path, &bytes).unwrap();
    assert!(!f.library.observe_source(f.doc.document_id).unwrap().dirty);
    #[cfg(unix)]
    {
        fs::remove_file(path).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"external").unwrap();
        std::os::unix::fs::symlink(outside.path(), path).unwrap();
        assert_eq!(
            f.library
                .observe_source(f.doc.document_id)
                .unwrap_err()
                .code,
            ErrorCode::StorageError
        );
        assert_eq!(fs::read(outside.path()).unwrap(), b"external");
    }
}
