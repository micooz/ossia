use ossia_engine::RunContext;
use ossia_library::*;
use std::{fs, path::Path};

struct TestEngine;

impl EngineBackend for TestEngine {
    fn build_id(&self) -> &str {
        "trash-test-engine"
    }

    fn convert(
        &self,
        input: &[u8],
        _: InputFormat,
        _: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport> {
        context.check()?;
        let source = String::from_utf8(input.to_vec()).unwrap();
        Ok(ImportReport {
            status: ImportStatus::Clean,
            input_hash: ContentHash::of(input),
            engine_build_id: self.build_id().into(),
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
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest> {
        context.check()?;
        let bytes = format!("<svg>{}</svg>", snapshot.source_hash()).into_bytes();
        let hash = ContentHash::of(&bytes);
        let relative = format!("assets/{hash}.svg");
        fs::create_dir_all(artifact_root.join("assets")).unwrap();
        fs::write(artifact_root.join(&relative), &bytes).unwrap();
        Ok(RenderManifest {
            source_hash: snapshot.source_hash().clone(),
            engine_build_id: self.build_id().into(),
            options_hash: ContentHash::of(&serde_json::to_vec(options).unwrap()),
            pages: vec![RenderPage {
                page: 1,
                artifact: ArtifactRef {
                    path: relative,
                    sha256: hash,
                    bytes: bytes.len() as u64,
                },
            }],
            pdf: None,
            midi: None,
            diagnostics: vec![],
            format_errors: vec![],
        })
    }
}

fn import(library: &Library, engine: &TestEngine, title: &str, source: &[u8]) -> DocumentState {
    library
        .import(
            engine,
            ImportRequest {
                title,
                input: source,
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            &RunContext::default(),
        )
        .unwrap()
        .document
        .unwrap()
}

#[test]
fn trash_survives_restart_and_purge_leaves_shared_content_and_external_files_alone() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::NamedTempFile::new().unwrap();
    fs::write(external.path(), b"external output").unwrap();
    let engine = TestEngine;
    let source = b"A c'1";
    let doc;
    let other;
    let edit_id;
    {
        let library = Library::open(root.path()).unwrap();
        doc = import(&library, &engine, "first", source);
        other = import(&library, &engine, "second", source);
        let edit = library
            .begin_edit(BeginEditRequest {
                document_id: doc.document_id,
                base_version_id: doc.head_version_id,
                expected_source_hash: &doc.source_hash,
                author: "test",
                adopt_workspace: false,
            })
            .unwrap();
        edit_id = edit.edit_id;
        fs::write(&doc.source_path, b"dirty source").unwrap();
        let moved = library
            .trash(doc.document_id, &ContentHash::of(b"dirty source"))
            .unwrap();
        assert_eq!(moved.title, "first");
        assert_eq!(
            library.get_document(doc.document_id).unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(library.list_documents().unwrap().len(), 1);
        assert_eq!(
            library
                .history_page(doc.document_id, 10, 0)
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            library
                .version_snapshot(doc.head_version_id.unwrap())
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            library.get_edit(edit_id).unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(library.trash_page(10, 0).unwrap().items.len(), 1);
        library.storage_cleanup(StorageCacheLimit::Gib1).unwrap();
        assert_eq!(fs::read(&doc.source_path).unwrap(), b"dirty source");
    }

    {
        let library = Library::open(root.path()).unwrap();
        assert_eq!(library.trash_page(10, 0).unwrap().items.len(), 1);
        let restored = library.untrash(doc.document_id).unwrap();
        assert!(restored.dirty);
        assert_eq!(restored.edit_state, DocumentEditState::Idle);
        assert_eq!(library.get_edit(edit_id).unwrap().status, EditStatus::Ended);
        assert_eq!(fs::read(&doc.source_path).unwrap(), b"dirty source");
        assert_eq!(library.history(doc.document_id).unwrap().len(), 1);
        library
            .trash(doc.document_id, &restored.source_hash)
            .unwrap();
        library.purge(doc.document_id).unwrap();
        assert!(!Path::new(&doc.source_path).parent().unwrap().exists());
        assert_eq!(library.list_documents().unwrap().len(), 1);
        assert_eq!(
            library.get_document(other.document_id).unwrap().source_hash,
            ContentHash::of(source)
        );
        assert_eq!(library.history(other.document_id).unwrap().len(), 1);
        library.storage_cleanup(StorageCacheLimit::Gib1).unwrap();
        assert_eq!(
            library
                .version_snapshot(other.head_version_id.unwrap())
                .unwrap()
                .bytes(),
            source
        );
    }
    assert_eq!(fs::read(external.path()).unwrap(), b"external output");
}

#[test]
fn version_one_library_migrates_in_place() {
    let root = tempfile::tempdir().unwrap();
    let engine = TestEngine;
    let doc;
    {
        let library = Library::open(root.path()).unwrap();
        doc = import(&library, &engine, "existing", b"A c'1");
    }
    {
        let conn = rusqlite::Connection::open(root.path().join("library.sqlite")).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=OFF;
             DROP TABLE purge_intents;
             ALTER TABLE documents DROP COLUMN trash_source_hash;
             ALTER TABLE documents DROP COLUMN deleted_at;
             PRAGMA user_version=1;",
        )
        .unwrap();
    }
    let library = Library::open(root.path()).unwrap();
    assert_eq!(library.list_documents().unwrap().len(), 1);
    assert!(library.trash_page(10, 0).unwrap().items.is_empty());
    library.trash(doc.document_id, &doc.source_hash).unwrap();
    assert_eq!(library.trash_page(10, 0).unwrap().items.len(), 1);
}

#[test]
fn interrupted_purge_cleanup_finishes_on_restart() {
    let root = tempfile::tempdir().unwrap();
    drop(Library::open(root.path()).unwrap());
    let document = DocumentId::new();
    let document_root = root.path().join("documents").join(document.to_string());
    fs::create_dir_all(&document_root).unwrap();
    fs::write(document_root.join("score.ly"), b"orphaned purge").unwrap();
    let source_path = format!("documents/{document}/score.ly");
    let conn = rusqlite::Connection::open(root.path().join("library.sqlite")).unwrap();
    conn.execute(
        "INSERT INTO purge_intents(document_id,source_path,created_at) VALUES (?1,?2,?3)",
        rusqlite::params![document.to_string(), source_path, "2026-11-13T00:00:00Z"],
    )
    .unwrap();
    drop(conn);

    let library = Library::open(root.path()).unwrap();
    assert!(!document_root.exists());
    let conn = rusqlite::Connection::open(root.path().join("library.sqlite")).unwrap();
    let pending: i64 = conn
        .query_row("SELECT count(*) FROM purge_intents", [], |row| row.get(0))
        .unwrap();
    assert_eq!(pending, 0);
    drop(conn);
    drop(library);
}
