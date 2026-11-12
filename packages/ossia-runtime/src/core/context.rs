use super::*;
use serde_json::json;

impl Runtime {
    pub fn open_url(&self, document: Option<DocumentId>) -> Result<String> {
        self.available()?;
        if let Some(id) = document {
            self.library.get_document(id)?;
        }
        Ok(match document {
            Some(id) => format!("{}/scores?documentId={id}", self.info.web_base_url),
            None => format!("{}/", self.info.web_base_url),
        })
    }

    pub fn observe(&self, document: DocumentId) -> Result<DocumentState> {
        let doc = self.library.observe_source(document)?;
        let mut state = self.state();
        if state
            .observed
            .insert(document, doc.source_hash.clone())
            .as_ref()
            != Some(&doc.source_hash)
        {
            state.selections.remove(&document);
            self.emit_locked(
                &mut state,
                "source-changed",
                Some(document),
                Some(doc.source_hash.clone()),
                json!({}),
            );
        }
        Ok(doc)
    }

    pub fn context(&self, document_id: DocumentId) -> Result<DocumentContext> {
        let document = self.observe(document_id)?;
        let selection = self.state().selections.get(&document_id).cloned();
        let selection = if let Some(selection) = selection {
            if selection.source_hash != document.source_hash {
                self.state().selections.remove(&document_id);
                None
            } else {
                Some(selection)
            }
        } else {
            None
        };
        let mut regions = Vec::new();
        if let Some(selection) = &selection {
            let source = self.library.source_snapshot(document_id)?;
            let index = ossia_score::build_index(source.source());
            for locator in &selection.locators {
                regions.push(ossia_score::resolve_locator(&index, locator)?);
            }
        }
        let render = document
            .last_successful_render
            .as_ref()
            .map(|manifest| RenderReference {
                render_key: ossia_library::render_key(document_id, manifest),
                manifest: manifest.clone(),
            });
        let edit_activity = self.library.edit_activity(document_id)?;
        Ok(DocumentContext {
            instance_id: self.info.instance_id.clone(),
            seq: self.event_seq(),
            document,
            selection,
            regions,
            render,
            edit_activity,
        })
    }

    pub fn selection(&self, selection: Selection) -> Result<DocumentContext> {
        self.library.get_document(selection.document_id)?;
        let snapshot = self.library.source_snapshot(selection.document_id)?;
        if snapshot.source_hash() != &selection.source_hash {
            return Err(stale());
        }
        let index = ossia_score::build_index(snapshot.source());
        for locator in &selection.locators {
            let region = ossia_score::resolve_locator(&index, locator)?;
            if region.capability != LocationCapability::Measure {
                return Err(AppError::unsupported(
                    "selection requires reliable measure locations",
                ));
            }
        }
        {
            let mut state = self.state();
            state.selections.insert(
                selection.document_id,
                Selection {
                    document_id: selection.document_id,
                    source_hash: selection.source_hash.clone(),
                    locators: selection.locators.clone(),
                },
            );
            state
                .observed
                .insert(selection.document_id, selection.source_hash.clone());
            self.emit_locked(
                &mut state,
                "selection-changed",
                Some(selection.document_id),
                Some(selection.source_hash.clone()),
                json!({}),
            );
        }
        let context = self.context(selection.document_id)?;
        if context
            .selection
            .as_ref()
            .is_none_or(|selected| selected.source_hash != selection.source_hash)
        {
            return Err(stale());
        }
        Ok(context)
    }

    pub fn clear_selection(&self, document_id: DocumentId) -> Result<DocumentContext> {
        self.library.get_document(document_id)?;
        let mut state = self.state();
        state.selections.remove(&document_id);
        self.emit_locked(
            &mut state,
            "selection-changed",
            Some(document_id),
            None,
            json!({}),
        );
        drop(state);
        self.context(document_id)
    }

    pub fn locate(&self, document: DocumentId, args: LocateArgs) -> Result<LocateOutcome> {
        let doc = self.observe(document)?;
        let snapshot = self.library.source_snapshot(document)?;
        if args.source_hash != doc.source_hash || snapshot.source_hash() != &args.source_hash {
            return Err(stale());
        }
        let manifest = doc
            .last_successful_render
            .filter(|render| render.source_hash == args.source_hash)
            .ok_or_else(|| AppError::unsupported("no current successful render"))?;
        let mut pages = Vec::new();
        for page in &manifest.pages {
            let bytes = self.library.read_artifact(&page.artifact)?;
            pages.push((page.page, String::from_utf8(bytes).map_err(storage)?));
        }
        let pages: Vec<_> = pages
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect();
        let index = ossia_score::build_index(snapshot.source());
        let layout = ossia_score::build_layout(&index, &pages)?;
        let result = ossia_score::locate(
            &index,
            &layout,
            &ossia_score::LocateQuery::Point {
                page: args.page,
                x: args.x,
                y: args.y,
            },
        )?;
        Ok(LocateOutcome {
            capability: result.capability,
            reason: result.reason,
            region: result.region,
        })
    }

    pub fn events(
        &self,
        document: Option<DocumentId>,
        instance: &str,
        after: u64,
    ) -> Result<Vec<RuntimeEvent>> {
        let state = self.state();
        if instance != self.info.instance_id
            || after > state.seq
            || state
                .events
                .front()
                .is_some_and(|event| after < event.seq.saturating_sub(1))
        {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "event cursor expired; fetch a new snapshot",
            ));
        }
        Ok(state
            .events
            .iter()
            .filter(|event| event.seq > after)
            .filter(|event| {
                document.is_none() || event.document_id.is_none() || event.document_id == document
            })
            .cloned()
            .collect())
    }
}
