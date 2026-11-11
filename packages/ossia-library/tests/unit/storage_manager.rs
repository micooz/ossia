use super::*;
use crate::test_support::Fixture;
use std::time::SystemTime;

fn large_policy() -> StoragePolicy {
    StoragePolicy {
        cache_limit_bytes: 1 << 40,
        job_retention_ms: 365 * 24 * 60 * 60 * 1000,
        render_retention_ms: 365 * 24 * 60 * 60 * 1000,
        jobs_per_document: 10_000,
        renders_per_document: 10_000,
        latest_per_document: 10_000,
        lock_retention: Duration::from_secs(7 * 24 * 60 * 60),
        scratch_retention: Duration::from_secs(60 * 60),
    }
}

#[test]
fn retention_prunes_terminal_jobs_but_keeps_newest_and_active_receipts() {
    let fixture = Fixture::new();
    let now = db::now_millis();
    for _ in 0..25 {
        let job = fixture
            .library
            .create_job("synthetic", Some(fixture.doc.document_id), None)
            .unwrap();
        fixture
            .library
            .transition_job(
                job.job_id,
                JobState::Queued,
                JobState::Running,
                "execute",
                None,
                None,
            )
            .unwrap();
        fixture
            .library
            .transition_job(
                job.job_id,
                JobState::Running,
                JobState::Done,
                "finished",
                None,
                None,
            )
            .unwrap();
    }
    {
        let conn = fixture.library.db().unwrap();
        conn.execute(
            "UPDATE jobs SET created_at=?1,updated_at=?1 WHERE kind='synthetic'",
            [now - 2 * 24 * 60 * 60 * 1000],
        )
        .unwrap();
    }
    let mut policy = large_policy();
    policy.job_retention_ms = 24 * 60 * 60 * 1000;
    policy.jobs_per_document = 5;
    policy.latest_per_document = 2;
    let report = fixture
        .library
        .storage_cleanup_with_policy(policy, &[])
        .unwrap();
    let remaining: i64 = fixture
        .library
        .db()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM jobs WHERE kind='synthetic'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(report.removed_jobs, 21);
    assert_eq!(remaining, 4);
    assert!(fixture
        .library
        .get_document(fixture.doc.document_id)
        .unwrap()
        .last_successful_render
        .is_some());
}

#[test]
fn active_edit_job_and_render_survive_retention_and_cleanup_can_still_commit() {
    let fixture = Fixture::new();
    let edit = fixture.begin();
    let hash = fixture.write("B c'1");
    fixture.check(&edit, &hash);
    let old = db::now_millis() - 365 * 24 * 60 * 60 * 1000;
    {
        let conn = fixture.library.db().unwrap();
        conn.execute("UPDATE jobs SET created_at=?1,updated_at=?1", [old])
            .unwrap();
        conn.execute("UPDATE renders SET created_at=?1,last_used_at=?1", [old])
            .unwrap();
    }
    let mut policy = large_policy();
    policy.job_retention_ms = 1;
    policy.render_retention_ms = 1;
    policy.jobs_per_document = 0;
    policy.renders_per_document = 0;
    policy.latest_per_document = 0;
    fixture
        .library
        .storage_cleanup_with_policy(policy, &[])
        .unwrap();
    fixture.commit(&edit, &hash).unwrap();
}

