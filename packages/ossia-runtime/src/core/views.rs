use super::*;
use serde_json::json;
use std::time::{Duration, Instant};

impl Runtime {
    pub fn open_view(&self, document: Option<DocumentId>) -> Result<OpenView> {
        self.available()?;
        if let Some(id) = document {
            self.library.get_document(id)?;
        }
        let view = ViewId::new();
        let ticket = secret();
        let mut state = self.state();
        state.views.insert(
            view,
            View {
                document,
                selection: None,
            },
        );
        state
            .tickets
            .retain(|_, ticket| ticket.expires > Instant::now());
        state.tickets.insert(
            ticket.clone(),
            Ticket {
                view,
                expires: Instant::now() + Duration::from_secs(300),
            },
        );
        Ok(OpenView {
            view_id: view,
            url: format!("{}/#ticket={ticket}", self.info.web_base_url),
            ticket,
        })
    }

    pub fn exchange(&self, ticket: &str) -> Result<ExchangeResponse> {
        let view = {
            let mut state = self.state();
            let ticket = state.tickets.remove(ticket).ok_or_else(|| {
                AppError::new(ErrorCode::Unauthorized, "invalid or consumed ticket")
            })?;
            if ticket.expires <= Instant::now() {
                return Err(AppError::new(ErrorCode::Unauthorized, "ticket expired"));
            }
            ticket.view
        };
        let token = secret();
        self.state().tokens.insert(
            token.clone(),
            Token {
                view,
                expires: Instant::now() + Duration::from_secs(86400),
            },
        );
        Ok(ExchangeResponse {
            token,
            view: self.context(view)?,
        })
    }

    pub fn authenticate(&self, token: &str) -> Result<ViewId> {
        let state = self.state();
        let token = state
            .tokens
            .get(token)
            .filter(|t| t.expires > Instant::now())
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Unauthorized,
                    "missing, expired or invalid bearer token",
                )
            })?;
        Ok(token.view)
    }

    fn view(&self, id: ViewId) -> Result<View> {
        self.state()
            .views
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "view not found; open a new view"))
    }

    pub(super) fn document_in_view(&self, view: ViewId, document: DocumentId) -> Result<()> {
        if self.view(view)?.document != Some(document) {
            Err(forbidden())
        } else {
            Ok(())
        }
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
            for view in state
                .views
                .values_mut()
                .filter(|v| v.document == Some(document))
            {
                view.selection = None;
            }
            self.emit_locked(
                &mut state,
                "source-changed",
                None,
                Some(document),
                Some(doc.source_hash.clone()),
                json!({}),
            );
        }
        Ok(doc)
    }

    pub fn context(&self, id: ViewId) -> Result<ViewSnapshot> {
        let view = self.view(id)?;
        if let Some(document) = view.document {
            self.observe(document)?;
        }
        // The cursor and view state share the event lock, preventing a snapshot/subscription gap.
        let state = self.state();
        let view = state
            .views
            .get(&id)
            .ok_or_else(|| invalid("view disappeared"))?;
        let document = view
            .document
            .map(|d| self.library.observe_source(d))
            .transpose()?;
        let selection = view.selection.clone();
        let mut regions = Vec::new();
        if let (Some(doc), Some(selection)) = (&document, &selection) {
            if selection.source_hash != doc.source_hash {
                return Err(stale());
            }
            let source = self.library.source_snapshot(doc.document_id)?;
            if source.source_hash() != &selection.source_hash {
                return Err(stale());
            }
            let index = ossia_score::build_index(source.source());
            for locator in &selection.locators {
                regions.push(ossia_score::resolve_locator(&index, locator)?);
            }
        }
        let render = document.as_ref().and_then(|d| {
            d.last_successful_render.as_ref().map(|m| RenderReference {
                render_key: ossia_library::render_key(d.document_id, m),
                manifest: m.clone(),
            })
        });
        let edit_activity = document
            .as_ref()
            .map(|d| self.library.edit_activity(d.document_id, id))
            .transpose()?
            .flatten();
        Ok(ViewSnapshot {
            instance_id: self.info.instance_id.clone(),
            seq: state.seq,
            view_id: id,
            document,
            selection,
            regions,
            render,
            edit_activity,
        })
    }

    pub fn select_document(
        &self,
        view: ViewId,
        document: Option<DocumentId>,
    ) -> Result<ViewSnapshot> {
        if let Some(id) = document {
            self.library.get_document(id)?;
        }
        {
            let mut state = self.state();
            let v = state
                .views
                .get_mut(&view)
                .ok_or_else(|| invalid("unknown view"))?;
            v.document = document;
            v.selection = None;
            self.emit_locked(
                &mut state,
                "view-changed",
                Some(view),
                document,
                None,
                json!({}),
            );
        }
        self.context(view)
    }

    pub fn selection(&self, selection: Selection) -> Result<ViewSnapshot> {
        self.document_in_view(selection.view_id, selection.document_id)?;
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
            let view = state
                .views
                .get_mut(&selection.view_id)
                .ok_or_else(|| invalid("unknown view"))?;
            view.selection = if selection.locators.is_empty() {
                None
            } else {
                Some(selection.clone())
            };
            state
                .observed
                .insert(selection.document_id, selection.source_hash.clone());
            self.emit_locked(
                &mut state,
                "selection-changed",
                Some(selection.view_id),
                Some(selection.document_id),
                Some(selection.source_hash.clone()),
                json!({}),
            );
        }
        let context = self.context(selection.view_id)?;
        if context
            .document
            .as_ref()
            .is_none_or(|doc| doc.source_hash != selection.source_hash)
        {
            return Err(stale());
        }
        Ok(context)
    }

    pub fn locate(&self, view: ViewId, args: LocateArgs) -> Result<LocateOutcome> {
        let document = self
            .view(view)?
            .document
            .ok_or_else(|| invalid("view has no document"))?;
        let doc = self.observe(document)?;
        let snapshot = self.library.source_snapshot(document)?;
        if args.source_hash != doc.source_hash || snapshot.source_hash() != &args.source_hash {
            return Err(stale());
        }
        let manifest = doc
            .last_successful_render
            .filter(|r| r.source_hash == args.source_hash)
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

    pub fn events(&self, view: ViewId, instance: &str, after: u64) -> Result<Vec<RuntimeEvent>> {
        self.view(view)?;
        let state = self.state();
        if instance != self.info.instance_id
            || after > state.seq
            || state
                .events
                .front()
                .is_some_and(|e| after < e.seq.saturating_sub(1))
        {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "event cursor expired; fetch a new snapshot",
            ));
        }
        Ok(state
            .events
            .iter()
            .filter(|e| e.seq > after && e.view_id.is_none_or(|v| v == view))
            .cloned()
            .collect())
    }
}
