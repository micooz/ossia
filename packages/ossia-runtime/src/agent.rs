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
    pub fn agent_status(&self, args: StatusArgs) -> Result<Value> {
        if args.detail == DetailLevel::Full {
            return value(self.status()?);
        }
        let jobs = self
            .library
            .list_jobs()?
            .into_iter()
            .filter(|j| matches!(j.state, JobState::Queued | JobState::Running))
            .map(|mut j| {
                j.result = None;
                j
            })
            .collect();
        value(AgentStatus {
            runtime: self.info.clone(),
            engine_build_id: self.engine.build_id().into(),
            document_count: self.library.document_count()?,
            instance_id: self.info.instance_id.clone(),
            jobs,
        })
    }
    pub fn agent_context(&self, args: ContextArgs) -> Result<Value> {
        let view = self.context(args.view_id)?;
        if args.detail == DetailLevel::Full {
            return value(view);
        }
        let document = view.document.map(|d| AgentDocument {
            document_id: d.document_id,
            title: d.title,
            source_path: d.source_path,
            head_version_id: d.head_version_id,
            source_hash: d.source_hash,
            dirty: d.dirty,
            edit_state: d.edit_state,
            last_check: d.last_check.map(summarize_job),
        });
        let render = view.render.map(|r| AgentRender {
            render_key: r.render_key,
            source_hash: r.manifest.source_hash,
            pages: r.manifest.pages.len(),
        });
        value(AgentView {
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
    pub fn analysis(&self, args: AnalysisArgs) -> Result<AnalysisPage> {
        let limit = args.limit.unwrap_or(64);
        if limit == 0 || limit > 512 {
            return Err(invalid("analysis limit must be 1..512"));
        }
        let (job, result) = self.library.checked_job(args.job_id)?;
        let items = match args.section {
            AnalysisSection::Performance => value(result.music.performance.notes)?,
            AnalysisSection::Source => value(result.music.source.events)?,
            AnalysisSection::Differences => value(result.verification.differences)?,
        };
        let all = items.as_array().expect("serialized event list");
        let total = all.len();
        if args.offset > total {
            return Err(invalid("analysis offset exceeds total"));
        }
        let end = args.offset.saturating_add(limit).min(total);
        Ok(AnalysisPage {
            source_job_id: job.job_id,
            source_hash: result.render.source_hash,
            engine_build_id: result.render.engine_build_id,
            section: args.section,
            total,
            offset: args.offset,
            next_offset: (end < total).then_some(end),
            items: all[args.offset..end].to_vec(),
        })
    }
}
