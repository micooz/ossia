use crate::{db, *};
use std::io::Write;

/// Includes document ownership so identical sources in different documents cannot be confused.
pub fn render_key(document: DocumentId, render: &RenderManifest) -> ContentHash {
    ContentHash::of(
        format!(
            "{document}:{}:{}:{}",
            render.source_hash, render.engine_build_id, render.options_hash
        )
        .as_bytes(),
    )
}

impl Library {
    pub fn edit_activity(&self, document: DocumentId) -> Result<Option<EditActivity>> {
        Ok(
            db::active_edit(&*self.db()?, document)?.map(|edit| EditActivity {
                author: edit.author,
                status: edit.status,
            }),
        )
    }

    pub fn render_snapshot(&self, key: &ContentHash) -> Result<SourceSnapshot> {
        let (document, manifest) = self.render(key)?;
        SourceSnapshot::new(document, self.store.get(&manifest.source_hash)?)
    }

    pub fn render_version(
        &self,
        engine: &dyn EngineBackend,
        document: DocumentId,
        version: VersionId,
        context: &RunContext,
    ) -> Result<RenderReference> {
        let version = self.get_version(document, version)?;
        let options = CompileOptions::default();
        let options_hash = ossia_engine::render_options_hash(options.point_and_click);
        let cached: Option<(i64, String)> = {
            use rusqlite::OptionalExtension;
            let conn = self.db()?;
            let cached = conn.query_row("SELECT rowid,body FROM renders WHERE document_id=?1 AND source_hash=?2 AND engine_build_id=?3 AND options_hash=?4",
                rusqlite::params![document.to_string(), version.source_hash.to_string(), engine.build_id(), options_hash.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(storage_error)?;
            if let Some((rowid, _)) = &cached {
                conn.execute(
                    "UPDATE renders SET last_used_at=?1 WHERE rowid=?2",
                    rusqlite::params![db::now_millis(), rowid],
                )
                .map_err(storage_error)?;
            }
            cached
        };
        let snapshot = self.version_snapshot(version.version_id)?;
        let render = if let Some((_, body)) = cached {
            db::decode(body)?
        } else {
            engine.compile(&snapshot, &options, &self.artifact_root()?, context)?
        };
        if !self.validate_render(engine, &snapshot, &render)? {
            return Err(compile_error(&render));
        }
        context.check()?;
        db::record_render(&*self.db()?, document, &render)?;
        Ok(RenderReference {
            render_key: render_key(document, &render),
            manifest: render,
        })
    }

    /// Count library records without loading mutable source files or render history.
    pub fn document_count(&self) -> Result<usize> {
        let count: i64 = self
            .db()?
            .query_row(
                "SELECT count(*) FROM documents WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        usize::try_from(count).map_err(storage_error)
    }

    /// Read only queued and running jobs without decoding historical result bodies.
    pub fn list_active_jobs(&self) -> Result<Vec<Job>> {
        let conn = self.db()?;
        let mut statement = conn
            .prepare("SELECT body FROM jobs WHERE state IN ('queued','running') ORDER BY rowid")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(storage_error)?;
        rows.map(|row| db::decode(row.map_err(storage_error)?))
            .collect()
    }

    pub fn list_jobs(&self) -> Result<Vec<Job>> {
        let conn = self.db()?;
        let mut statement = conn
            .prepare("SELECT body FROM jobs ORDER BY rowid")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(storage_error)?;
        rows.map(|row| db::decode(row.map_err(storage_error)?))
            .collect()
    }

    pub fn get_version(&self, document: DocumentId, version: VersionId) -> Result<Version> {
        let version = db::version(&*self.db()?, version)?;
        if version.document_id != document {
            return Err(not_found());
        }
        Ok(version)
    }

    pub fn render(&self, key: &ContentHash) -> Result<(DocumentId, RenderManifest)> {
        let conn = self.db()?;
        let mut statement = conn
            .prepare(
                "SELECT r.rowid,r.document_id,r.body FROM renders r JOIN documents d ON d.document_id=r.document_id WHERE d.deleted_at IS NULL",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(storage_error)?;
        let mut found = None;
        for row in rows {
            let (rowid, document, body) = row.map_err(storage_error)?;
            let document = db::parse(document)?;
            let render: RenderManifest = db::decode(body)?;
            if &render_key(document, &render) == key {
                found = Some((rowid, document, render));
                break;
            }
        }
        drop(statement);
        let Some((rowid, document, render)) = found else {
            return Err(not_found());
        };
        conn.execute(
            "UPDATE renders SET last_used_at=?1 WHERE rowid=?2",
            rusqlite::params![db::now_millis(), rowid],
        )
        .map_err(storage_error)?;
        Ok((document, render))
    }

    pub fn read_artifact(&self, asset: &ArtifactRef) -> Result<Vec<u8>> {
        self.validate_asset(asset)?;
        let bytes = fs::read(self.store.path(&format!("artifacts/{}", asset.path))?)
            .map_err(storage_error)?;
        if bytes.len() as u64 != asset.bytes || ContentHash::of(&bytes) != asset.sha256 {
            return Err(storage_error("artifact changed during read"));
        }
        Ok(bytes)
    }

    pub fn read_render_page(&self, key: &ContentHash, page: u32) -> Result<Vec<u8>> {
        let (_, render) = self.render(key)?;
        let asset = render
            .pages
            .iter()
            .find(|p| p.page == page)
            .ok_or_else(not_found)?;
        self.read_artifact(&asset.artifact)
    }

    pub fn output(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        destination: &Path,
        context: &RunContext,
    ) -> Result<OutputOutcome> {
        let drawing = self.drawing_for_output(request)?;
        self.output_with_drawing(engine, request, destination, context, drawing.as_ref())
    }

    /// Produce an output using the drawing snapshot captured with the request.
    pub fn output_with_drawing(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        destination: &Path,
        context: &RunContext,
        drawing: Option<&Drawing>,
    ) -> Result<OutputOutcome> {
        let settings = if request.format == OutputFormat::Wav {
            let document = match request.source {
                OutputSource::Version { version_id } => {
                    self.version_snapshot(version_id)?.document_id()
                }
                OutputSource::Job { job_id } => self
                    .job_snapshot(job_id)?
                    .ok_or_else(not_found)?
                    .document_id(),
            };
            Some(self.score_settings(document)?)
        } else {
            None
        };
        self.output_with_settings(
            engine,
            request,
            destination,
            context,
            drawing,
            settings.as_ref(),
        )
    }

    /// Produce an output using settings captured when the job was submitted.
    pub fn output_with_settings(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        destination: &Path,
        context: &RunContext,
        drawing: Option<&Drawing>,
        settings: Option<&ScoreSettings>,
    ) -> Result<OutputOutcome> {
        context.check()?;
        let pages = request.pages.as_deref();
        let page = pages.and_then(|pages| pages.first().copied()).unwrap_or(1);
        let paged = matches!(request.format, OutputFormat::Svg | OutputFormat::Png);
        if page == 0
            || (pages.is_some()
                && request.format != OutputFormat::Pdf
                && (!paged || pages.is_some_and(|pages| pages.len() != 1)))
            || (request.width.is_some() && request.format != OutputFormat::Png)
            || (request.pdf_password.is_some() && request.format != OutputFormat::Pdf)
            || ((request.png_quality.is_some() || request.png_transparent.is_some())
                && request.format != OutputFormat::Png)
        {
            return Err(AppError::invalid(
                "output options do not match the selected format",
            ));
        }
        if request
            .png_quality
            .is_some_and(|quality| !(1..=100).contains(&quality))
        {
            return Err(AppError::invalid("PNG quality must be 1..100"));
        }
        if request
            .pdf_password
            .as_ref()
            .is_some_and(|password| password.is_empty() || password.chars().count() > 128)
        {
            return Err(AppError::invalid(
                "PDF password must contain 1..128 characters",
            ));
        }
        if request.audio_range.is_some()
            && !matches!(request.format, OutputFormat::Midi | OutputFormat::Wav)
        {
            return Err(AppError::invalid("audioRange supports only MIDI and WAV"));
        }
        if let Some(range) = request.audio_range {
            range.validate()?;
        }
        let (snapshot, engine_build_id, checked) = match &request.source {
            OutputSource::Job { job_id } => {
                let (job, result) = self.checked_job(*job_id)?;
                let snapshot = self.job_snapshot(job.job_id)?.ok_or_else(not_found)?;
                (
                    snapshot,
                    result.render.engine_build_id.clone(),
                    Some(result.render),
                )
            }
            OutputSource::Version { version_id } => {
                let version = db::version(&*self.db()?, *version_id)?;
                let snapshot = SourceSnapshot::new(
                    version.document_id,
                    self.store.get(&version.source_hash)?,
                )?;
                (snapshot, engine.build_id().to_owned(), None)
            }
        };
        let checked = checked.as_ref();
        if request.audio_range.is_some()
            && ossia_score::source_has_partial_measure(snapshot.source())
        {
            return Err(AppError::unsupported(
                "audioRange requires stable measure numbers; pickup/partial measures are not supported",
            ));
        }
        context.report_progress("generating", None);
        let bytes = match request.format {
            OutputFormat::MusicXml => {
                if request.crop.is_some()
                    || request.padding_staff_spaces.is_some()
                    || request.output_directory.is_some()
                    || request.include_drawing == Some(true)
                    || drawing.is_some()
                {
                    return Err(AppError::invalid("MusicXML exports one complete score"));
                }
                if checked.is_some() && engine_build_id != engine.build_id() {
                    return Err(AppError::new(
                        ErrorCode::RuntimeIncompatible,
                        "check again with the current engine before generating MusicXML",
                    ));
                }
                engine.render_musicxml(&snapshot, context)?
            }
            OutputFormat::Ly => snapshot.bytes().to_vec(),
            OutputFormat::Svg => {
                let render =
                    self.output_render(engine, &snapshot, checked, RenderFormat::Svg, context)?;
                let asset = render
                    .pages
                    .iter()
                    .find(|page_asset| page_asset.page == page)
                    .ok_or_else(not_found)?;
                crate::drawing_output::overlay_svg(
                    &self.read_artifact(&asset.artifact)?,
                    page,
                    drawing,
                )?
            }
            OutputFormat::Png => {
                if checked.is_some() && engine_build_id != engine.build_id() {
                    return Err(AppError::new(
                        ErrorCode::RuntimeIncompatible,
                        "check again with the current engine before generating PNG",
                    ));
                }
                let render =
                    self.output_render(engine, &snapshot, checked, RenderFormat::Svg, context)?;
                let asset = render
                    .pages
                    .iter()
                    .find(|page_asset| page_asset.page == page)
                    .ok_or_else(not_found)?;
                let svg = crate::drawing_output::overlay_svg(
                    &self.read_artifact(&asset.artifact)?,
                    page,
                    drawing,
                )?;
                let raster = engine.rasterize_svg_with_options(
                    &svg,
                    request.width.unwrap_or(1600),
                    request.png_quality,
                    request.png_transparent.unwrap_or(false),
                    &self.artifact_root()?,
                    context,
                )?;
                self.read_artifact(&raster)?
            }
            OutputFormat::Pdf | OutputFormat::Midi => {
                let format = if request.format == OutputFormat::Pdf {
                    RenderFormat::Pdf
                } else {
                    RenderFormat::Midi
                };
                let render = self.output_render(engine, &snapshot, checked, format, context)?;
                let asset = if format == RenderFormat::Pdf {
                    render.pdf.as_ref()
                } else {
                    render.midi.as_ref()
                };
                let asset = asset.ok_or_else(|| {
                    render
                        .format_errors
                        .iter()
                        .find(|e| e.format == format)
                        .map(|e| e.error.clone())
                        .unwrap_or_else(|| AppError::unsupported("requested output is unavailable"))
                })?;
                let mut bytes = self.read_artifact(asset)?;
                if request.format == OutputFormat::Pdf {
                    if let Some(drawing) = drawing {
                        bytes = ossia_engine::overlay_pdf_drawing(&bytes, drawing)?;
                    }
                    if let Some(pages) = pages {
                        bytes = ossia_engine::select_pdf_pages(&bytes, pages)?;
                    }
                    if let Some(password) = request.pdf_password.as_deref() {
                        ossia_engine::protect_pdf(&bytes, password)?
                    } else {
                        bytes
                    }
                } else if let Some(range) = request.audio_range {
                    ossia_score::slice_midi_measures(&bytes, range)?
                } else {
                    bytes
                }
            }
            OutputFormat::Wav => {
                let render = self.audio_render(engine, &snapshot, checked, context)?;
                let mut playback = self.audio_playback(&snapshot, &render, request.audio_range)?;
                if let Some(settings) = settings {
                    for note in &mut playback.notes {
                        if let Some(instrument) = settings.instruments.get(&note.part_id) {
                            note.instrument = *instrument;
                        }
                    }
                }
                engine.render_wav(&playback, context)?
            }
        };
        context.report_progress("writing", None);
        if request.format == OutputFormat::MusicXml {
            publish_musicxml(destination, &bytes, context)?;
        } else {
            crate::crop_output::write_new(destination, &bytes)?;
        }
        let media_type = match request.format {
            OutputFormat::Ly => "text/plain; charset=utf-8",
            OutputFormat::Svg => "image/svg+xml",
            OutputFormat::Png => "image/png",
            OutputFormat::Pdf => "application/pdf",
            OutputFormat::Midi => "audio/midi",
            OutputFormat::Wav => "audio/wav",
            OutputFormat::MusicXml => "application/vnd.recordare.musicxml+xml",
        };
        Ok(OutputOutcome {
            source: request.source.clone(),
            document_id: snapshot.document_id(),
            source_hash: snapshot.source_hash().clone(),
            engine_build_id,
            format: request.format,
            page: paged.then_some(page),
            path: path_text(destination),
            bytes: bytes.len() as u64,
            sha256: ContentHash::of(&bytes),
            media_type: media_type.into(),
        })
    }

    pub(crate) fn output_render(
        &self,
        engine: &dyn EngineBackend,
        snapshot: &SourceSnapshot,
        checked: Option<&RenderManifest>,
        format: RenderFormat,
        context: &RunContext,
    ) -> Result<RenderManifest> {
        if let Some(render) = checked {
            let available = match format {
                RenderFormat::Svg => !render.pages.is_empty(),
                RenderFormat::Pdf => render.pdf.is_some(),
                RenderFormat::Midi => render.midi.is_some(),
            };
            if available && !matches!(format, RenderFormat::Pdf | RenderFormat::Svg) {
                return Ok(render.clone());
            }
            if render.engine_build_id != engine.build_id() {
                return Err(AppError::new(
                    ErrorCode::RuntimeIncompatible,
                    "check again with the current engine before generating this output",
                ));
            }
        }
        let render = engine.compile(
            snapshot,
            &CompileOptions {
                formats: vec![format],
                point_and_click: !matches!(format, RenderFormat::Pdf | RenderFormat::Svg),
            },
            &self.artifact_root()?,
            context,
        )?;
        if render.source_hash != *snapshot.source_hash()
            || render.engine_build_id != engine.build_id()
        {
            return Err(storage_error("output render does not match its snapshot"));
        }
        Ok(render)
    }
}

fn publish_musicxml(path: &Path, bytes: &[u8], context: &RunContext) -> Result<()> {
    context.check()?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::invalid("output path requires a parent"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(storage_error)?;
    file.write_all(bytes).map_err(storage_error)?;
    file.as_file().sync_all().map_err(storage_error)?;
    context.check()?;
    file.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            conflict("output destination already exists")
        } else {
            storage_error(error.error)
        }
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/artifacts.rs"]
mod tests;
