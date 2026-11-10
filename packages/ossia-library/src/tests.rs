use super::*;
use std::{
    process::Command,
    sync::{Arc, Barrier},
    thread,
};

type CompileHook = Box<dyn FnOnce(&SourceSnapshot) + Send>;
#[derive(Default)]
struct TestEngine {
    hook: Mutex<Option<CompileHook>>,
    error: Mutex<Option<AppError>>,
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
            measures: 1,
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
    fn hook(
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
fn initialization_schema_and_writer_lock_are_isolated() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("db.sqlite"), b"unrelated bytes").unwrap();
    let library = Library::open(root.path()).unwrap();
    assert!(root.path().join(storage::DATABASE).is_file());
    assert_eq!(
        fs::read(root.path().join("db.sqlite")).unwrap(),
        b"unrelated bytes"
    );
    drop(library);
    let other = tempfile::tempdir().unwrap();
    let library = Library::open(other.path()).unwrap();
    assert_eq!(
        Library::open(other.path()).err().unwrap().code,
        ErrorCode::LibraryBusy
    );
    let conn = library.db().unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        conn.pragma_query_value(None, "foreign_keys", |r| r.get::<_, i32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.pragma_query_value(None, "synchronous", |r| r.get::<_, i32>(0))
            .unwrap(),
        2
    );
    assert!(rusqlite::version_number() >= 3_051_003);
    drop(conn);
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::process_fixture", "--ignored"])
        .env("OSSIA_LIBRARY_TEST_MODE", "lock")
        .env("OSSIA_LIBRARY_TEST_HOME", other.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    drop(library);
    let conn = Connection::open(other.path().join(storage::DATABASE)).unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    drop(conn);
    assert_eq!(
        Library::open(other.path()).err().unwrap().code,
        ErrorCode::RuntimeIncompatible
    );
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
fn external_save_after_final_commit_check_remains_a_dirty_workspace() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("B");
    f.check(&edit, &hash);
    let path = f.doc.source_path.clone();
    f.hook("after-commit-check", move |_| {
        fs::write(path, b"C").unwrap();
        Ok(())
    });
    let version = f.commit(&edit, &hash).unwrap();
    assert_eq!(
        f.library
            .version_snapshot(version.version_id)
            .unwrap()
            .bytes(),
        b"B"
    );
    let doc = f.library.get_document(f.doc.document_id).unwrap();
    assert!(doc.dirty);
    assert_eq!(doc.source_hash, ContentHash::of(b"C"));
    assert_eq!(doc.edit_state, DocumentEditState::Idle);
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
fn cas_and_database_failures_never_publish_unverified_versions() {
    let f = Fixture::new();
    f.hook("after-cas", |_| Err(storage_error("injected")));
    assert!(f
        .library
        .import(
            &f.engine,
            ImportRequest {
                title: "interrupted",
                input: b"unpublished",
                format: InputFormat::Ly,
                confirmation: None,
                author: "test"
            },
            &RunContext::default()
        )
        .is_err());
    assert_eq!(f.library.list_documents().unwrap().len(), 1);
    let edit = f.begin();
    let hash = f.write("B");
    f.check(&edit, &hash);
    f.hook("before-version-commit", |_| Err(storage_error("injected")));
    assert!(f.commit(&edit, &hash).is_err());
    assert_eq!(f.library.history(f.doc.document_id).unwrap().len(), 1);
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"B");
    assert_eq!(
        f.library
            .get_document(f.doc.document_id)
            .unwrap()
            .edit_state,
        DocumentEditState::Active
    );
    let version = f.commit(&edit, &hash).unwrap();
    fs::write(
        f.library
            .store
            .path(&format!("contents/{}.gz", version.source_hash))
            .unwrap(),
        b"corrupt",
    )
    .unwrap();
    assert_eq!(
        f.library
            .version_snapshot(version.version_id)
            .unwrap_err()
            .code,
        ErrorCode::StorageError
    );
}

#[test]
fn restore_faults_recover_by_actual_file_hash_without_duplicate_nodes() {
    for (point, expected, count) in [
        ("after-intent", b"B".as_slice(), 2),
        ("after-replace", b"A c'1".as_slice(), 3),
        ("before-intent-finalize", b"A c'1".as_slice(), 3),
        ("after-intent-finalize", b"A c'1".as_slice(), 3),
    ] {
        let f = Fixture::new();
        f.change("B");
        f.hook(point, |_| Err(storage_error("crash")));
        assert!(f.restore(f.doc.head_version_id.unwrap()).is_err());
        let home = f.library.home().to_owned();
        drop(f.library);
        for _ in 0..2 {
            let library = Library::open(&home).unwrap();
            assert_eq!(fs::read(&f.doc.source_path).unwrap(), expected);
            assert_eq!(library.history(f.doc.document_id).unwrap().len(), count);
            assert!(!library.get_document(f.doc.document_id).unwrap().dirty);
        }
    }
}

#[test]
fn recovery_conflict_preserves_external_file_until_explicit_acknowledgement() {
    let f = Fixture::new();
    f.change("B");
    f.hook("after-replace", |_| Err(storage_error("crash")));
    assert!(f.restore(f.doc.head_version_id.unwrap()).is_err());
    let hash = f.write("external C");
    let home = f.library.home().to_owned();
    drop(f.library);
    let library = Library::open(&home).unwrap();
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"external C");
    assert_eq!(library.history(f.doc.document_id).unwrap().len(), 2);
    assert_eq!(
        library.get_document(f.doc.document_id).unwrap().edit_state,
        DocumentEditState::RecoveryConflict
    );
    assert_eq!(
        library
            .resolve_recovery_conflict(f.doc.document_id, &f.doc.source_hash)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    library
        .resolve_recovery_conflict(f.doc.document_id, &hash)
        .unwrap();
    assert!(library.get_document(f.doc.document_id).unwrap().dirty);
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"external C");
}

