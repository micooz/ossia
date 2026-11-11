use crate::*;

impl Library {
    /// Resolve an import request to its child check without following arbitrary task results.
    pub fn checked_job(&self, id: JobId) -> Result<(Job, CheckResult)> {
        let mut job = self.get_job(id)?;
        if job.state != JobState::Done {
            return Err(conflict("artifact source job must be completed"));
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
        let result = serde_json::from_value(job.result.clone().ok_or_else(not_found)?)
            .map_err(|_| AppError::unsupported("legacy job has no music report; check again"))?;
        Ok((job, result))
    }

    /// Prepare a verified artifact from the source job's captured snapshot.
    pub fn draft_artifact(
        &self,
        engine: &dyn EngineBackend,
        args: &ArtifactArgs,
        context: &RunContext,
    ) -> Result<ArtifactOutcome> {
        context.check()?;
        let (job, checked) = self.checked_job(args.job_id)?;
        let snapshot = self.job_snapshot(job.job_id)?.ok_or_else(not_found)?;
        let page = args.page.unwrap_or(1);
        if page == 0
            || (args.width.is_some() && args.format != DraftFormat::Png)
            || (args.page.is_some() && matches!(args.format, DraftFormat::Midi | DraftFormat::Pdf))
        {
            return Err(AppError::invalid(
                "page/width are valid only for paged preview formats",
            ));
        }
        let render = &checked.render;
        let existing = match args.format {
            DraftFormat::Svg | DraftFormat::Png => render
                .pages
                .iter()
                .find(|p| p.page == page)
                .map(|p| p.artifact.clone()),
            DraftFormat::Midi => render.midi.clone(),
            DraftFormat::Pdf => render.pdf.clone(),
        };
        let asset = match (args.format, existing) {
            (DraftFormat::Png, Some(svg)) => {
                if render.engine_build_id != engine.build_id() {
                    return Err(AppError::new(
                        ErrorCode::RuntimeIncompatible,
                        "check again with the current engine before generating PNG",
                    ));
                }
                engine.rasterize_svg(
                    &self.read_artifact(&svg)?,
                    args.width.unwrap_or(1600),
                    &self.artifact_root()?,
                    context,
                )?
            }
            (_, Some(asset)) => asset,
            (DraftFormat::Svg | DraftFormat::Png, None) => return Err(not_found()),
            (format, None) => {
                if render.engine_build_id != engine.build_id() {
                    return Err(AppError::new(
                        ErrorCode::RuntimeIncompatible,
                        "check again with the current engine before generating an artifact",
                    ));
                }
                let format = if format == DraftFormat::Pdf {
                    RenderFormat::Pdf
                } else {
                    RenderFormat::Midi
                };
                let generated = engine.compile(
                    &snapshot,
                    &CompileOptions {
                        formats: vec![format],
                        point_and_click: true,
                    },
                    &self.artifact_root()?,
                    context,
                )?;
                if generated.source_hash != *snapshot.source_hash()
                    || generated.engine_build_id != engine.build_id()
                {
                    return Err(storage_error("draft artifact provenance mismatch"));
                }
                let asset = if format == RenderFormat::Pdf {
                    generated.pdf
                } else {
                    generated.midi
                };
                asset.ok_or_else(|| {
                    generated
                        .format_errors
                        .first()
                        .map(|e| e.error.clone())
                        .unwrap_or_else(|| AppError::unsupported("draft format unavailable"))
                })?
            }
        };
        self.read_artifact(&asset)?;
        let media_type = match args.format {
            DraftFormat::Svg => "image/svg+xml",
            DraftFormat::Png => "image/png",
            DraftFormat::Midi => "audio/midi",
            DraftFormat::Pdf => "application/pdf",
        };
        Ok(ArtifactOutcome {
            source_job_id: job.job_id,
            document_id: snapshot.document_id(),
            source_hash: snapshot.source_hash().clone(),
            engine_build_id: render.engine_build_id.clone(),
            format: args.format,
            page: matches!(args.format, DraftFormat::Svg | DraftFormat::Png).then_some(page),
            path: self
                .store
                .path(&format!("artifacts/{}", asset.path))?
                .to_string_lossy()
                .into_owned(),
            sha256: asset.sha256,
            bytes: asset.bytes,
            media_type: media_type.into(),
        })
    }
}
