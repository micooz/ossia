mod common;

use common::{wait_job, Fixture};
use ossia_runtime::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn view_ownership_stale_selections_and_explicit_commit() {
    let f = Fixture::new(true).await;
    let other = f
        .runtime
        .open_view(Some(f.doc.document_id))
        .unwrap()
        .view_id;
    let selection = Selection {
        view_id: f.view,
        document_id: f.doc.document_id,
        source_hash: f.doc.source_hash.clone(),
        locators: vec![Locator {
            staff_index: 0,
            measure_start: 1,
            measure_end: 1,
            voice: None,
        }],
    };
    f.runtime.selection(selection.clone()).unwrap();
    assert!(f.runtime.context(other).unwrap().selection.is_none());
    let edit = f.begin();
    let changed = f.save(b"\\score { \\new Staff { e'1 | d'1 | } \\layout {} }");
    assert!(f.runtime.context(f.view).unwrap().selection.is_none());
    assert_eq!(
        f.runtime.selection(selection).unwrap_err().code,
        ErrorCode::StaleSelection
    );
    let args = CheckArgs {
        expect: None,
        document_id: Some(f.doc.document_id),
        view_id: other,
        edit_id: Some(edit.edit_id),
        expected_source_hash: changed.clone(),
        wait_ms: None,
    };
    assert_eq!(
        f.runtime.check(args, false).unwrap_err().code,
        ErrorCode::Forbidden
    );
    let job = f
        .runtime
        .check(
            CheckArgs {
                expect: None,
                document_id: Some(f.doc.document_id),
                view_id: f.view,
                edit_id: Some(edit.edit_id),
                expected_source_hash: changed.clone(),
                wait_ms: None,
            },
            false,
        )
        .unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    let version = f
        .runtime
        .commit(CommitArgs {
            view_id: f.view,
            edit_id: edit.edit_id,
            expected_source_hash: changed,
            summary: "raise note".into(),
        })
        .unwrap();
    assert_eq!(version.seq, 2);
    assert!(!f.runtime.context(f.view).unwrap().document.unwrap().dirty);
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn snapshots_replay_events_and_detect_expired_instance_or_cursor() {
    let f = Fixture::new(false).await;
    let snapshot = f.runtime.context(f.view).unwrap();
    let other = f.runtime.open_view(None).unwrap().view_id;
    f.runtime
        .select_document(other, Some(f.doc.document_id))
        .unwrap();
    f.runtime.select_document(f.view, None).unwrap();
    let events = f
        .runtime
        .events(f.view, &snapshot.instance_id, snapshot.seq)
        .unwrap();
    assert!(events.iter().all(|e| e.view_id != Some(other)));
    assert!(events.iter().any(|e| e.r#type == "view-changed"));
    assert_eq!(
        f.runtime
            .events(f.view, "previous-instance", snapshot.seq)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    for _ in 0..520 {
        f.runtime.emit("tick", None, None, json!({}));
    }
    assert!(f
        .runtime
        .events(f.view, &snapshot.instance_id, snapshot.seq)
        .is_err());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn language_exchange_and_global_updates_reach_every_view() {
    let f = Fixture::new(false).await;
    let other = f.runtime.open_view(None).unwrap();
    f.runtime.initialize_ui_language(Some(UiLanguage::ZhCn));
    let opened = f.runtime.open_view(None).unwrap();
    assert_eq!(
        f.runtime.exchange(&opened.ticket).unwrap().language,
        Some(UiLanguage::ZhCn)
    );
    let cursor = f.runtime.agent_status().unwrap().seq;
    let changed = ipc::dispatch(&f.runtime, "ui-language", json!({"language": "en-US"})).unwrap();
    assert_eq!(changed, json!("en-US"));
    let events = f
        .runtime
        .events(f.view, &f.runtime.info.instance_id, cursor)
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.r#type == "ui-language-changed")
        .unwrap();
    assert_eq!(event.payload, json!({"language": "en-US"}));
    assert!(event.view_id.is_none());
    assert!(f
        .runtime
        .events(other.view_id, &f.runtime.info.instance_id, cursor)
        .unwrap()
        .iter()
        .any(|event| event.r#type == "ui-language-changed"));
    ipc::dispatch(
        &f.runtime,
        "watch-parent",
        json!({"pid": std::process::id()}),
    )
    .unwrap();
    assert_eq!(f.runtime.parent_pid(), Some(std::process::id()));
    let opened = f.runtime.open_view(None).unwrap();
    assert_eq!(
        f.runtime.exchange(&opened.ticket).unwrap().language,
        Some(UiLanguage::EnUs)
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn browser_version_renders_are_immutable_and_edit_summaries_omit_credentials() {
    let f = Fixture::new(true).await;
    let edit = f.begin();
    let snapshot = f.runtime.context(f.view).unwrap();
    let activity = snapshot.edit_activity.unwrap();
    assert_eq!(activity.author, "test-agent");
    assert!(activity.owned_by_view);
    let other = f
        .runtime
        .open_view(Some(f.doc.document_id))
        .unwrap()
        .view_id;
    let public = serde_json::to_string(&f.runtime.context(other).unwrap()).unwrap();
    assert!(!public.contains(&edit.edit_id.to_string()));
    assert!(
        !f.runtime
            .context(other)
            .unwrap()
            .edit_activity
            .unwrap()
            .owned_by_view
    );
    let changed = f.save(b"\\score { { g'1 } }");
    let before = f.runtime.context(f.view).unwrap().document.unwrap();
    let job = f
        .runtime
        .render_version(f.doc.document_id, f.doc.head_version_id.unwrap())
        .unwrap();
    assert_eq!(job.source_hash, Some(f.doc.source_hash.clone()));
    let job = wait_job(&f.runtime, job.job_id).await;
    assert_eq!(job.state, JobState::Done);
    let reference: RenderReference = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(reference.manifest.source_hash, f.doc.source_hash);
    let geometry = f.runtime.geometry(&reference.render_key).unwrap();
    assert_eq!(geometry.source_hash, f.doc.source_hash);
    assert!(geometry.pages.iter().all(|p| p.regions.is_empty()));
    let after = f.runtime.context(f.view).unwrap().document.unwrap();
    assert_eq!(after, before);
    assert_eq!(after.source_hash, changed);
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap().len(),
        1
    );
    assert!(f
        .runtime
        .render_version(DocumentId::new(), f.doc.head_version_id.unwrap())
        .is_err());
    assert!(f.runtime.geometry(&ContentHash::of(b"missing")).is_err());
    assert!(f.runtime.export_location(job.job_id).is_err());
    f.finish().await;
}
