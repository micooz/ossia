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
                state.selections.insert(document, selection.clone());
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
            let target_count = retained
                .as_ref()
                .map_or(0, |selection| selection.regions.len());
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
                    },
                    "targetCount": target_count
                }),
            );
        }
        Ok(doc)
    }

    pub fn context(&self, document_id: DocumentId) -> Result<DocumentContext> {
        let document = self.observe(document_id)?;
        let (mut selection, mut regions, mut seq) = {
            let mut state = self.state();
            if state
                .selections
                .get(&document_id)
                .is_some_and(|s| s.selection.source_hash != document.source_hash)
            {
                state.selections.remove(&document_id);
            }
            let selected = state.selections.get(&document_id);
            (
                selected.map(|s| s.selection.clone()),
                selected.map(|s| s.regions.clone()).unwrap_or_default(),
                state.seq,
            )
        };
        if regions
            .iter()
            .any(|r| matches!(r.target, SelectionTarget::Element { .. }))
        {
            if let Some(manifest) = document
                .last_successful_render
                .as_ref()
                .filter(|r| r.source_hash == document.source_hash)
            {
                let snapshot = self.library.source_snapshot(document_id)?;
                let geometry = self.score_geometry(&snapshot, manifest)?;
                let prior_selection = selection.clone();
                let valid: Vec<_> = regions
                    .iter()
                    .filter(|region| {
                        matches!(region.target, SelectionTarget::Measure { .. })
                            || geometry
                                .iter()
                                .flat_map(|p| &p.elements)
                                .any(|e| e.region.target == region.target)
                    })
                    .map(|r| r.target.clone())
                    .collect();
                let targets = ossia_score::expand_selection(&geometry, &valid);
                regions = targets
                    .iter()
                    .filter_map(|target| {
                        geometry
                            .iter()
                            .flat_map(|p| &p.elements)
                            .find(|e| &e.region.target == target)
                            .map(|e| e.region.clone())
                            .or_else(|| regions.iter().find(|r| &r.target == target).cloned())
                    })
                    .collect();
                if let Some(current) = &mut selection {
                    current.targets = targets;
                }
                if selection.as_ref().is_some_and(|s| s.targets.is_empty()) {
                    selection = None;
                }
                let mut state = self.state();
                if state.selections.get(&document_id).map(|s| &s.selection)
                    == prior_selection.as_ref()
                {
                    if let Some(current) = &selection {
                        if let Some(stored) = state.selections.get_mut(&document_id) {
                            stored.selection = current.clone();
                            stored.regions = regions.clone();
                        }
                    } else {
                        state.selections.remove(&document_id);
                    }
                    if selection != prior_selection {
                        self.emit_locked(
                            &mut state,
                            "selection-changed",
                            Some(document_id),
                            Some(document.source_hash.clone()),
                            json!({ "targetCount": regions.len() }),
                        );
                        seq = state.seq;
                    }
                }
            }
        }
        self.ensure_interaction_render(&document)?;
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
        let mut regions = Vec::new();
        let geometry = if selection
            .targets
            .iter()
            .any(|t| matches!(t, SelectionTarget::Element { .. }))
        {
            let document = self.library.get_document(selection.document_id)?;
            let manifest = document
                .last_successful_render
                .filter(|r| r.source_hash == selection.source_hash)
                .ok_or_else(|| {
                    AppError::unsupported("element selection requires a current render")
                })?;
            self.score_geometry(&snapshot, &manifest)?
        } else {
            Arc::new(Vec::new())
        };
        let identity = ossia_score::music_identity_with_index(&index, snapshot.source());
        let targets = ossia_score::expand_selection(&geometry, &selection.targets);
        for target in &targets {
            if regions
                .iter()
                .any(|r: &SelectionRegion| &r.target == target)
            {
                continue;
            }
            let region = match target {
                SelectionTarget::Measure { locator } => {
                    if !identity.reliable {
                        return Err(AppError::unsupported(
                            "measure selection requires reliable musical identity",
                        ));
                    }
                    ossia_score::measure_selection(&index, locator)?
                }
                SelectionTarget::Element { .. } => geometry
                    .iter()
                    .flat_map(|p| &p.elements)
                    .find(|e| &e.region.target == target)
                    .map(|e| e.region.clone())
                    .ok_or_else(|| {
                        AppError::new(ErrorCode::NotFound, "element not found in current render")
                    })?,
            };
            if region.spans.is_empty() && region.insertion_anchor.is_none() {
                return Err(AppError::unsupported(
                    region
                        .reason
                        .as_deref()
                        .unwrap_or("element has no source mapping"),
                ));
            }
            regions.push(region);
        }
        {
            let target_count = regions.len();
            let mut state = self.state();
            state.selections.insert(
                selection.document_id,
                SelectionState {
                    selection: Selection {
                        document_id: selection.document_id,
                        source_hash: selection.source_hash.clone(),
                        targets: regions.iter().map(|r| r.target.clone()).collect(),
                    },
                    identity,
                    regions,
                    source: snapshot.source().into(),
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
                client_id.map_or_else(
                    || json!({ "targetCount": target_count }),
                    |client_id| json!({ "clientId": client_id, "targetCount": target_count }),
                ),
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
    ) -> Option<SelectionState> {
        let index = self.score_index(snapshot);
        let identity = ossia_score::music_identity_with_index(&index, snapshot.source());
        let same_music = prior.identity.reliable && identity.reliable && prior.identity == identity;
        let mut regions = Vec::new();
        for old in &prior.regions {
            let region = match &old.target {
                SelectionTarget::Measure { locator } => {
                    if !same_music {
                        continue;
                    }
                    let Ok(region) = ossia_score::measure_selection(&index, locator) else {
                        continue;
                    };
                    region
                }
                SelectionTarget::Element { .. } => {
                    let mut region = old.clone();
                    region.occurrences.clear();
                    if old.spans.is_empty() {
                        if !same_music {
                            continue;
                        }
                        region.insertion_anchor = old
                            .staff_index
                            .and_then(|i| ossia_score::property_insertion_anchor(&index, i));
                        if region.insertion_anchor.is_none() {
                            continue;
                        }
                        region.voice = old
                            .staff_index
                            .and_then(|i| index.staves.get(i))
                            .filter(|s| s.voices.len() == 1)
                            .map(|s| s.voices[0].clone());
                    } else {
                        let Some(spans) = old
                            .spans
                            .iter()
                            .map(|span| {
                                ossia_score::remap_selection_span(
                                    &prior.source,
                                    snapshot.source(),
                                    *span,
                                )
                            })
                            .collect::<Option<Vec<_>>>()
                        else {
                            continue;
                        };
                        region.spans = spans;
                        region.voice = region
                            .spans
                            .first()
                            .and_then(|span| {
                                index.voices.iter().find(|v| {
                                    span.byte_start >= v.music_span.byte_start
                                        && span.byte_end <= v.music_span.byte_end
                                })
                            })
                            .map(|v| v.name.clone());
                    }
                    region.target = ossia_score::element_target(snapshot.source_hash(), &region);
                    region
                }
            };
            regions.push(region);
        }
        if regions.is_empty() {
            return None;
        }
        Some(SelectionState {
            selection: Selection {
                document_id: snapshot.document_id(),
                source_hash: snapshot.source_hash().clone(),
                targets: regions.iter().map(|r| r.target.clone()).collect(),
            },
            regions,
            identity,
            source: snapshot.source().into(),
        })
    }

    fn ensure_interaction_render(&self, document: &DocumentState) -> Result<()> {
        let outdated = document.last_successful_render.as_ref().is_some_and(|r| {
            r.source_hash == document.source_hash
                && r.options_hash != ossia_engine::render_options_hash(true)
        });
        if !outdated {
            return Ok(());
        }
        let (pending, running) = {
            let state = self.state();
            (
                state
                    .pending
                    .iter()
                    .any(|p| p.job.document_id == Some(document.document_id)),
                state.running.keys().copied().collect::<Vec<_>>(),
            )
        };
        let busy = pending
            || running.into_iter().any(|id| {
                self.library
                    .get_job(id)
                    .is_ok_and(|job| job.document_id == Some(document.document_id))
            });
        if !busy {
            self.check(
                CheckArgs {
                    document_id: Some(document.document_id),
                    edit_id: None,
                    expected_source_hash: document.source_hash.clone(),
                    expect: None,
                    reference: None,
                    wait_ms: None,
                },
                true,
            )?;
        }
        Ok(())
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
            json!({ "targetCount": 0 }),
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
        let geometry = self.score_geometry(&snapshot, &manifest)?;
        let mut region = ossia_score::hit_selection(&index, &geometry, args.page, args.x, args.y)?;
        let reason = if region
            .as_ref()
            .is_some_and(|r| r.spans.is_empty() && r.insertion_anchor.is_none())
        {
            region.take().and_then(|r| r.reason)
        } else {
            region.is_none().then(|| "no-hit".into())
        };
        Ok(LocateOutcome {
            capability: match region.as_ref().map(|r| &r.target) {
                Some(SelectionTarget::Measure { .. }) => LocationCapability::Measure,
                Some(SelectionTarget::Element { .. }) => LocationCapability::AnchorOnly,
                None => LocationCapability::None,
            },
            reason,
            region,
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
