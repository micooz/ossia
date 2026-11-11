use super::*;
use notify::{RecursiveMode, Watcher};
use serde_json::json;
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

impl Runtime {
    pub async fn watch(self: Arc<Self>) -> Result<()> {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let root = self.library.home().join("documents");
        std::fs::create_dir_all(&root).map_err(storage)?;
        let native = self.start_native_watcher(&root, sender.clone())?;
        // Native event streams may lose changes; a content poll also detects same-timestamp saves
        // and rename/recreation.
        let mut polling = notify::PollWatcher::new(
            move |event| {
                let _ = sender.send(event);
            },
            notify::Config::default()
                .with_poll_interval(Duration::from_secs(1))
                .with_compare_contents(true),
        )
        .map_err(storage)?;
        polling
            .watch(&root, RecursiveMode::Recursive)
            .map_err(storage)?;
        let _native = native;
        let mut saves: HashMap<DocumentId, Instant> = HashMap::new();
        loop {
            tokio::select! {
                event = receiver.recv() => {
                    if let Some(event) = event {
                        match event {
                            Ok(event) if !matches!(event.kind, notify::EventKind::Access(_)) => {
                                for path in event.paths {
                                    if let Some(id) = watched_document(&root, &path) {
                                        saves.insert(id, Instant::now());
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(error) => self.emit(
                                "watch-error",
                                None,
                                None,
                                json!({"message": error.to_string()}),
                            ),
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
            if self.stopping.load(Ordering::Acquire) {
                break;
            }
            let ready: Vec<_> = saves
                .iter()
                .filter(|(_, time)| time.elapsed() >= Duration::from_millis(300))
                .map(|(id, _)| *id)
                .collect();
            for id in ready {
                saves.remove(&id);
                self.reconcile_document(id)?;
            }
        }
        Ok(())
    }

    fn start_native_watcher(
        &self,
        root: &Path,
        sender: tokio::sync::mpsc::UnboundedSender<notify::Result<notify::Event>>,
    ) -> Result<Option<notify::RecommendedWatcher>> {
        let watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        });
        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                self.emit(
                    "watch-fallback",
                    None,
                    None,
                    json!({"message": error.to_string(), "mode": "content-poll"}),
                );
                return Ok(None);
            }
        };
        if let Err(error) = watcher.watch(root, RecursiveMode::Recursive) {
            self.emit(
                "watch-fallback",
                None,
                None,
                json!({"message": error.to_string(), "mode": "content-poll"}),
            );
            return Ok(None);
        }
        Ok(Some(watcher))
    }

    fn reconcile_document(&self, id: DocumentId) -> Result<()> {
        let doc = match self.observe(id) {
            Ok(doc) => doc,
            Err(error) => {
                self.emit("source-error", Some(id), None, json!({"error": error}));
                return Ok(());
            }
        };
        if self
            .state()
            .suppressed_previews
            .remove(&id)
            .is_some_and(|hash| hash == doc.source_hash)
        {
            return Ok(());
        }
        let (pending_match, running) = {
            let state = self.state();
            (
                state.pending.iter().any(|p| {
                    p.job.document_id == Some(id)
                        && p.job.source_hash.as_ref() == Some(&doc.source_hash)
                }),
                state.running.keys().copied().collect::<Vec<_>>(),
            )
        };
        let running_match = running.iter().any(|job| {
            self.library.get_job(*job).ok().is_some_and(|job| {
                job.document_id == Some(id) && job.source_hash.as_ref() == Some(&doc.source_hash)
            })
        });
        let has_current_render = doc
            .last_successful_render
            .as_ref()
            .is_some_and(|render| render.source_hash == doc.source_hash);
        if !pending_match && !running_match && !has_current_render {
            let _ = self.check(
                CheckArgs {
                    expect: None,
                    reference: None,
                    document_id: Some(id),
                    edit_id: None,
                    expected_source_hash: doc.source_hash,
                    wait_ms: None,
                },
                true,
            );
        }
        Ok(())
    }
}

fn watched_document(root: &Path, path: &Path) -> Option<DocumentId> {
    let relative = path.strip_prefix(root).ok()?;
    relative
        .components()
        .next()?
        .as_os_str()
        .to_str()?
        .parse()
        .ok()
}
