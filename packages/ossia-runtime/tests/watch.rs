mod common;

use common::{until, Fixture};
use ossia_runtime::*;
use std::{path::Path, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_watcher_events_are_reconciled_and_failed_checks_keep_old_render() {
    let f = Fixture::new(true).await;
    let broken = f.save(b"BROKEN");
    let context = f.runtime.context(f.doc.document_id).unwrap();
    assert_eq!(context.document.source_hash, broken);
    let job = f.check(broken);
    assert_eq!(
        common::wait_job(&f.runtime, job.job_id).await.state,
        ossia_runtime::JobState::Error
    );
    assert_eq!(
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .unwrap()
            .source_hash,
        f.doc.source_hash
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watcher_debounces_atomic_saves_and_recovers_deleted_file() {
    let f = Fixture::new(true).await;
    let watcher = tokio::spawn(f.runtime.clone().watch());
    tokio::time::sleep(Duration::from_millis(100)).await;
    if watcher.is_finished() {
        panic!("watcher exited: {:?}", watcher.await);
    }
    for i in 0..4 {
        let path = Path::new(&f.doc.source_path).with_file_name("save.tmp");
        std::fs::write(&path, format!("\\score {{ {{ c'1 }} }} % {i}")).unwrap();
        std::fs::rename(path, &f.doc.source_path).unwrap();
    }
    let hash = ContentHash::of(&std::fs::read(&f.doc.source_path).unwrap());
    until(|| {
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .is_some_and(|r| r.source_hash == hash)
    })
    .await;
    std::fs::remove_file(&f.doc.source_path).unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let hash = f.save(b"\\score { { g'1 } }");
    until(|| {
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .is_some_and(|r| r.source_hash == hash)
    })
    .await;
    f.runtime.stop().unwrap();
    watcher.await.unwrap().unwrap();
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn write_source_can_suppress_the_automatic_preview() {
    let f = Fixture::new(false).await;
    let watcher = tokio::spawn(f.runtime.clone().watch());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let edit = f.begin();
    let source = "\\version \"2.26.0\"\n\\score { \\new Staff { e'1 | } \\layout {} }\n";
    let before = f.runtime.library.list_jobs().unwrap().len();
    let written = f
        .runtime
        .write_source(WriteSourceArgs {
            edit_id: edit.edit_id,
            expected_source_hash: f.doc.source_hash.clone(),
            change: SourceWriteChange::Replace {
                text: source.into(),
            },
            preview: false,
        })
        .unwrap();
    assert_eq!(written.source_hash, ContentHash::of(source.as_bytes()));
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(f.runtime.library.list_jobs().unwrap().len(), before);
    watcher.abort();
    f.finish().await;
}
