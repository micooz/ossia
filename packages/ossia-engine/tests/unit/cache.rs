use super::*;
use crate::cache::{self, Cache};
use crate::RunContext;
use ossia_types::RenderPage;
use std::{fs, time::Duration};

fn empty_manifest() -> RenderManifest {
    RenderManifest {
        source_hash: ContentHash::of(b"unused"),
        engine_build_id: "test-build".into(),
        options_hash: ContentHash::of(b"options"),
        pages: vec![],
        pdf: None,
        midi: None,
        diagnostics: vec![],
        format_errors: vec![],
    }
}

#[test]
fn cache_recovers_and_rejects_escape_or_corrupted_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let cache = Cache::new(temp.path()).unwrap();
    let mut manifest = empty_manifest();
    manifest.pages.push(RenderPage {
        page: 1,
        artifact: cache.asset(b"<svg/>", "svg").unwrap(),
    });
    cache.publish("key", &manifest).unwrap();
    assert_eq!(
        cache.read("key", &manifest, RenderFormat::Svg).unwrap(),
        Some(manifest.clone())
    );
    let mut mismatch = manifest.clone();
    mismatch.engine_build_id.push('2');
    assert_eq!(
        cache.read("key", &mismatch, RenderFormat::Svg).unwrap(),
        None
    );
    fs::write(
        temp.path().join(&manifest.pages[0].artifact.path),
        "damaged",
    )
    .unwrap();
    assert_eq!(
        cache.read("key", &manifest, RenderFormat::Svg).unwrap(),
        None
    );
    cache.asset(b"<svg/>", "svg").unwrap();
    for path in [
        "../outside.svg",
        "/absolute.svg",
        "C:\\outside.svg",
        "assets/../../outside",
    ] {
        let mut invalid = manifest.clone();
        invalid.pages[0].artifact.path = path.into();
        cache.publish("key", &invalid).unwrap();
        assert!(cache.read("key", &manifest, RenderFormat::Svg).is_err());
    }
    fs::write(temp.path().join("renders/key.json"), b"truncated {").unwrap();
    assert_eq!(
        cache.read("key", &manifest, RenderFormat::Svg).unwrap(),
        None
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).unwrap();
        assert!(cache::confined(temp.path(), "escape/data").is_err());
    }
    assert_ne!(
        Cache::key(
            &manifest.source_hash,
            "one",
            &manifest.options_hash,
            RenderFormat::Svg
        ),
        Cache::key(
            &manifest.source_hash,
            "two",
            &manifest.options_hash,
            RenderFormat::Svg
        )
    );
    assert_ne!(
        Cache::key(
            &manifest.source_hash,
            "one",
            &manifest.options_hash,
            RenderFormat::Svg
        ),
        Cache::key(
            &manifest.source_hash,
            "one",
            &manifest.options_hash,
            RenderFormat::Pdf
        )
    );
}

#[test]
fn cache_lock_wait_observes_deadline_and_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let cache = Cache::new(temp.path()).unwrap();
    let _guard = cache.lock("same", &RunContext::default()).unwrap();
    let context = RunContext::new(Duration::from_millis(30));
    assert!(cache.lock("same", &context).is_err());
    let context = RunContext::default();
    context.cancellation.cancel();
    assert!(cache.lock("same", &context).is_err());
}
