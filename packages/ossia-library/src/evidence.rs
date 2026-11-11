use crate::*;

impl Library {
    /// Read one cached evidence report for an immutable source and engine build.
    pub fn cached_evidence(
        &self,
        document: DocumentId,
        source_hash: &ContentHash,
        engine_build_id: &str,
        analysis_version: u32,
    ) -> Result<Option<EvidenceReport>> {
        let conn = self.db()?;
        let row = conn
            .query_row(
                "SELECT rowid,body FROM evidence_runs WHERE document_id=?1 AND source_hash=?2 AND engine_build_id=?3 AND analysis_version=?4",
                params![
                    document.to_string(),
                    source_hash.to_string(),
                    engine_build_id,
                    i64::from(analysis_version)
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let Some((rowid, body)) = row else {
            return Ok(None);
        };
        let report: EvidenceReport = db::decode(body)?;
        if report.document_id != document
            || report.source_hash != *source_hash
            || report.engine_build_id != engine_build_id
            || report.analysis_version != analysis_version
        {
            return Err(storage_error("evidence cache key does not match its body"));
        }
        conn.execute(
            "UPDATE evidence_runs SET last_used_at=?1 WHERE rowid=?2",
            params![db::now_millis(), rowid],
        )
        .map_err(storage_error)?;
        Ok(Some(report))
    }

    /// Persist deterministic evidence. Callers must not store transient engine failures.
    pub fn save_evidence(&self, report: &EvidenceReport) -> Result<()> {
        if report.analysis_version == 0 {
            return Err(storage_error("evidence report has no analysis version"));
        }
        let now = db::now_millis();
        self.db()?
            .execute(
                "INSERT INTO evidence_runs(document_id,source_hash,engine_build_id,analysis_version,body,created_at,last_used_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?6)
                 ON CONFLICT(document_id,source_hash,engine_build_id,analysis_version)
                 DO UPDATE SET body=excluded.body,last_used_at=excluded.last_used_at",
                params![
                    report.document_id.to_string(),
                    report.source_hash.to_string(),
                    report.engine_build_id,
                    i64::from(report.analysis_version),
                    db::encode(report)?,
                    now
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    /// Derive source and performed-note evidence from one captured snapshot.
    /// The boolean is false when a transient engine failure prevents performance evidence.
    pub fn derive_evidence(
        &self,
        engine: &dyn EngineBackend,
        snapshot: &SourceSnapshot,
        context: &RunContext,
    ) -> Result<(EvidenceReport, bool)> {
        context.check()?;
        let source = ossia_score::analyze_source(snapshot.source());
        let options = CompileOptions {
            formats: vec![RenderFormat::Midi],
            point_and_click: false,
        };
        let (music, cacheable) =
            match engine.compile(snapshot, &options, &self.artifact_root()?, context) {
                Ok(render) => {
                    if render.source_hash != *snapshot.source_hash() {
                        return Err(storage_error(
                            "evidence render does not match its source snapshot",
                        ));
                    }
                    let cacheable = render.midi.is_some();
                    (
                        MusicReport {
                            source,
                            performance: self.performance_from_render(&render)?,
                        },
                        cacheable,
                    )
                }
                Err(error) if error.code == ErrorCode::Cancelled => return Err(error),
                Err(error) => (
                    MusicReport {
                        source,
                        performance: ossia_score::unavailable_performance(error.message),
                    },
                    false,
                ),
            };
        Ok((
            EvidenceReport {
                document_id: snapshot.document_id(),
                source_hash: snapshot.source_hash().clone(),
                engine_build_id: engine.build_id().to_owned(),
                analysis_version: EVIDENCE_ANALYSIS_VERSION,
                music,
            },
            cacheable,
        ))
    }

    /// Reuse a completed check as evidence when it was produced by current semantics.
    pub fn cache_check_evidence(&self, document: DocumentId, result: &CheckResult) -> Result<()> {
        if result.analysis_version != EVIDENCE_ANALYSIS_VERSION || result.render.midi.is_none() {
            return Ok(());
        }
        self.save_evidence(&EvidenceReport {
            document_id: document,
            source_hash: result.render.source_hash.clone(),
            engine_build_id: result.render.engine_build_id.clone(),
            analysis_version: result.analysis_version,
            music: result.music.clone(),
        })
    }

    fn performance_from_render(&self, render: &RenderManifest) -> Result<PerformanceAnalysis> {
        Ok(match &render.midi {
            Some(asset) => ossia_score::analyze_performance(&ossia_score::parse_midi(
                &self.read_artifact(asset)?,
            )?),
            None => ossia_score::unavailable_performance(
                render
                    .format_errors
                    .iter()
                    .find(|error| error.format == RenderFormat::Midi)
                    .map(|error| error.error.message.as_str())
                    .unwrap_or("MIDI was not requested or produced"),
            ),
        })
    }
}
