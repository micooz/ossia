mod common;

use common::{wait_job, Fixture, SOURCE};
use ossia_runtime::*;
use std::sync::atomic::Ordering;

async fn fixture() -> Fixture {
    let f = Fixture::new(true).await;
    f.engine.interaction.store(true, Ordering::Release);
    assert_eq!(
        wait_job(&f.runtime, f.check(f.doc.source_hash.clone()).job_id)
            .await
            .state,
        JobState::Done
    );
    f
}

fn choose(f: &Fixture, targets: Vec<SelectionTarget>) -> DocumentContext {
    f.runtime
        .selection(Selection {
            document_id: f.doc.document_id,
            source_hash: f
                .runtime
                .context(f.doc.document_id)
                .unwrap()
                .document
                .source_hash,
            targets,
        })
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn element_selection_is_authoritative_and_independent_of_measure_geometry() {
    let f = fixture().await;
    let context = f.runtime.context(f.doc.document_id).unwrap();
    let geometry = f
        .runtime
        .geometry(&context.render.unwrap().render_key)
        .unwrap();
    assert!(geometry.pages[0].regions.is_empty());
    let note = geometry.pages[0]
        .elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead"))
        .unwrap();
    let selected = choose(
        &f,
        vec![note.region.target.clone(), note.region.target.clone()],
    );
    assert_eq!(selected.selection.as_ref().unwrap().targets.len(), 1);
    assert_eq!(selected.regions[0].occurrences.len(), 2);
    assert_eq!(
        ossia_score::SourceMap::new(std::str::from_utf8(SOURCE).unwrap())
            .slice(selected.regions[0].spans[0])
            .unwrap(),
        "c'1"
    );
    let error = f
        .runtime
        .selection(Selection {
            document_id: f.doc.document_id,
            source_hash: f.doc.source_hash.clone(),
            targets: vec![SelectionTarget::Element {
                element_id: "forged".into(),
            }],
        })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    let clef = geometry.pages[0]
        .elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("Clef"))
        .unwrap();
    let selected = choose(&f, vec![clef.region.target.clone()]);
    assert!(selected.regions[0].spans.is_empty());
    let anchor = selected.regions[0].insertion_anchor.unwrap();
    assert_eq!(SOURCE[anchor.byte_start - 1], b'{');
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn element_targets_remap_and_only_deleted_or_ambiguous_targets_are_cleared() {
    let f = fixture().await;
    let context = f.runtime.context(f.doc.document_id).unwrap();
    let geometry = f
        .runtime
        .geometry(&context.render.unwrap().render_key)
        .unwrap();
    let notes: Vec<_> = geometry.pages[0]
        .elements
        .iter()
        .filter(|e| e.region.element_kind.as_deref() == Some("NoteHead") && e.bounds.y0 == 20.)
        .map(|e| e.region.target.clone())
        .collect();
    choose(
        &f,
        vec![
            SelectionTarget::Measure {
                locator: Locator {
                    staff_index: 0,
                    measure_start: 1,
                    measure_end: 1,
                    voice: None,
                },
            },
            notes[0].clone(),
            notes[1].clone(),
        ],
    );
    let annotated = format!("% annotation\n{}", std::str::from_utf8(SOURCE).unwrap());
    let hash = f.save(annotated.as_bytes());
    let retained = f.runtime.context(f.doc.document_id).unwrap();
    assert_eq!(retained.selection.as_ref().unwrap().targets.len(), 3);
    assert_ne!(retained.selection.as_ref().unwrap().targets[1], notes[0]);
    assert!(retained.regions[1].occurrences.is_empty());
    assert_eq!(
        wait_job(&f.runtime, f.check(hash).job_id).await.state,
        JobState::Done
    );
    assert_eq!(
        f.runtime.context(f.doc.document_id).unwrap().regions[1]
            .occurrences
            .len(),
        2
    );
    f.save(annotated.replace("c'1", "e'1").as_bytes());
    let retained = f.runtime.context(f.doc.document_id).unwrap();
    assert_eq!(retained.selection.unwrap().targets.len(), 1);
    assert_eq!(retained.regions[0].label, "NoteHead: d'1");
    f.save(
        annotated
            .replace("c'1", "e'1")
            .replace("d'1", "d'1 d'1")
            .as_bytes(),
    );
    assert!(f
        .runtime
        .context(f.doc.document_id)
        .unwrap()
        .selection
        .is_none());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn old_renders_refresh_once_without_creating_a_version() {
    let f = Fixture::new(true).await;
    f.engine.legacy_render.store(true, Ordering::Release);
    assert_eq!(
        wait_job(&f.runtime, f.check(f.doc.source_hash.clone()).job_id)
            .await
            .state,
        JobState::Done
    );
    f.engine.legacy_render.store(false, Ordering::Release);
    let jobs = f.runtime.library.list_jobs().unwrap().len();
    let versions = f.runtime.library.history(f.doc.document_id).unwrap().len();
    f.runtime.context(f.doc.document_id).unwrap();
    f.runtime.context(f.doc.document_id).unwrap();
    let queued = f.runtime.library.list_jobs().unwrap();
    assert_eq!(queued.len(), jobs + 1);
    assert_eq!(
        wait_job(&f.runtime, queued.last().unwrap().job_id)
            .await
            .state,
        JobState::Done
    );
    assert_eq!(
        f.runtime
            .context(f.doc.document_id)
            .unwrap()
            .render
            .unwrap()
            .manifest
            .options_hash,
        ossia_engine::render_options_hash(true)
    );
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap().len(),
        versions
    );
    f.finish().await;
}
