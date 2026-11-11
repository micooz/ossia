use crate::*;
use std::path::PathBuf;

impl Runtime {
    /// Page/system/staff structure from a completed import or check snapshot.
    pub fn layout(&self, args: LayoutArgs) -> Result<LayoutOutcome> {
        let (job, result) = self.library.checked_job(args.job_id)?;
        let snapshot = self
            .library
            .job_snapshot(job.job_id)?
            .ok_or_else(|| storage("layout source snapshot is missing"))?;
        let index = ossia_score::build_index(snapshot.source());
        let sources = result
            .render
            .pages
            .iter()
            .map(|page| {
                Ok((
                    page.page,
                    String::from_utf8(self.library.read_artifact(&page.artifact)?)
                        .map_err(storage)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let pages = sources
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect::<Vec<_>>();
        let layout = ossia_score::build_layout(&index, &pages)?;
        let pages = layout
            .pages
            .iter()
            .map(|page| LayoutPage {
                page: page.page,
                view_box: page.view_box.map(|b| GeometryBounds {
                    x0: b.x0,
                    y0: b.y0,
                    x1: b.x1,
                    y1: b.y1,
                }),
                staff_space: page.staff_space,
                systems: page
                    .systems
                    .iter()
                    .map(|system| LayoutSystem {
                        top: system.top,
                        bot: system.bot,
                        bars: system.bar_xs.len(),
                        staves: system
                            .staves
                            .iter()
                            .map(|staff| LayoutStaff {
                                top: staff.top,
                                bot: staff.bot,
                                x0: staff.x0,
                                x1: staff.x1,
                            })
                            .collect(),
                    })
                    .collect(),
                capability: page.capability,
                reason: page.reason.clone(),
            })
            .collect::<Vec<_>>();
        Ok(LayoutOutcome {
            job_id: args.job_id,
            document_id: snapshot.document_id(),
            source_hash: snapshot.source_hash().clone(),
            page_count: pages.len(),
            system_count: pages.iter().map(|page| page.systems.len()).sum(),
            staff_count: pages
                .iter()
                .flat_map(|page| &page.systems)
                .map(|system| system.staves.len())
                .sum(),
            pages,
        })
    }

    pub fn geometry(&self, key: &ContentHash) -> Result<RenderGeometry> {
        let (_, manifest) = self.library.render(key)?;
        let snapshot = self.library.render_snapshot(key)?;
        let index = ossia_score::build_index(snapshot.source());
        let sources = manifest
            .pages
            .iter()
            .map(|page| {
                Ok((
                    page.page,
                    String::from_utf8(self.library.read_artifact(&page.artifact)?)
                        .map_err(storage)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let pages = sources
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect::<Vec<_>>();
        let layout = ossia_score::build_layout(&index, &pages)?;
        let mut output = Vec::new();
        for page in &layout.pages {
            let mut regions = Vec::new();
            for col in layout.columns.iter().filter(|c| c.page == page.page) {
                let Some(system) = page.systems.get(col.system_index) else {
                    continue;
                };
                for staff in &system.staves {
                    let result = ossia_score::locate(
                        &index,
                        &layout,
                        &ossia_score::LocateQuery::Point {
                            page: page.page,
                            x: (col.x0 + col.x1) / 2.,
                            y: (staff.top + staff.bot) / 2.,
                        },
                    )?;
                    if let Some(region) = result
                        .region
                        .filter(|r| r.capability == LocationCapability::Measure)
                    {
                        regions.push(GeometryRegion {
                            locator: region.locator,
                            system_index: col.system_index,
                            bounds: GeometryBounds {
                                x0: col.x0.max(staff.x0),
                                x1: col.x1.min(staff.x1),
                                y0: staff.top,
                                y1: staff.bot,
                            },
                        });
                    }
                }
            }
            output.push(GeometryPage {
                page: page.page,
                view_box: page.view_box.map(|b| GeometryBounds {
                    x0: b.x0,
                    y0: b.y0,
                    x1: b.x1,
                    y1: b.y1,
                }),
                capability: page.capability,
                reason: page.reason.clone(),
                regions,
            });
        }
        Ok(RenderGeometry {
            render_key: key.clone(),
            source_hash: manifest.source_hash,
            pages: output,
        })
    }

    /// Resolve a recorded output, never a caller-supplied filesystem path.
    pub fn export_location(&self, job: JobId) -> Result<PathBuf> {
        let job = self.library.get_job(job)?;
        if job.kind != "output" || job.state != JobState::Done {
            return Err(invalid("only completed outputs have an openable location"));
        }
        let outcome: OutputOutcome =
            decode(job.result.ok_or_else(|| storage("output result missing"))?)?;
        let path = PathBuf::from(outcome.path);
        let meta = std::fs::symlink_metadata(&path).map_err(storage)?;
        if !path.is_absolute() || !meta.is_file() || meta.file_type().is_symlink() {
            return Err(storage("output is no longer a regular file"));
        }
        let bytes = std::fs::read(&path).map_err(storage)?;
        if bytes.len() as u64 != outcome.bytes || ContentHash::of(&bytes) != outcome.sha256 {
            return Err(storage("output changed since it was saved"));
        }
        Ok(path)
    }

    pub fn reveal_export(&self, job: JobId) -> Result<()> {
        let path = self.export_location(job)?;
        #[cfg(target_os = "macos")]
        let status = std::process::Command::new("/usr/bin/open")
            .arg("-R")
            .arg(&path)
            .status();
        #[cfg(target_os = "windows")]
        let status = std::process::Command::new("explorer.exe")
            .arg(
                path.parent()
                    .ok_or_else(|| invalid("export has no parent"))?,
            )
            .status();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let status = std::process::Command::new("xdg-open")
            .arg(
                path.parent()
                    .ok_or_else(|| invalid("export has no parent"))?,
            )
            .status();
        if !status.map_err(storage)?.success() {
            return Err(AppError::new(
                ErrorCode::RuntimeUnavailable,
                "file manager could not open the export location",
            ));
        }
        Ok(())
    }
}
