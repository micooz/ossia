use super::*;
use std::time::Duration;

impl Runtime {
    pub fn storage_status(&self) -> Result<StorageStatus> {
        let limit = crate::preferences::load(self.library.home())?.storage_cache_limit;
        let mut status = self.library.storage_status(limit)?;
        let state = self.state();
        status.idle = state.pending.is_empty() && state.running.is_empty();
        Ok(status)
    }

    /// Run a policy cleanup. The runtime must be idle so cache files cannot race a compilation.
    pub fn storage_cleanup(&self) -> Result<StorageCleanupReport> {
        self.available()?;
        let state = self.state();
        if !state.pending.is_empty() || !state.running.is_empty() {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "storage cleanup requires an idle runtime",
            ));
        }
        if self.maintenance.swap(true, Ordering::AcqRel) {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "storage cleanup is already running",
            ));
        }
        drop(state);
        *self
            .last_maintenance
            .lock()
            .expect("maintenance clock poisoned") = Instant::now();
        let result = (|| {
            let manifests = self.view_manifests()?;
            let limit = crate::preferences::load(self.library.home())?.storage_cache_limit;
            self.library.storage_cleanup(limit, &manifests)
        })();
        self.maintenance.store(false, Ordering::Release);
        self.changed.notify_one();
        result
    }

    pub(crate) fn maybe_cleanup_storage(self: &Arc<Self>) {
        if self.stopping.load(Ordering::Acquire)
            || self.maintenance.load(Ordering::Acquire)
            || !self.automatic_cleanup_enabled()
            || self
                .last_maintenance
                .lock()
                .expect("maintenance clock poisoned")
                .elapsed()
                < Duration::from_secs(60 * 60)
        {
            return;
        }
        let busy = {
            let state = self.state();
            !state.pending.is_empty() || !state.running.is_empty()
        };
        if busy {
            return;
        }
        let runtime = self.clone();
        tokio::spawn(async move {
            if !runtime.automatic_cleanup_enabled() {
                return;
            }
            let worker = runtime.clone();
            let result = tokio::task::spawn_blocking(move || worker.storage_cleanup()).await;
            if let Ok(Err(error)) = result {
                if error.code != ErrorCode::Conflict {
                    runtime.emit("storage-error", None, None, json!({"error":error}));
                }
            }
        });
    }

    pub async fn maintain_storage(self: Arc<Self>) {
        loop {
            self.wait_maintenance(Duration::from_secs(6 * 60 * 60))
                .await;
            if self.stopping.load(Ordering::Acquire) {
                break;
            }
            if !self.automatic_cleanup_enabled() {
                continue;
            }
            loop {
                if !self.automatic_cleanup_enabled() {
                    break;
                }
                let runtime = self.clone();
                let result = tokio::task::spawn_blocking(move || runtime.storage_cleanup()).await;
                match result {
                    Ok(Ok(_)) => break,
                    Ok(Err(error)) if error.code == ErrorCode::Conflict => {
                        if self.stopping.load(Ordering::Acquire) {
                            break;
                        }
                        self.wait_maintenance(Duration::from_secs(5 * 60)).await;
                    }
                    Ok(Err(error)) => {
                        self.emit("storage-error", None, None, json!({"error":error}));
                        break;
                    }
                    Err(error) => {
                        self.emit("storage-error", None, None, json!({"error":storage(error)}));
                        break;
                    }
                }
            }
            if self.stopping.load(Ordering::Acquire) {
                break;
            }
        }
    }

    async fn wait_maintenance(&self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while !self.stopping.load(Ordering::Acquire) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            tokio::time::sleep(remaining.min(Duration::from_secs(1))).await;
        }
    }

    fn view_manifests(&self) -> Result<Vec<RenderManifest>> {
        let documents = {
            let state = self.state();
            state
                .views
                .values()
                .filter_map(|view| view.document)
                .collect::<std::collections::HashSet<_>>()
        };
        let mut manifests = Vec::new();
        for document in documents {
            if let Some(manifest) = self.library.get_document(document)?.last_successful_render {
                manifests.push(manifest);
            }
        }
        Ok(manifests)
    }

    fn automatic_cleanup_enabled(&self) -> bool {
        crate::preferences::load(self.library.home())
            .is_ok_and(|preferences| preferences.storage_auto_cleanup)
    }
}