#[test]
fn cache_limit_evicts_lru_assets_without_breaking_the_current_preview() {
    let fixture = Fixture::new();
    let current = fixture
        .library
        .get_document(fixture.doc.document_id)
        .unwrap();
    let current_manifest = current.last_successful_render.unwrap();
    let root = fixture.library.artifact_root().unwrap();
    fs::create_dir_all(root.join("assets")).unwrap();
    let mut old_assets = Vec::new();
    for index in 0..4 {
        let bytes = vec![index as u8; 4096];
        let hash = ContentHash::of(&bytes);
        let source_hash = ContentHash::of(format!("old-source-{index}").as_bytes());
        let path = format!("assets/{hash}.svg");
        fs::write(root.join(&path), &bytes).unwrap();
        old_assets.push(path.clone());
        let manifest = RenderManifest {
            source_hash: source_hash.clone(),
            engine_build_id: format!("old-{index}"),
            options_hash: ContentHash::of(format!("old-{index}").as_bytes()),
            pages: vec![RenderPage {
                page: 1,
                artifact: ArtifactRef {
                    path,
                    sha256: hash,
                    bytes: 4096,
                },
            }],
            pdf: None,
            midi: None,
            diagnostics: vec![],
            format_errors: vec![],
        };
        let conn = fixture.library.db().unwrap();
        db::record_content(&conn, &source_hash, 32).unwrap();
        db::record_render(&conn, fixture.doc.document_id, &manifest).unwrap();
    }
    {
        let conn = fixture.library.db().unwrap();
        conn.execute(
            "UPDATE renders SET created_at=?1,last_used_at=?1 WHERE engine_build_id LIKE 'old-%'",
            [db::now_millis() - 24 * 60 * 60 * 1000],
        )
        .unwrap();
        conn.execute(
            "UPDATE renders SET last_used_at=?1 WHERE engine_build_id='test-engine-1'",
            [db::now_millis()],
        )
        .unwrap();
    }
    let mut policy = large_policy();
    policy.cache_limit_bytes = 4096;
    policy.latest_per_document = 0;
    let report = fixture
        .library
        .storage_cleanup_with_policy(policy, &[])
        .unwrap();
    assert!(report.removed_renders >= 4, "{report:?}");
    let remaining: Vec<_> = old_assets
        .iter()
        .filter(|asset| {
            fixture
                .library
                .artifact_root()
                .unwrap()
                .join(asset)
                .exists()
        })
        .collect();
    assert!(remaining.is_empty(), "old assets remain: {remaining:?}");
    let current = fixture
        .library
        .get_document(fixture.doc.document_id)
        .unwrap();
    let manifest = current.last_successful_render.unwrap();
    let key = render_key(fixture.doc.document_id, &manifest);
    fixture
        .library
        .read_render_page(&key, 1)
        .expect("current preview survives");
    assert_eq!(manifest, current_manifest);
}

#[test]
fn stale_staging_locks_docs_and_orphan_contents_are_removed() {
    let fixture = Fixture::new();
    let home = fixture.library.home();
    let old = SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
    let imports = home.join("imports/import-leftover");
    fs::create_dir_all(&imports).unwrap();
    fs::write(imports.join("score.ly"), b"unfinished").unwrap();
    let scratch = home.join("run/scratch/ossia-engine-old");
    fs::create_dir_all(&scratch).unwrap();
    fs::File::open(&scratch).unwrap().set_modified(old).unwrap();
    let locks = home.join("artifacts/locks");
    fs::create_dir_all(&locks).unwrap();
    let lock = locks.join("stale.lock");
    fs::write(&lock, b"").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&lock)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let docs = home.join("docs/lilypond/2.26.0");
    fs::create_dir_all(docs.join("current")).unwrap();
    fs::create_dir_all(docs.join("old")).unwrap();
    fs::write(
        home.join("docs/lilypond/current.json"),
        br#"{"contentHash":"current"}"#,
    )
    .unwrap();
    let orphan = fixture.library.store.put(b"orphan").unwrap();
    let orphan_path = home.join(format!("contents/{orphan}.gz"));
    assert!(orphan_path.exists());
    let report = fixture
        .library
        .storage_cleanup_with_policy(large_policy(), &[])
        .unwrap();
    assert!(!imports.exists());
    assert!(!scratch.exists());
    assert!(!lock.exists());
    assert!(!docs.join("old").exists());
    assert!(docs.join("current").exists());
    assert!(!orphan_path.exists());
    assert!(report.removed_content_objects >= 1);
}
