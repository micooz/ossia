use crate::*;
use serde_json::json;

fn truncate_array(value: &mut Value, path: &[&str], limit: usize) {
    let mut current = value;
    for key in &path[..path.len() - 1] {
        let Some(next) = current.get_mut(*key) else {
            return;
        };
        current = next;
    }
    let key = path[path.len() - 1];
    if let Some(items) = current.get_mut(key).and_then(Value::as_array_mut) {
        let total = items.len();
        items.truncate(limit);
        current[format!("{key}Total")] = json!(total);
        current[format!("{key}Truncated")] = json!(total > limit);
    }
}
fn summarize_result(result: &mut Value) {
    for path in [
        ["music", "performance", "notes"],
        ["music", "source", "events"],
    ] {
        truncate_array(result, &path, 64);
    }
    truncate_array(result, &["verification", "differences"], 64);
    if let Some(diagnostics) = result
        .pointer_mut("/render/diagnostics")
        .and_then(Value::as_array_mut)
    {
        diagnostics.retain(|d| d["severity"] != "info");
    }
}

/// Keep bounded evidence in Agent task responses; full persisted results remain available.
pub(crate) fn summarize_job(mut job: Job) -> Job {
    if let Some(result) = &mut job.result {
        if matches!(job.kind.as_str(), "check" | "import") {
            summarize_result(result);
        }
        if job.kind == "import-request" {
            if let Some(report) = result.get_mut("report").and_then(Value::as_object_mut) {
                report.remove("source");
                report.insert("diagnostics".into(), json!([]));
            }
            result["render"] = Value::Null;
            if let Some(doc) = result.get_mut("document").and_then(Value::as_object_mut) {
                doc.insert("lastSuccessfulRender".into(), Value::Null);
                doc.insert("lastCheck".into(), Value::Null);
            }
            if let Some(child) = result.get_mut("job").and_then(Value::as_object_mut) {
                child.insert("result".into(), Value::Null);
            }
            for path in [
                ["music", "performance", "notes"],
                ["music", "source", "events"],
            ] {
                truncate_array(result, &path, 64);
            }
        }
    }
    job
}