#[test]
fn discard_recovery_ends_edit_only_when_replacement_occurred() {
    for (point, expected, status) in [
        (
            "after-intent",
            b"B".as_slice(),
            DocumentEditState::Interrupted,
        ),
        (
            "after-replace",
            b"A c'1".as_slice(),
            DocumentEditState::Idle,
        ),
    ] {
        let f = Fixture::new();
        let edit = f.begin();
        let hash = f.write("B");
        f.hook(point, |_| Err(storage_error("crash")));
        assert!(f
            .library
            .discard(
                f.doc.document_id,
                Some(edit.edit_id),
                f.doc.head_version_id,
                &hash
            )
            .is_err());
        let home = f.library.home().to_owned();
        drop(f.library);
        let library = Library::open(&home).unwrap();
        assert_eq!(fs::read(&f.doc.source_path).unwrap(), expected);
        assert_eq!(
            library.get_document(f.doc.document_id).unwrap().edit_state,
            status
        );
        assert_eq!(library.history(f.doc.document_id).unwrap().len(), 1);
    }
}

#[test]
fn process_death_releases_lock_and_recovers_durable_restore_intent() {
    for (point, expected, count) in [
        ("after-intent", b"B".as_slice(), 2),
        ("after-replace", b"A c'1".as_slice(), 3),
        ("before-intent-finalize", b"A c'1".as_slice(), 3),
    ] {
        let f = Fixture::new();
        f.change("B");
        let home = f.library.home().to_owned();
        drop(f.library);
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::process_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("OSSIA_LIBRARY_TEST_MODE", point)
            .env("OSSIA_LIBRARY_TEST_HOME", &home)
            .env(
                "OSSIA_LIBRARY_TEST_TARGET",
                f.doc.head_version_id.unwrap().to_string(),
            )
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let library = Library::open(&home).unwrap();
        assert_eq!(fs::read(&f.doc.source_path).unwrap(), expected);
        assert_eq!(library.history(f.doc.document_id).unwrap().len(), count);
        assert!(!library.get_document(f.doc.document_id).unwrap().dirty);
    }
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
    atomic_write(path, &bytes).unwrap();
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

#[test]
fn queued_checks_execute_the_persisted_snapshot_after_later_saves() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("queued B");
    let job = f
        .library
        .queue_check(CheckRequest {
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
fn converted_compile_failure_and_replacement_conflict_preserve_existing_documents() {
    let f = Fixture::new();
    *f.engine.error.lock().unwrap() = Some(AppError::new(
        ErrorCode::CompileFailed,
        "conversion compiled unsuccessfully",
    ));
    assert!(f
        .library
        .import(
            &f.engine,
            ImportRequest {
                title: "converted failure",
                input: b"xml",
                format: InputFormat::MusicXml,
                confirmation: None,
                author: "test"
            },
            &RunContext::default()
        )
        .is_err());
    assert_eq!(f.library.list_documents().unwrap().len(), 1);
    f.change("B");
    let path = f.doc.source_path.clone();
    f.hook("after-intent", move |_| {
        fs::write(path, b"external").unwrap();
        Ok(())
    });
    assert_eq!(
        f.restore(f.doc.head_version_id.unwrap()).unwrap_err().code,
        ErrorCode::Conflict
    );
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"external");
    assert_eq!(f.library.history(f.doc.document_id).unwrap().len(), 2);
    assert_eq!(
        f.library
            .get_document(f.doc.document_id)
            .unwrap()
            .edit_state,
        DocumentEditState::RecoveryConflict
    );
}
