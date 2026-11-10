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
    pub fn edit_activity(
        &self,
        document: DocumentId,
        view: ViewId,
    ) -> Result<Option<EditActivity>> {
        Ok(
            db::active_edit(&*self.db()?, document)?.map(|edit| EditActivity {
                author: edit.author,
                status: edit.status,
                owned_by_view: edit.owner_view_id == view,
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
        let options_hash = ContentHash::of(&serde_json::to_vec(&options).map_err(storage_error)?);
        let cached: Option<String> = {
            use rusqlite::OptionalExtension;
            self.db()?.query_row("SELECT body FROM renders WHERE document_id=?1 AND source_hash=?2 AND engine_build_id=?3 AND options_hash=?4",
                rusqlite::params![document.to_string(), version.source_hash.to_string(), engine.build_id(), options_hash.to_string()], |r| r.get(0)).optional().map_err(storage_error)?
        };
        let snapshot = self.version_snapshot(version.version_id)?;
        let render = if let Some(body) = cached {
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
            .prepare("SELECT document_id,body FROM renders")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(storage_error)?;
        for row in rows {
            let (document, body) = row.map_err(storage_error)?;
            let document = db::parse(document)?;
            let render: RenderManifest = db::decode(body)?;
            if &render_key(document, &render) == key {
                return Ok((document, render));
            }
        }
        Err(not_found())
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

    pub fn export_version(
        &self,
        engine: &dyn EngineBackend,
        request: &ExportArgs,
        destination: &Path,
        context: &RunContext,
    ) -> Result<ExportOutcome> {
        let version = self.get_version(request.document_id, request.version_id)?;
        let snapshot = self.version_snapshot(version.version_id)?;
        let bytes = match request.format {
            ExportFormat::Ly => snapshot.bytes().to_vec(),
            ExportFormat::Pdf | ExportFormat::Midi => {
                let format = if matches!(request.format, ExportFormat::Pdf) {
                    RenderFormat::Pdf
                } else {
                    RenderFormat::Midi
                };
                let render = engine.compile(
                    &snapshot,
                    &CompileOptions {
                        formats: vec![format],
                        point_and_click: true,
                    },
                    &self.artifact_root()?,
                    context,
                )?;
                if render.source_hash != version.source_hash
                    || render.engine_build_id != engine.build_id()
                {
                    return Err(storage_error("export render does not match version"));
                }
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
                        .unwrap_or_else(|| AppError::unsupported("requested export is unavailable"))
                })?;
                self.read_artifact(asset)?
            }
        };
        context.check()?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    conflict("export destination already exists")
                } else {
                    storage_error(e)
                }
            })?;
        if let Err(error) = output.write_all(&bytes).and_then(|_| output.sync_all()) {
            drop(output);
            let _ = fs::remove_file(destination);
            return Err(storage_error(error));
        }
        Ok(ExportOutcome {
            document_id: version.document_id,
            version_id: version.version_id,
            source_hash: version.source_hash,
            format: request.format,
            path: destination.to_string_lossy().into_owned(),
            bytes: bytes.len() as u64,
            sha256: ContentHash::of(&bytes),
        })
    }
}
