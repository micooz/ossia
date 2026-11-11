use super::*;
use serde_json::json;

struct EvidenceInput {
    job: Option<Job>,
    snapshot: SourceSnapshot,
    report: Option<EvidenceReport>,
}

impl Runtime {
    /// Read or derive bounded evidence from an immutable version or job snapshot.
    pub fn evidence(&self, args: EvidenceArgs) -> Result<EvidenceOutcome> {
        self.available()?;
        let wait_ms = validate_wait_ms(args.wait_ms)?;
        let (job, report) = self.ensure_evidence(args.source, wait_ms)?;
        let page = report
            .as_ref()
            .map(|report| evidence_page(args.source, report, args.section, args.offset, args.limit))
            .transpose()?;
        Ok(EvidenceOutcome { job, page })
    }

    pub(crate) fn resolve_reference(
        &self,
        reference: &ReferenceExpectation,
    ) -> Result<ReferenceMusic> {
        let (job, report) = self.ensure_evidence(reference.source, 30_000)?;
        let report = report.ok_or_else(|| {
            if let Some(error) = job.as_ref().and_then(|job| job.error.clone()) {
                return error;
            }
            let mut error = AppError::new(
                ErrorCode::RuntimeUnavailable,
                "reference evidence is not ready; call ossia_evidence and retry",
            );
            error.detail = Some(json!({
                "jobId": job.as_ref().map(|job| job.job_id),
                "source": reference.source,
            }));
            error
        })?;
        Ok(ReferenceMusic {
            source: reference.source,
            source_hash: report.source_hash,
            engine_build_id: report.engine_build_id,
            comparison: reference.comparison,
            performance: report.music.performance,
        })
    }

    fn ensure_evidence(
        &self,
        source: EvidenceSource,
        wait_ms: u64,
    ) -> Result<(Option<Job>, Option<EvidenceReport>)> {
        let mut input = self.evidence_input(source)?;
        if let Some(report) = input.report.take() {
            return Ok((input.job, Some(report)));
        }
        let document = input.snapshot.document_id();
        let hash = input.snapshot.source_hash().clone();
        if let Some(report) = self.library.cached_evidence(
            document,
            &hash,
            self.engine.build_id(),
            EVIDENCE_ANALYSIS_VERSION,
        )? {
            return Ok((input.job, Some(report)));
        }

        let job = if let Some(job) = self.pending_evidence(document, &hash)? {
            job
        } else {
            let job = self
                .library
                .create_job("evidence", Some(document), Some(&input.snapshot))?;
            self.enqueue(job, jobs::Work::Evidence, false)?
        };
        let job = self.wait_for_job(job.job_id, Some(wait_ms))?;
        if job.state != JobState::Done {
            return Ok((Some(job), None));
        }
        let report: EvidenceReport = decode(
            job.result
                .clone()
                .ok_or_else(|| storage("completed evidence job has no report"))?,
        )?;
        Ok((Some(job), Some(report)))
    }

