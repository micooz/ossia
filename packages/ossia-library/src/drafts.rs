use crate::*;

impl Library {
    /// Resolve an import request to its child check without following arbitrary task results.
    pub fn checked_job(&self, id: JobId) -> Result<(Job, CheckResult)> {
        let mut job = self.get_job(id)?;
        if job.state != JobState::Done {
            return Err(conflict("output source job must be completed"));
        }
        if job.kind == "import-request" {
            let outcome: ImportOutcome =
                serde_json::from_value(job.result.clone().ok_or_else(not_found)?)
                    .map_err(storage_error)?;
            job = self.get_job(
                outcome
                    .job
                    .ok_or_else(|| AppError::unsupported("import has no score"))?
                    .job_id,
            )?;
        }
        if !matches!(job.kind.as_str(), "import" | "check") || job.state != JobState::Done {
            return Err(AppError::invalid(
                "expected a completed import or check job",
            ));
        }
        self.get_document(
            job.document_id
                .ok_or_else(|| AppError::invalid("completed job has no document"))?,
        )?;
        let result =
            serde_json::from_value(job.result.clone().ok_or_else(not_found)?).map_err(|_| {
                AppError::unsupported(
                    "job has no embedded music report; read or derive it with ossia_evidence",
                )
            })?;
        Ok((job, result))
    }
}