impl Runtime {
    pub fn initialize_documentation(&self, resources: &std::path::Path) {
        *self
            .documentation_resources
            .write()
            .expect("documentation resource lock poisoned") = Some(resources.to_owned());
    }
    pub fn documentation_status(&self) -> DocumentationStatus {
        match &*self
            .documentation
            .read()
            .expect("documentation lock poisoned")
        {
            Ok(docs) => docs.status(),
            Err(error) => ossia_library::Documentation::unavailable(error.clone()),
        }
    }
    pub fn docs(&self, request: DocsRequest) -> Result<DocsOutcome> {
        let mut documentation = self
            .documentation
            .write()
            .expect("documentation lock poisoned");
        if documentation.is_err() {
            let resources = self
                .documentation_resources
                .read()
                .expect("documentation resource lock poisoned")
                .clone()
                .ok_or_else(|| invalid("offline documentation is not configured"))?;
            *documentation = ossia_library::Documentation::install(self.library.home(), &resources);
        }
        documentation
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|docs| docs.query(request))
    }
    pub fn doctor(&self) -> DoctorReport {
        let documentation = self
            .documentation_resources
            .read()
            .expect("documentation resource lock poisoned")
            .as_ref()
            .map_or_else(
                || self.documentation_status(),
                |resources| ossia_library::Documentation::inspect(resources),
            );
        DoctorReport {
            home: self.library.home().to_string_lossy().into_owned(),
            engine: EngineHealth {
                ok: true,
                build_id: Some(self.engine.build_id().into()),
                error: None,
            },
            runtime: RuntimeHealth {
                ok: true,
                info: Some(self.info.clone()),
                error: None,
            },
            documentation: Some(documentation),
        }
    }

    pub fn agent_status(&self) -> Result<AgentStatus> {
        let seq = self.event_seq();
        let jobs = self
            .library
            .list_jobs()?
            .into_iter()
            .filter(|j| matches!(j.state, JobState::Queued | JobState::Running))
            .map(|mut j| {
                j.result = None;
                j
            })
            .collect::<Vec<_>>();
        Ok(AgentStatus {
            runtime: self.info.clone(),
            engine_build_id: self.engine.build_id().into(),
            document_count: self.library.document_count()?,
            instance_id: self.info.instance_id.clone(),
            seq,
            jobs,
            documentation: self.documentation_status(),
        })
    }

    pub fn agent_list(&self, args: ListArgs) -> Result<DocumentPage> {
        let limit = args.limit.unwrap_or(50);
        if !(1..=200).contains(&limit) {
            return Err(invalid("list limit must be 1..200"));
        }
        self.library.document_page(limit, args.offset)
    }

    pub fn agent_history(&self, args: HistoryArgs) -> Result<VersionPage> {
        let limit = args.limit.unwrap_or(50);
        if !(1..=200).contains(&limit) {
            return Err(invalid("history limit must be 1..200"));
        }
        self.library
            .history_page(args.document_id, limit, args.offset)
    }

    pub fn agent_open(&self, document_id: Option<DocumentId>) -> Result<AgentOpenOutcome> {
        let open = self.open_view(document_id)?;
        let context = self.agent_context(open.view_id)?;
        Ok(AgentOpenOutcome {
            view_id: open.view_id,
            url: open.url,
            context,
        })
    }

    pub fn agent_context(&self, view_id: ViewId) -> Result<AgentView> {
        let view = self.context(view_id)?;
        let source_bytes = view
            .document
            .as_ref()
            .map(|document| {
                std::fs::metadata(&document.source_path)
                    .map(|metadata| metadata.len())
                    .map_err(storage)
            })
            .transpose()?
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or_default();
        let document = view.document.map(|d| AgentDocument {
            document_id: d.document_id,
            title: d.title,
            head_version_id: d.head_version_id,
            source_hash: d.source_hash,
            source_bytes,
            dirty: d.dirty,
            edit_state: d.edit_state,
            last_check: d.last_check.map(summarize_job),
        });
        let render = view.render.map(|r| AgentRender {
            render_key: r.render_key,
            source_hash: r.manifest.source_hash,
            pages: r.manifest.pages.len(),
        });
        Ok(AgentView {
            instance_id: view.instance_id,
            seq: view.seq,
            view_id: view.view_id,
            document,
            selection: view.selection,
            regions: view.regions,
            render,
            edit_activity: view.edit_activity,
        })
    }

    pub fn analysis(
        &self,
        job_id: JobId,
        section: AnalysisSection,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<AnalysisPage> {
        let limit = limit.unwrap_or(64);
        if limit == 0 || limit > 512 {
            return Err(invalid("analysis limit must be 1..512"));
        }
        let (job, result) = self.library.checked_job(job_id)?;
        let items = match section {
            AnalysisSection::Performance => value(result.music.performance.notes)?,
            AnalysisSection::Source => value(result.music.source.events)?,
            AnalysisSection::Differences => value(result.verification.differences)?,
        };
        let all = items.as_array().expect("serialized event list");
        let total = all.len();
        if offset > total {
            return Err(invalid("analysis offset exceeds total"));
        }
        let end = offset.saturating_add(limit).min(total);
        Ok(AnalysisPage {
            source_job_id: job.job_id,
            source_hash: result.render.source_hash,
            engine_build_id: result.render.engine_build_id,
            section,
            total,
            offset,
            next_offset: (end < total).then_some(end),
            items: all[offset..end].to_vec(),
        })
    }

    pub fn job_outcome(&self, args: JobArgs) -> Result<JobOutcome> {
        let wait_ms = validate_wait_ms(args.wait_ms)?;
        if args.action == JobAction::Cancel && args.section.is_some() {
            return Err(invalid("cancel does not read job evidence"));
        }
        if args.detail == DetailLevel::Full && args.section.is_some() {
            return Err(invalid("detail full cannot be combined with section"));
        }
        let job = match args.action {
            JobAction::Get => self.wait_for_job(args.job_id, Some(wait_ms))?,
            JobAction::Cancel => {
                let job = self.cancel(args.job_id)?;
                self.wait_for_job(job.job_id, Some(wait_ms))?
            }
        };
        if let Some(section) = args.section {
            let analysis = self.analysis(args.job_id, section, args.offset, args.limit)?;
            let mut job = job;
            job.result = None;
            return Ok(JobOutcome {
                job,
                analysis: Some(analysis),
            });
        }
        let job = if args.detail == DetailLevel::Full {
            job
        } else {
            summarize_job(job)
        };
        Ok(JobOutcome {
            job,
            analysis: None,
        })
    }
}