    fn evidence_input(&self, source: EvidenceSource) -> Result<EvidenceInput> {
        match source {
            EvidenceSource::Version { version_id } => {
                let snapshot = self.library.version_snapshot(version_id)?;
                Ok(EvidenceInput {
                    job: None,
                    snapshot,
                    report: None,
                })
            }
            EvidenceSource::Job { job_id } => {
                let mut job = self.library.get_job(job_id)?;
                if job.kind == "import-request" {
                    if job.state != JobState::Done {
                        return Err(AppError::new(
                            ErrorCode::Conflict,
                            "evidence source job is not complete",
                        ));
                    }
                    let outcome: ImportOutcome = decode(
                        job.result
                            .clone()
                            .ok_or_else(|| storage("import request has no outcome"))?,
                    )?;
                    job = self.library.get_job(
                        outcome
                            .job
                            .ok_or_else(|| invalid("import request has no score job"))?
                            .job_id,
                    )?;
                }
                if job.state != JobState::Done {
                    return Err(AppError::new(
                        ErrorCode::Conflict,
                        "evidence source job is not complete",
                    ));
                }
                let snapshot = self
                    .library
                    .job_snapshot(job.job_id)?
                    .ok_or_else(|| invalid("evidence source job has no captured snapshot"))?;
                let report = match job.kind.as_str() {
                    "evidence" => serde_json::from_value::<EvidenceReport>(
                        job.result
                            .clone()
                            .ok_or_else(|| storage("evidence job has no report"))?,
                    )
                    .ok()
                    .filter(|report| report.analysis_version == EVIDENCE_ANALYSIS_VERSION),
                    "check" | "import" => serde_json::from_value::<CheckResult>(
                        job.result
                            .clone()
                            .ok_or_else(|| storage("check job has no result"))?,
                    )
                    .ok()
                    .filter(|result| result.analysis_version == EVIDENCE_ANALYSIS_VERSION)
                    .map(|result| {
                        let report = EvidenceReport {
                            document_id: snapshot.document_id(),
                            source_hash: result.render.source_hash.clone(),
                            engine_build_id: result.render.engine_build_id.clone(),
                            analysis_version: result.analysis_version,
                            music: result.music,
                        };
                        self.library.save_evidence(&report)?;
                        Ok(report)
                    })
                    .transpose()?,
                    _ => None,
                };
                Ok(EvidenceInput {
                    job: Some(job),
                    snapshot,
                    report,
                })
            }
        }
    }

    fn pending_evidence(&self, document: DocumentId, hash: &ContentHash) -> Result<Option<Job>> {
        let (pending, running) = {
            let state = self.state();
            let pending = state
                .pending
                .iter()
                .find(|task| {
                    task.job.kind == "evidence"
                        && task.job.document_id == Some(document)
                        && task.job.source_hash.as_ref() == Some(hash)
                })
                .map(|task| task.job.clone());
            let running = state.running.keys().copied().collect::<Vec<_>>();
            (pending, running)
        };
        if pending.is_some() {
            return Ok(pending);
        }
        for id in running {
            let job = self.library.get_job(id)?;
            if job.kind == "evidence"
                && job.document_id == Some(document)
                && job.source_hash.as_ref() == Some(hash)
            {
                return Ok(Some(job));
            }
        }
        Ok(None)
    }
}

fn evidence_summary(report: &EvidenceReport) -> EvidenceSummary {
    let source = &report.music.source;
    let performance = &report.music.performance;
    EvidenceSummary {
        source_status: source.status,
        source_reasons: source.reasons.clone(),
        measures: source.measures,
        voices: source.voices,
        note_tokens: source.note_tokens,
        source_events: source.events.len(),
        performance_status: performance.status,
        performance_reasons: performance.reasons.clone(),
        performed_notes: performance.notes.len(),
    }
}

fn evidence_page(
    source: EvidenceSource,
    report: &EvidenceReport,
    section: EvidenceSection,
    offset: usize,
    limit: Option<usize>,
) -> Result<EvidencePage> {
    let limit = limit.unwrap_or(64);
    if limit == 0 || limit > 512 {
        return Err(invalid("evidence limit must be 1..512"));
    }
    let items = match section {
        EvidenceSection::Summary => json!([]),
        EvidenceSection::Performance => value(&report.music.performance.notes)?,
        EvidenceSection::Source => value(&report.music.source.events)?,
    };
    let all = items.as_array().expect("serialized evidence list");
    let total = all.len();
    if offset > total {
        return Err(invalid("evidence offset exceeds total"));
    }
    let end = if section == EvidenceSection::Summary {
        0
    } else {
        offset.saturating_add(limit).min(total)
    };
    Ok(EvidencePage {
        source,
        document_id: report.document_id,
        source_hash: report.source_hash.clone(),
        engine_build_id: report.engine_build_id.clone(),
        analysis_version: report.analysis_version,
        section,
        summary: evidence_summary(report),
        total,
        offset,
        next_offset: (section != EvidenceSection::Summary && end < total).then_some(end),
        items: all[offset.min(total)..end].to_vec(),
    })
}
