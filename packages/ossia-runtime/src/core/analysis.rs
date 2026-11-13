use super::*;
use ossia_score::{ScoreIndex, ScoreLayout};

const CACHE_ENTRIES: usize = 8;

#[derive(Default)]
pub(super) struct AnalysisCache {
    indexes: VecDeque<(ContentHash, Arc<ScoreIndex>)>,
    layouts: VecDeque<(ContentHash, Arc<ScoreLayout>)>,
}

impl Runtime {
    pub(crate) fn score_index(&self, snapshot: &SourceSnapshot) -> Arc<ScoreIndex> {
        let mut cache = self.analysis.lock().expect("analysis cache poisoned");
        let entry = cache
            .indexes
            .iter()
            .position(|(hash, _)| hash == snapshot.source_hash())
            .and_then(|position| cache.indexes.remove(position))
            .unwrap_or_else(|| {
                (
                    snapshot.source_hash().clone(),
                    Arc::new(ossia_score::build_index(snapshot.source())),
                )
            });
        let index = entry.1.clone();
        cache.indexes.push_back(entry);
        if cache.indexes.len() > CACHE_ENTRIES {
            cache.indexes.pop_front();
        }
        index
    }

    pub(crate) fn score_layout(
        &self,
        snapshot: &SourceSnapshot,
        manifest: &RenderManifest,
    ) -> Result<Arc<ScoreLayout>> {
        if snapshot.source_hash() != &manifest.source_hash {
            return Err(stale());
        }
        let key = ossia_library::render_key(snapshot.document_id(), manifest);
        let index = self.score_index(snapshot);
        let mut cache = self.analysis.lock().expect("analysis cache poisoned");
        if let Some(position) = cache.layouts.iter().position(|(hash, _)| hash == &key) {
            let entry = cache.layouts.remove(position).expect("cached layout");
            let layout = entry.1.clone();
            cache.layouts.push_back(entry);
            return Ok(layout);
        }
        // Render artifacts are immutable; parse once for each document/render identity.
        let sources = manifest
            .pages
            .iter()
            .map(|page| {
                Ok((
                    page.page,
                    String::from_utf8(self.library.read_artifact(&page.artifact)?)
                        .map_err(storage)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let pages = sources
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect::<Vec<_>>();
        let layout = Arc::new(ossia_score::build_layout(&index, &pages)?);
        cache.layouts.push_back((key, layout.clone()));
        if cache.layouts.len() > CACHE_ENTRIES {
            cache.layouts.pop_front();
        }
        Ok(layout)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/core/analysis.rs"]
mod tests;
