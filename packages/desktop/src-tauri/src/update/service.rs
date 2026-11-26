//! Update state machine, ownership and install hand-off.

use super::{
    feed::{self, Endpoints, Release},
    github,
    notes::{self, NotesCache},
    stage::{self, Download},
    target, version, Error, Result,
};
use ossia_types::{
    UpdateChannel, UpdateChannelPreference, UpdateSnapshot, UpdateSource, UpdateState,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
};

type Sink = Arc<dyn Fn(&UpdateSnapshot) + Send + Sync>;

mod config;
mod install;
pub use install::InstallOutcome;

#[cfg(test)]
#[path = "../../tests/unit/update_service.rs"]
mod tests;

struct ActiveDownload {
    cancel: tokio::sync::watch::Sender<bool>,
    done: tokio::sync::watch::Receiver<bool>,
}

struct Staged {
    path: PathBuf,
    version: String,
}

const OSSIA_UPDATE_FEED: &str = "https://ossia-releases.maisow.com";

#[derive(Clone)]
pub struct UpdateService {
    snapshot: Arc<Mutex<UpdateSnapshot>>,
    debug_build: bool,
    endpoints: Arc<Endpoints>,
    build: Arc<str>,
    notes_path: Arc<PathBuf>,
    status_home: Option<Arc<PathBuf>>,
    notes_cache: Arc<Mutex<NotesCache>>,
    release: Arc<Mutex<Option<Release>>>,
    staged: Arc<Mutex<Option<Staged>>>,
    download: Arc<Mutex<Option<ActiveDownload>>>,
    installing: Arc<AtomicBool>,
    install_settled: Arc<(Mutex<()>, Condvar)>,
    announce: Arc<Mutex<Option<Sink>>>,
}

impl UpdateService {
    pub fn new(home: &Path) -> Self {
        let notes_path = notes::notes_path(home);
        let cache = notes::load(&notes_path).unwrap_or_else(|error| {
            eprintln!("ossia: cannot read update notes cache: {error}");
            NotesCache::default()
        });
        let mut service = Self::from_parts(
            Endpoints {
                github_base: Arc::from(github::API_BASE),
                github_repository: Arc::from(github::REPOSITORY),
                ossia_base: Arc::from(OSSIA_UPDATE_FEED),
            },
            version::build_stamp(),
            notes_path,
            cache,
            cfg!(debug_assertions),
        );
        service.status_home = Some(Arc::new(home.to_owned()));
        service
    }

    #[cfg(test)]
    pub fn for_build(base: &str, repository: &str, build: &str) -> Self {
        Self::for_test_build(base, repository, build, false)
    }

    #[cfg(test)]
    pub fn for_debug_build(base: &str, repository: &str, build: &str) -> Self {
        Self::for_test_build(base, repository, build, true)
    }

    #[cfg(test)]
    fn for_test_build(base: &str, repository: &str, build: &str, debug_build: bool) -> Self {
        let notes_path = std::env::temp_dir()
            .join("ossia-update-tests")
            .join(format!("{}.json", uuid::Uuid::new_v4()));
        Self::from_parts(
            Endpoints {
                github_base: Arc::from(base),
                github_repository: Arc::from(repository),
                ossia_base: Arc::from(base),
            },
            build,
            notes_path,
            NotesCache::default(),
            debug_build,
        )
    }

