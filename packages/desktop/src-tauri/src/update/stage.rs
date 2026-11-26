//! One prepared asset download.

use super::{
    download,
    feed::{Asset, Release},
    target::Target,
    verify, Error, Result,
};
use ossia_types::UpdateSnapshot;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub struct Download {
    pub asset: Asset,
    pub version: String,
    pub sums_url: Option<String>,
    pub path: PathBuf,
    pub token: Option<String>,
}

pub fn prepare(
    release: &Release,
    target: Target,
    token: Option<&str>,
    previous: Option<&Path>,
) -> Result<Download> {
    let asset = release
        .asset_for(target)
        .ok_or_else(|| {
            Error::other(format!(
                "release {} has no {} asset",
                release.version,
                target.asset_name()
            ))
        })?
        .clone();
    let dir = std::env::temp_dir().join(format!("ossia-update-{}", uuid::Uuid::new_v4()));
    if let Some(dir) = previous.and_then(Path::parent) {
        let _ = std::fs::remove_dir_all(dir);
    }
    std::fs::create_dir_all(&dir)?;
    Ok(Download {
        version: release.version.clone(),
        path: dir.join(&asset.name),
        sums_url: release.sha256sums().map(|asset| asset.url.clone()),
        asset,
        token: token.map(str::to_string),
    })
}

impl Download {
    pub async fn run(self, progress: &Arc<Mutex<UpdateSnapshot>>) -> Result<PathBuf> {
        let path = self.path.clone();
        let fetched = self.fetch(progress).await;
        if fetched.is_err() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        fetched
    }

    async fn fetch(self, progress: &Arc<Mutex<UpdateSnapshot>>) -> Result<PathBuf> {
        let sums = match self.asset.digest {
            Some(_) => None,
            None => {
                let url = self
                    .sums_url
                    .as_deref()
                    .ok_or_else(|| Error::other("the release publishes no SHA256SUMS"))?;
                Some(download::text(url, self.token.as_deref()).await?)
            }
        };
        let expected = verify::expected(&self.asset, sums.as_deref())?;
        let progress = progress.clone();
        let actual = download::to_file(
            &self.asset.url,
            self.token.as_deref(),
            &self.path,
            &mut |done, total| {
                let mut snapshot = progress.lock().unwrap();
                snapshot.done = Some(done);
                if total > 0 {
                    snapshot.total = Some(total);
                }
            },
        )
        .await?;
        if actual != expected {
            let _ = std::fs::remove_file(&self.path);
            return Err(Error::other(format!(
                "{}: checksum mismatch, expected {expected}, got {actual}",
                self.asset.name
            )));
        }
        Ok(self.path)
    }
}
