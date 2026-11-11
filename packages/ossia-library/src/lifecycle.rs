use crate::db;
use crate::storage::{self, Store};
use crate::*;
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

impl Library {
    pub fn open(home: &Path) -> Result<Self> {
        storage::preflight(home)?;
        storage::ensure_directory(home)?;
        let store = Store {
            home: home.canonicalize().map_err(storage_error)?,
        };
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(store.path("library.lock")?)
            .map_err(storage_error)?;
        lock.try_lock_exclusive().map_err(|e| {
            let busy = e.kind() == std::io::ErrorKind::WouldBlock
                || (cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33)));
            if busy {
                AppError::new(ErrorCode::LibraryBusy, "another process owns this library")
            } else {
                storage_error(e)
            }
        })?;
        if !storage::preflight(&store.home)? {
            storage::initialize(&store)?;
        }
        let conn = Connection::open(store.path(storage::DATABASE)?).map_err(storage_error)?;
        let schema: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(storage_error)?;
        if schema != SCHEMA_VERSION {
            return Err(AppError::new(
                ErrorCode::RuntimeIncompatible,
                "unsupported library schema",
            ));
        }
        let _: String = conn
            .query_row("SELECT library_id FROM library_meta", [], |r| r.get(0))
            .map_err(storage_error)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage_error)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .map_err(storage_error)?;
        let library = Self {
            store,
            conn: Mutex::new(conn),
            _lock: lock,
            #[cfg(test)]
            hooks: Mutex::default(),
        };
        library.recover()?;
        Ok(library)
    }

    pub fn home(&self) -> &Path {
        &self.store.home
    }

    pub fn artifact_root(&self) -> Result<PathBuf> {
        self.store.path("artifacts")
    }

    pub(crate) fn db(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| storage_error("library database mutex poisoned"))
    }

    pub(crate) fn hit(&self, point: &'static str) -> Result<()> {
        #[cfg(test)]
        {
            let hook = self.hooks.lock().unwrap().remove(point);
            if let Some(hook) = hook {
                hook(self)?;
            }
        }
        let _ = point;
        Ok(())
    }

    pub(crate) fn no_intent(&self, conn: &Connection, document: DocumentId) -> Result<()> {
        if conn
            .query_row(
                "SELECT 1 FROM worktree_intents WHERE document_id=?1",
                [document.to_string()],
                |r| r.get::<_, i32>(0),
            )
            .optional()
            .map_err(storage_error)?
            .is_some()
        {
            return Err(conflict("document has an unresolved worktree operation"));
        }
        Ok(())
    }

    pub(crate) fn snapshot(&self, doc: &db::Document) -> Result<SourceSnapshot> {
        SourceSnapshot::new(doc.id, self.store.read_source(&doc.source_path)?)
    }

    pub(crate) fn expect_snapshot(
        &self,
        doc: &db::Document,
        expected: &ContentHash,
    ) -> Result<SourceSnapshot> {
        let snapshot = self.snapshot(doc)?;
        if snapshot.source_hash() != expected {
            return Err(conflict("public source changed"));
        }
        Ok(snapshot)
    }

    pub(crate) fn expect_head(&self, doc: &db::Document, base: Option<VersionId>) -> Result<()> {
        if doc.head != base {
            Err(conflict("base version is stale"))
        } else {
            Ok(())
        }
    }
}
