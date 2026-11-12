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
            .map(|page| {
                let mapped = page
                    .systems
                    .iter()
                    .enumerate()
                    .map(|(system_index, _)| system_measure_range(&layout, page.page, system_index))
                    .collect::<Vec<_>>();
                let measure_mapping = if mapped.is_empty() {
                    MeasurementCapability::None
                } else if mapped.iter().all(Option::is_some) {
                    MeasurementCapability::Complete
                } else if mapped.iter().any(Option::is_some) {
                    MeasurementCapability::Partial
                } else {
                    MeasurementCapability::None
                };
                LayoutPage {
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
                        .zip(mapped)
                        .map(|(system, range)| LayoutSystem {
                            top: system.top,
                            bot: system.bot,
                            bars: measure_count(system),
                            measure_start: range.map(|range| range.0),
                            measure_end: range.map(|range| range.1),
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
                    measure_mapping,
                    reason: page.reason.clone(),
                }
            })
            .collect::<Vec<_>>();
        let measure_mapping = if pages.is_empty() {
            MeasurementCapability::None
        } else if pages
            .iter()
            .all(|page| page.measure_mapping == MeasurementCapability::Complete)
        {
            MeasurementCapability::Complete
        } else if pages
            .iter()
            .any(|page| page.measure_mapping != MeasurementCapability::None)
        {
            MeasurementCapability::Partial
        } else {
            MeasurementCapability::None
        };
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
            measure_mapping,
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
        self.verified_export(job).map(|(path, _, _)| path)
    }

    /// Read a recorded output after verifying its path, size and content hash.
    pub fn export_bytes(&self, job: JobId) -> Result<(OutputOutcome, Vec<u8>)> {
        self.verified_export(job)
            .map(|(_, outcome, bytes)| (outcome, bytes))
    }

    fn verified_export(&self, job: JobId) -> Result<(PathBuf, OutputOutcome, Vec<u8>)> {
        let job = self.library.get_job(job)?;
        if job.kind != "output" || job.state != JobState::Done {
            return Err(invalid(
                "only completed outputs can be downloaded or opened",
            ));
        }
        let outcome: OutputOutcome =
            decode(job.result.ok_or_else(|| storage("output result missing"))?)?;
        let path = PathBuf::from(&outcome.path);
        let meta = std::fs::symlink_metadata(&path).map_err(storage)?;
        if !path.is_absolute() || !meta.is_file() || meta.file_type().is_symlink() {
            return Err(storage("output is no longer a regular file"));
        }
        let bytes = std::fs::read(&path).map_err(storage)?;
        if bytes.len() as u64 != outcome.bytes || ContentHash::of(&bytes) != outcome.sha256 {
            return Err(storage("output changed since it was saved"));
        }
        Ok((path, outcome, bytes))
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

fn system_measure_range(
    layout: &ossia_score::ScoreLayout,
    page: u32,
    system_index: usize,
) -> Option<(u32, u32)> {
    let mut range: Option<(u32, u32)> = None;
    for column in layout
        .columns
        .iter()
        .filter(|column| column.page == page && column.system_index == system_index)
    {
        let start = column.measure_start?;
        let end = column.measure_end?;
        match range {
            None => range = Some((start, end)),
            Some((_, previous_end)) if previous_end.checked_add(1) == Some(start) => {
                range = Some((range.unwrap().0, end));
            }
            Some(_) => return None,
        }
    }
    range
}

fn measure_count(system: &ossia_score::SystemGeometry) -> usize {
    system.bar_xs.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(bar_xs: Vec<f64>) -> ossia_score::SystemGeometry {
        ossia_score::SystemGeometry {
            top: 0.,
            bot: 1.,
            staves: Vec::new(),
            bar_xs,
        }
    }

    #[test]
    fn barline_counts_become_measure_intervals() {
        for (barlines, expected) in [(2, 1), (3, 2), (4, 3), (5, 4)] {
            let xs = (0..barlines).map(f64::from).collect();
            assert_eq!(measure_count(&system(xs)), expected);
        }
    }

    #[test]
    fn measure_ranges_require_contiguous_mapped_columns() {
        let column = |column_index, measure_start, measure_end| ossia_score::MeasureColumn {
            page: 1,
            system_index: 0,
            column_index,
            x0: column_index as f64,
            x1: column_index as f64 + 1.,
            measure_start,
            measure_end,
        };
        let mut layout = ossia_score::ScoreLayout {
            source_hash: ContentHash::of(b"layout"),
            pages: Vec::new(),
            columns: vec![
                column(0, Some(1), Some(1)),
                column(1, Some(2), Some(2)),
                column(2, Some(3), Some(3)),
            ],
            capability: LocationCapability::Measure,
            reason: None,
        };
        assert_eq!(system_measure_range(&layout, 1, 0), Some((1, 3)));
        layout.columns[1].measure_start = None;
        assert_eq!(system_measure_range(&layout, 1, 0), None);
        layout.columns[1].measure_start = Some(4);
        assert_eq!(system_measure_range(&layout, 1, 0), None);
    }
}
