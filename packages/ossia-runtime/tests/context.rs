mod common;

use common::{wait_job, Fixture, SOURCE};
use ossia_engine::RunContext;
use ossia_library::ImportRequest;
use ossia_runtime::*;
use serde_json::json;

fn selection(fixture: &Fixture) -> Selection {
    Selection {
        document_id: fixture.doc.document_id,
        source_hash: fixture.doc.source_hash.clone(),
        targets: vec![SelectionTarget::Measure {
            locator: Locator {
                staff_index: 0,
                measure_start: 1,
                measure_end: 1,
                voice: None,
            },
        }],
    }
}

fn measure(staff_index: usize, measure: u32) -> SelectionTarget {
    SelectionTarget::Measure {
        locator: Locator {
            staff_index,
            measure_start: measure,
            measure_end: measure,
            voice: None,
        },
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selection_events_report_target_count() {
    let f = Fixture::new(false).await;
    let cursor = f.runtime.agent_status().unwrap().seq;
    let mut selected = selection(&f);
    selected.targets = vec![measure(0, 1), measure(0, 2)];
    f.runtime.selection(selected).unwrap();
    let events = f
        .runtime
        .events(Some(f.doc.document_id), &f.runtime.info.instance_id, cursor)
        .unwrap();
    let selection_event = events
        .iter()
        .find(|event| event.r#type == "selection-changed")
        .unwrap();
    assert_eq!(selection_event.payload["targetCount"], 2);

    let cursor = f.runtime.agent_status().unwrap().seq;
    f.runtime.clear_selection(f.doc.document_id).unwrap();
    let events = f
        .runtime
        .events(Some(f.doc.document_id), &f.runtime.info.instance_id, cursor)
        .unwrap();
    let selection_event = events
        .iter()
        .find(|event| event.r#type == "selection-changed")
        .unwrap();
    assert_eq!(selection_event.payload["targetCount"], 0);
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selections_are_shared_by_document_and_cleared_when_source_changes() {
    let f = Fixture::new(true).await;
    let second = f
        .runtime
        .library
        .import(
            f.engine.as_ref(),
            ImportRequest {
                title: "second",
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
    f.runtime.selection(selection(&f)).unwrap();
    assert!(f
        .runtime
        .context(f.doc.document_id)
        .unwrap()
        .selection
        .is_some());
    assert!(f
        .runtime
        .context(second.document_id)
        .unwrap()
        .selection
        .is_none());
    let changed = f.save(b"\\score { \\new Staff { e'1 | d'1 | } \\layout {} }");
    assert!(f
        .runtime
        .context(f.doc.document_id)
        .unwrap()
        .selection
        .is_none());
    assert_eq!(
        f.runtime.selection(selection(&f)).unwrap_err().code,
        ErrorCode::StaleSelection
    );
    assert_eq!(
        f.runtime
            .context(f.doc.document_id)
            .unwrap()
            .document
            .source_hash,
        changed
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn annotations_preserve_selection_while_structural_music_changes_clear_it() {
    let f = Fixture::new(true).await;
    f.runtime.selection(selection(&f)).unwrap();
    let cursor = f.runtime.agent_status().unwrap().seq;
    let annotated = f.save(
        b"\\version \"2.26.0\"\n\\score { \\new Staff { c'1^\\markup { \"I\" } | d'1 | } \\layout {} }\n",
    );
    let context = f.runtime.context(f.doc.document_id).unwrap();
    let events = f
        .runtime
        .events(Some(f.doc.document_id), &f.runtime.info.instance_id, cursor)
        .unwrap();
    let selection_event = events
        .iter()
        .find(|event| event.r#type == "selection-changed")
        .unwrap();
    assert_eq!(selection_event.payload["targetCount"], 1);
    assert_eq!(context.document.source_hash, annotated);
    assert_eq!(context.selection.as_ref().unwrap().source_hash, annotated);
    assert_eq!(
        context.selection.as_ref().unwrap().targets,
        selection(&f).targets
    );
    let cursor = f.runtime.agent_status().unwrap().seq;
    let changed =
        f.save(b"\\version \"2.26.0\"\n\\score { \\new Staff { e'1 | d'1 | } \\layout {} }\n");
    let context = f.runtime.context(f.doc.document_id).unwrap();
    let events = f
        .runtime
        .events(Some(f.doc.document_id), &f.runtime.info.instance_id, cursor)
        .unwrap();
    let selection_event = events
        .iter()
        .find(|event| event.r#type == "selection-changed")
        .unwrap();
    assert_eq!(selection_event.payload["targetCount"], 0);
    assert_eq!(context.document.source_hash, changed);
    assert!(context.selection.is_none());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn events_are_runtime_wide_or_filtered_to_one_document() {
    let f = Fixture::new(false).await;
    let second = f
        .runtime
        .library
        .import(
            f.engine.as_ref(),
            ImportRequest {
                title: "second",
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
    let cursor = f.runtime.agent_status().unwrap().seq;
    f.runtime.emit("global", None, None, json!({}));
    f.runtime
        .emit("first", Some(f.doc.document_id), None, json!({}));
    f.runtime
        .emit("second", Some(second.document_id), None, json!({}));
    let all = f
        .runtime
        .events(None, &f.runtime.info.instance_id, cursor)
        .unwrap();
    assert_eq!(all.len(), 3);
    let first = f
        .runtime
        .events(Some(f.doc.document_id), &f.runtime.info.instance_id, cursor)
        .unwrap();
    assert!(first.iter().any(|event| event.r#type == "global"));
    assert!(first.iter().any(|event| event.r#type == "first"));
    assert!(first.iter().all(|event| event.r#type != "second"));
    assert_eq!(
        f.runtime
            .events(Some(f.doc.document_id), "previous-instance", cursor)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn context_reports_edit_activity_without_credentials_and_version_renders_are_immutable() {
    let f = Fixture::new(true).await;
    let edit = f.begin();
    let context = f.runtime.context(f.doc.document_id).unwrap();
    let activity = context.edit_activity.as_ref().unwrap();
    assert_eq!(activity.author, "test-agent");
    let public = serde_json::to_string(&context).unwrap();
    assert!(!public.contains(&edit.edit_id.to_string()));
    let changed = f.save(b"\\score { { g'1 } }");
    let before = f.runtime.context(f.doc.document_id).unwrap().document;
    let job = f
        .runtime
        .render_version(f.doc.document_id, f.doc.head_version_id.unwrap())
        .unwrap();
    assert_eq!(job.source_hash, Some(f.doc.source_hash.clone()));
    let job = wait_job(&f.runtime, job.job_id).await;
    assert_eq!(job.state, JobState::Done);
    let reference: RenderReference = serde_json::from_value(job.result.unwrap()).unwrap();
    let geometry = f.runtime.geometry(&reference.render_key).unwrap();
    assert_eq!(geometry.source_hash, f.doc.source_hash);
    let after = f.runtime.context(f.doc.document_id).unwrap().document;
    assert_eq!(after, before);
    assert_eq!(after.source_hash, changed);
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap().len(),
        1
    );
    f.finish().await;
}