    fn from_parts(
        endpoints: Endpoints,
        build: &str,
        notes_path: PathBuf,
        cache: NotesCache,
        debug_build: bool,
    ) -> Self {
        let channel = version::effective_channel(UpdateChannelPreference::Auto, build);
        let current = (!debug_build && version::is_released(build)).then_some(build);
        let retained = if debug_build || cache.channel == Some(channel) {
            notes::select(&cache, current, None).map(Box::new)
        } else {
            None
        };
        Self {
            snapshot: Arc::new(Mutex::new(UpdateSnapshot {
                state: UpdateState::Idle,
                is_debug_build: debug_build,
                current: build.to_string(),
                source: UpdateSource::Ossia,
                channel,
                latest: None,
                error: None,
                done: None,
                total: None,
                notes: retained,
                staged: false,
            })),
            debug_build,
            endpoints: Arc::new(endpoints),
            build: Arc::from(build),
            notes_path: Arc::new(notes_path),
            status_home: None,
            notes_cache: Arc::new(Mutex::new(cache)),
            release: Arc::new(Mutex::new(None)),
            staged: Arc::new(Mutex::new(None)),
            download: Arc::new(Mutex::new(None)),
            installing: Arc::new(AtomicBool::new(false)),
            install_settled: Arc::new((Mutex::new(()), Condvar::new())),
            announce: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_debug_build(&self) -> bool {
        self.debug_build
    }

    pub fn is_updatable(&self) -> bool {
        !self.debug_build && version::is_released(&self.build)
    }

    pub fn snapshot(&self) -> UpdateSnapshot {
        let mut snapshot = self.snapshot.lock().unwrap().clone();
        snapshot.staged = self.staged().is_some();
        snapshot
    }

    pub fn is_installing(&self) -> bool {
        self.installing.load(Ordering::SeqCst)
    }

    pub fn is_in_flight(&self) -> bool {
        self.snapshot.lock().unwrap().state.in_flight()
    }

    pub fn wait_for_install(&self) {
        let mut guard = self.install_settled.0.lock().unwrap();
        while self.installing.load(Ordering::SeqCst) {
            guard = self.install_settled.1.wait(guard).unwrap();
        }
    }

    /// Finish an explicit install in progress, or discard an unused download.
    pub fn finish_on_quit(&self) {
        if self.is_installing() {
            self.wait_for_install();
        } else {
            self.discard_staged();
        }
    }

    fn clear_installing(&self) {
        let _guard = self.install_settled.0.lock().unwrap();
        self.installing.store(false, Ordering::SeqCst);
        self.install_settled.1.notify_all();
    }

    pub fn staged(&self) -> Option<PathBuf> {
        self.staged
            .lock()
            .unwrap()
            .as_ref()
            .map(|staged| staged.path.clone())
    }

    fn staged_version(&self) -> Option<String> {
        self.staged
            .lock()
            .unwrap()
            .as_ref()
            .map(|staged| staged.version.clone())
    }

    pub fn announce(&self, sink: impl Fn(&UpdateSnapshot) + Send + Sync + 'static) {
        *self.announce.lock().unwrap() = Some(Arc::new(sink));
    }

    fn notify(&self) {
        let sink = self.announce.lock().unwrap().clone();
        if let Some(sink) = sink {
            let snapshot = self.snapshot();
            sink(&snapshot);
        }
    }

    fn set(&self, next: UpdateSnapshot) {
        *self.snapshot.lock().unwrap() = next;
        self.notify();
    }

    fn clear_agent_status(&self) {
        if let Some(home) = &self.status_home {
            if let Err(error) = ossia_backend::update_status::clear(home) {
                eprintln!("ossia: cannot clear update status: {error}");
            }
        }
    }

    fn publish_agent_status(&self, snapshot: &UpdateSnapshot) {
        if let Some(home) = &self.status_home {
            if let Err(error) = ossia_backend::update_status::write(home, snapshot) {
                eprintln!("ossia: cannot save update status: {error}");
            }
        }
    }

    fn base_snapshot(&self) -> UpdateSnapshot {
        self.snapshot()
    }

    fn fail(&self, error: &Error) -> UpdateSnapshot {
        UpdateSnapshot {
            state: UpdateState::Error,
            error: Some(error.to_string()),
            ..self.base_snapshot()
        }
    }

    fn fail_check(&self, error: &Error) -> UpdateSnapshot {
        if self.staged().is_some() {
            return UpdateSnapshot {
                state: UpdateState::Ready,
                error: Some(error.to_string()),
                ..self.base_snapshot()
            };
        }
        self.fail(error)
    }

    fn begin(&self) -> bool {
        self.begin_with(false)
    }

    fn begin_with(&self, installing: bool) -> bool {
        {
            let mut snapshot = self.snapshot.lock().unwrap();
            if snapshot.state.in_flight() {
                return false;
            }
            if installing {
                self.installing.store(true, Ordering::SeqCst);
            }
            snapshot.state = UpdateState::Checking;
            snapshot.error = None;
            snapshot.done = None;
            snapshot.total = None;
        }
        self.notify();
        true
    }

    fn discard_staged(&self) {
        if let Some(staged) = self.staged.lock().unwrap().take() {
            if let Some(dir) = staged.path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    pub async fn check(
        &self,
        source: UpdateSource,
        token: Option<&str>,
        channel: UpdateChannel,
        auto_download: bool,
    ) -> UpdateSnapshot {
        self.check_inner(source, token, channel, auto_download, false)
            .await
    }

    pub async fn check_notes(
        &self,
        source: UpdateSource,
        token: Option<&str>,
        channel: UpdateChannel,
    ) -> UpdateSnapshot {
        self.check_inner(source, token, channel, false, true).await
    }

    async fn check_inner(
        &self,
        source: UpdateSource,
        token: Option<&str>,
        channel: UpdateChannel,
        auto_download: bool,
        notes_only: bool,
    ) -> UpdateSnapshot {
        if !notes_only && !self.debug_build && !self.is_updatable() {
            let mut snapshot = self.snapshot.lock().unwrap();
            snapshot.state = UpdateState::Source;
            snapshot.source = source;
            snapshot.channel = channel;
            snapshot.latest = None;
            snapshot.error = None;
            snapshot.done = None;
            snapshot.total = None;
            drop(snapshot);
            self.publish_agent_status(&self.snapshot());
            self.notify();
            return self.snapshot();
        }
        if !self.begin() {
            return self.snapshot();
        }
        {
            let mut snapshot = self.snapshot.lock().unwrap();
            snapshot.source = source;
            snapshot.channel = channel;
        }
        let feed_channel = if notes_only && self.debug_build {
            UpdateChannel::Beta
        } else {
            channel
        };
        let endpoints = self.endpoints.clone();
        let target = target::current();
        let current = self
            .is_updatable()
            .then(|| self.build.to_string())
            .filter(|_| !notes_only);
        let token_owned = token.map(str::to_string);
        let fetched = tokio::task::spawn_blocking(move || {
            feed::releases(
                &endpoints,
                source,
                token_owned.as_deref(),
                current.as_deref(),
                feed_channel,
                target,
            )
        })
        .await;
        let settled = match fetched {
            Ok(Ok(releases)) if releases.is_empty() => self.settle_nothing(channel),
            Ok(Ok(releases)) => {
                let cache = match notes::remember(&self.notes_path, source, feed_channel, &releases)
                {
                    Ok(cache) => cache,
                    Err(error) => {
                        eprintln!("ossia: cannot cache update notes: {error}");
                        NotesCache::default()
                    }
                };
                *self.notes_cache.lock().unwrap() = cache.clone();
                self.settle(
                    releases[0].clone(),
                    &cache,
                    channel,
                    auto_download,
                    notes_only,
                )
            }
            Ok(Err(error)) => self.fail_check(&error),
            Err(error) => self.fail_check(&Error::other(format!("update check: {error}"))),
        };
        if !notes_only && !self.debug_build {
            let current = self.snapshot.lock().unwrap();
            if current.source == source && current.channel == channel {
                self.publish_agent_status(&settled);
            }
        }
        if settled.state == UpdateState::Found && auto_download {
            self.set(UpdateSnapshot {
                state: UpdateState::Checking,
                ..settled
            });
            if let Some(prepared) = self.prepare_claimed_download(token) {
                return self.finish_download(prepared).await;
            }
        } else {
            self.set(settled);
        }
        self.snapshot()
    }

    fn settle_nothing(&self, channel: UpdateChannel) -> UpdateSnapshot {
        if self.staged().is_some() {
            return UpdateSnapshot {
                state: UpdateState::Ready,
                channel,
                error: None,
                ..self.base_snapshot()
            };
        }
        *self.release.lock().unwrap() = None;
        UpdateSnapshot {
            state: UpdateState::NoRelease,
            channel,
            latest: None,
            error: None,
            ..self.base_snapshot()
        }
    }

    fn settle(
        &self,
        release: Release,
        cache: &NotesCache,
        channel: UpdateChannel,
        auto_download: bool,
        notes_only: bool,
    ) -> UpdateSnapshot {
        let latest = release.version.clone();
        let current = self.build.to_string();
        if !self.is_updatable() && (!self.debug_build || notes_only) {
            return UpdateSnapshot {
                state: UpdateState::Source,
                channel,
                latest: None,
                error: None,
                notes: notes::select(cache, None, Some(&latest)).map(Box::new),
                ..self.base_snapshot()
            };
        }
        if let Some(downloaded) = self.staged_version() {
            if version::is_newer(&downloaded, &latest) {
                self.discard_staged();
            } else if latest == downloaded || !auto_download {
                return UpdateSnapshot {
                    state: UpdateState::Ready,
                    channel,
                    latest: Some(if latest == downloaded {
                        latest.clone()
                    } else {
                        downloaded.clone()
                    }),
                    error: None,
                    notes: notes::select(
                        cache,
                        (!self.debug_build).then_some(current.as_str()),
                        Some(&downloaded),
                    )
                    .map(Box::new)
                    .or_else(|| self.snapshot().notes),
                    ..self.base_snapshot()
                };
            } else {
                self.discard_staged();
            }
        }
        let state = if self.debug_build || version::is_newer(&latest, &current) {
            UpdateState::Found
        } else {
            UpdateState::Latest
        };
        *self.release.lock().unwrap() = Some(release);
        UpdateSnapshot {
            state,
            channel,
            latest: Some(latest.clone()),
            error: None,
            notes: notes::select(
                cache,
                (!self.debug_build).then_some(current.as_str()),
                Some(&latest),
            )
            .map(Box::new),
            ..self.base_snapshot()
        }
    }

    fn prepare_claimed_download(&self, token: Option<&str>) -> Option<Download> {
        let target = match target::current() {
            Some(target) => target,
            None => {
                self.set(self.fail(&Error::other("this install cannot update itself in place")));
                return None;
            }
        };
        let release = self.release.lock().unwrap().clone();
        let Some(release) = release else {
            self.set(self.fail(&Error::other("no release to download yet")));
            return None;
        };
        let previous = self.staged.lock().unwrap().take().map(|staged| staged.path);
        let prepared = match stage::prepare(&release, target, token, previous.as_deref()) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.set(self.fail(&error));
                return None;
            }
        };
        Some(prepared)
    }

    pub async fn cancel_download(&self) -> UpdateSnapshot {
        let done = {
            let active = self.download.lock().unwrap();
            active.as_ref().map(|active| {
                let _ = active.cancel.send(true);
                active.done.clone()
            })
        };
        if let Some(mut done) = done {
            while !*done.borrow_and_update() {
                if done.changed().await.is_err() {
                    break;
                }
            }
        }
        self.snapshot()
    }

    async fn finish_download(&self, prepared: Download) -> UpdateSnapshot {
        let version = prepared.version.clone();
        let dir = prepared.path.parent().map(Path::to_path_buf);
        let (cancel, mut cancelled) = tokio::sync::watch::channel(false);
        let (done, done_rx) = tokio::sync::watch::channel(false);
        *self.download.lock().unwrap() = Some(ActiveDownload {
            cancel,
            done: done_rx,
        });
        self.set(UpdateSnapshot {
            state: UpdateState::Downloading,
            done: Some(0),
            total: Some(prepared.asset.size),
            error: None,
            ..self.base_snapshot()
        });
        let outcome = tokio::select! {
            biased;
            _ = cancelled.changed() => None,
            result = prepared.run(&self.snapshot) => Some(result),
        };
        let mut active = self.download.lock().unwrap();
        let was_cancelled = *cancelled.borrow();
        let settled = if was_cancelled || outcome.is_none() {
            if let Some(dir) = dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            UpdateSnapshot {
                state: UpdateState::Idle,
                latest: None,
                error: None,
                done: None,
                total: None,
                ..self.base_snapshot()
            }
        } else {
            match outcome.unwrap() {
                Ok(path) => {
                    *self.staged.lock().unwrap() = Some(Staged { path, version });
                    UpdateSnapshot {
                        state: UpdateState::Ready,
                        error: None,
                        ..self.base_snapshot()
                    }
                }
                Err(error) => self.fail(&error),
            }
        };
        *self.snapshot.lock().unwrap() = settled.clone();
        *active = None;
        drop(active);
        self.notify();
        let _ = done.send(true);
        settled
    }
}
