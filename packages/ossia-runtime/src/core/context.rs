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
        let prior = {
            let state = self.state();
            if state.observed.get(&document) == Some(&doc.source_hash) {
                return Ok(doc);
            }
            state.selections.get(&document).cloned()
        };
        let retained = if let Some(prior) = &prior {
            let snapshot = self.library.source_snapshot(document)?;
            self.retained_selection(prior, &snapshot)
        } else {
            None
        };
        let mut state = self.state();
        state.observed.insert(document, doc.source_hash.clone());
        let prior_is_current = prior
            .as_ref()
            .is_none_or(|old| state.selections.get(&document) == Some(old));
        if prior_is_current {
            if let Some(selection) = &retained {
                state.selections.insert(
                    document,
                    SelectionState {
                        selection: selection.clone(),
                        identity: prior.as_ref().expect("retained selection").identity.clone(),
                    },
                );
            } else if prior.is_some() {
                state.selections.remove(&document);
            }
        }
        self.emit_locked(
            &mut state,
            "source-changed",
            Some(document),
            Some(doc.source_hash.clone()),
            json!({}),
        );
        if prior.is_some() && prior_is_current {
            self.emit_locked(
                &mut state,
                "selection-changed",
                Some(document),
                Some(doc.source_hash.clone()),
                json!({
                    "reason": if retained.is_some() {
                        "source-music-preserved"
                    } else {
                        "source-structure-changed"
                    }
                }),
            );
        }
        Ok(doc)
    }

    pub fn context(&self, document_id: DocumentId) -> Result<DocumentContext> {
        let document = self.observe(document_id)?;
        let (selection, seq) = {
            let state = self.state();
            (
                state
                    .selections
                    .get(&document_id)
                    .map(|state| state.selection.clone()),
                state.seq,
            )
        };
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
            if source.source_hash() != &selection.source_hash {
                return Err(stale());
            }
            let index = self.score_index(&source);
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
            seq,
            document,
            selection,
            regions,
            render,
            edit_activity,
        })
    }

    pub fn selection(&self, selection: Selection) -> Result<DocumentContext> {
        self.selection_with_client(selection, None)
    }

    pub(crate) fn selection_with_client(
        &self,
        selection: Selection,
        client_id: Option<String>,
    ) -> Result<DocumentContext> {
        self.library.get_document(selection.document_id)?;
        let snapshot = self.library.source_snapshot(selection.document_id)?;
        if snapshot.source_hash() != &selection.source_hash {
            return Err(stale());
        }
        let index = self.score_index(&snapshot);
        for locator in &selection.locators {
            let region = ossia_score::resolve_locator(&index, locator)?;
            if region.capability != LocationCapability::Measure {
                return Err(AppError::unsupported(
                    "selection requires reliable measure locations",
                ));
            }
        }
        let identity = ossia_score::music_identity_with_index(&index, snapshot.source());
        if !identity.reliable {
            return Err(AppError::unsupported(
                "selection requires reliable musical identity",
            ));
        }
        {
            let mut state = self.state();
            state.selections.insert(
                selection.document_id,
                SelectionState {
                    selection: Selection {
                        document_id: selection.document_id,
                        source_hash: selection.source_hash.clone(),
                        locators: selection.locators.clone(),
                    },
                    identity,
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
                client_id.map_or_else(|| json!({}), |client_id| json!({ "clientId": client_id })),
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

    fn retained_selection(
        &self,
        prior: &SelectionState,
        snapshot: &SourceSnapshot,
    ) -> Option<Selection> {
        let index = self.score_index(snapshot);
        let identity = ossia_score::music_identity_with_index(&index, snapshot.source());
        if !prior.identity.reliable || !identity.reliable || prior.identity != identity {
            return None;
        }
        let locators = prior
            .selection
            .locators
            .iter()
            .map(|locator| {
                ossia_score::resolve_locator(&index, locator)
                    .ok()
                    .filter(|region| region.capability == LocationCapability::Measure)
                    .map(|region| region.locator)
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Selection {
            document_id: snapshot.document_id(),
            source_hash: snapshot.source_hash().clone(),
            locators,
        })
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
        let index = self.score_index(&snapshot);
        let layout = self.score_layout(&snapshot, &manifest)?;
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
