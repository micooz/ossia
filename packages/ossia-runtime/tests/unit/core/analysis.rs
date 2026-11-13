use super::*;
use ossia_engine::CompileOptions;
use std::path::Path;

struct Engine;
impl EngineBackend for Engine {
    fn build_id(&self) -> &str {
        "analysis-test"
    }
    fn convert(
        &self,
        _: &[u8],
        _: InputFormat,
        _: Option<&ContentHash>,
        _: &RunContext,
    ) -> Result<ImportReport> {
        unreachable!()
    }
    fn compile(
        &self,
        _: &SourceSnapshot,
        _: &CompileOptions,
        _: &Path,
        _: &RunContext,
    ) -> Result<RenderManifest> {
        unreachable!()
    }
}

#[test]
fn analysis_is_reused_and_bounded_by_snapshot_and_render_identity() {
    let root = tempfile::tempdir().unwrap();
    let runtime = Runtime::new(
        Library::open(root.path()).unwrap(),
        Arc::new(Engine),
        "http://localhost:3211".into(),
    );
    let snapshot = SourceSnapshot::new(DocumentId::new(), b"{ c'1 | }".to_vec()).unwrap();
    let first = runtime.score_index(&snapshot);
    assert!(Arc::ptr_eq(&first, &runtime.score_index(&snapshot)));
    let mut manifest = RenderManifest {
        source_hash: snapshot.source_hash().clone(),
        engine_build_id: "test".into(),
        options_hash: ContentHash::of(b"default"),
        pages: vec![],
        pdf: None,
        midi: None,
        diagnostics: vec![],
        format_errors: vec![],
    };
    let layout = runtime.score_layout(&snapshot, &manifest).unwrap();
    assert!(Arc::ptr_eq(
        &layout,
        &runtime.score_layout(&snapshot, &manifest).unwrap()
    ));
    for n in 0..CACHE_ENTRIES {
        let other = SourceSnapshot::new(
            snapshot.document_id(),
            format!("{{ c'1 | }} % {n}").into_bytes(),
        )
        .unwrap();
        runtime.score_index(&other);
        manifest.options_hash = ContentHash::of(n.to_string().as_bytes());
        runtime.score_layout(&snapshot, &manifest).unwrap();
    }
    let cache = runtime.analysis.lock().unwrap();
    assert_eq!(cache.indexes.len(), CACHE_ENTRIES);
    assert_eq!(cache.layouts.len(), CACHE_ENTRIES);
    drop(cache);
    manifest.options_hash = ContentHash::of(b"default");
    assert!(!Arc::ptr_eq(
        &layout,
        &runtime.score_layout(&snapshot, &manifest).unwrap()
    ));
    manifest.source_hash = ContentHash::of(b"changed");
    assert_eq!(
        runtime.score_layout(&snapshot, &manifest).unwrap_err().code,
        ErrorCode::StaleSelection
    );
}
