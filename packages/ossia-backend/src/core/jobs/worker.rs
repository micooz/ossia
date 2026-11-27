use super::*;
use ossia_library::{ImportRequest, RestoreRequest};
use serde_json::json;

impl Runtime {
    pub(super) fn run(&self, task: Pending) -> Result<()> {
        if self.library.get_job(task.job.job_id)?.state != JobState::Queued {
            return Ok(());
        }
        if matches!(task.work, Work::Check) {
            let _ = self
                .library
                .run_check(self.engine.as_ref(), task.job.job_id, &task.context);
            return Ok(());
        }
        self.library.transition_job(
            task.job.job_id,
            JobState::Queued,
            JobState::Running,
            "execute",
            None,
            None,
        )?;
        let result: Result<Value> = (|| {
            task.context.check()?;
            match task.work {
                Work::Check => unreachable!(),
                Work::Evidence => {
                    let snapshot = self
                        .library
                        .job_snapshot(task.job.job_id)?
                        .ok_or_else(|| storage("evidence job has no snapshot"))?;
                    let (report, cacheable) = self.library.derive_evidence(
                        self.engine.as_ref(),
                        &snapshot,
                        &task.context,
                    )?;
                    if cacheable {
                        self.library.save_evidence(&report)?;
                    }
                    value(report)
                }
                Work::VersionRender(document, version) => value(self.library.render_version(
                    self.engine.as_ref(),
                    document,
                    version,
                    &task.context,
                )?),
                Work::Import(args) => {
                    if args.format.is_omr() {
                        self.library.transition_job(
                            task.job.job_id,
                            JobState::Running,
                            JobState::Running,
                            "omr",
                            None,
                            None,
                        )?;
                    }
                    let request = ImportRequest {
                        title: &args.title,
                        input: &args.bytes,
                        format: args.format,
                        confirmation: args.confirmation.as_ref(),
                        author: &args.author,
                    };
                    let outcome = if let Some(reference) = args.reference.as_ref() {
                        self.library.import_with_reference(
                            self.engine.as_ref(),
                            request,
                            Some(reference),
                            &task.context,
                        )?
                    } else {
                        self.library.import_with_expect(
                            self.engine.as_ref(),
                            request,
                            args.expect.as_ref(),
                            &task.context,
                        )?
                    };
                    if let Some(document) = &outcome.document {
                        self.emit(
                            "document-created",
                            Some(document.document_id),
                            Some(document.source_hash.clone()),
                            json!({}),
                        );
                    }
                    value(outcome)
                }
                Work::Restore(args) => value(self.library.restore(
                    self.engine.as_ref(),
                    RestoreRequest {
                        target_version_id: args.version_id,
                        base_version_id: Some(args.base_version_id),
                        expected_source_hash: &args.expected_source_hash,
                        discard_current_edit: args.discard_current_edit,
                        author: args.author.as_deref().unwrap_or(DEFAULT_AUTHOR),
                        summary: &args.summary,
                    },
                    &task.context,
                )?),
                Work::Output(args, destination, drawing, settings) => {
                    task.context.report_progress("preparing", None);
                    match destination {
                        OutputDestination::File(destination) => {
                            value(self.library.output_with_settings(
                                self.engine.as_ref(),
                                &args,
                                &destination,
                                &task.context,
                                drawing.as_ref(),
                                settings.as_ref(),
                            )?)
                        }
                        OutputDestination::CropFile(destination) => {
                            value(self.library.crop_output_file_with_drawing(
                                self.engine.as_ref(),
                                &args,
                                &destination,
                                &task.context,
                                drawing.as_ref(),
                            )?)
                        }
                        OutputDestination::PageBatch {
                            parent,
                            directory_name,
                        } => value(self.library.page_output_with_drawing(
                            self.engine.as_ref(),
                            &args,
                            &parent,
                            &directory_name,
                            &task.context,
                            drawing.as_ref(),
                        )?),
                        OutputDestination::Batch {
                            parent,
                            directory_name,
                        } => value(self.library.crop_output_with_drawing(
                            self.engine.as_ref(),
                            &args,
                            &parent,
                            &directory_name,
                            &task.context,
                            drawing.as_ref(),
                        )?),
                    }
                }
            }
        })();
        let current = self.library.get_job(task.job.job_id)?;
        if current.state != JobState::Running {
            return Ok(());
        }
        match result {
            Ok(result) => {
                self.library.transition_job(
                    task.job.job_id,
                    JobState::Running,
                    JobState::Done,
                    "finished",
                    Some(result),
                    None,
                )?;
            }
            Err(error) => {
                let state = if error.code == ErrorCode::Cancelled {
                    JobState::Cancelled
                } else {
                    JobState::Error
                };
                self.library.transition_job(
                    task.job.job_id,
                    JobState::Running,
                    state,
                    "finished",
                    None,
                    Some(error),
                )?;
            }
        }
        Ok(())
    }
}
