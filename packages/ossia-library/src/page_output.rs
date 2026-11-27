use crate::*;
use std::{collections::HashMap, fs, path::Path};

impl Library {
    /// Render selected SVG/PNG pages into one atomic batch directory.
    pub fn page_output(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        parent: &Path,
        directory_name: &str,
        context: &RunContext,
    ) -> Result<OutputBatchOutcome> {
        let drawing = self.drawing_for_output(request)?;
        self.page_output_with_drawing(
            engine,
            request,
            parent,
            directory_name,
            context,
            drawing.as_ref(),
        )
    }

    /// Render a page batch with an immutable drawing snapshot.
    pub fn page_output_with_drawing(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        parent: &Path,
        directory_name: &str,
        context: &RunContext,
        drawing: Option<&Drawing>,
    ) -> Result<OutputBatchOutcome> {
        if !matches!(request.format, OutputFormat::Svg | OutputFormat::Png) {
            return Err(AppError::invalid("page output supports only SVG and PNG"));
        }
        let mut pages = request
            .pages
            .clone()
            .ok_or_else(|| AppError::invalid("page output requires pages"))?;
        if pages.is_empty() {
            return Err(AppError::invalid("pages must contain at least one page"));
        }
        pages.sort_unstable();
        if pages.contains(&0) || pages.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AppError::invalid("pages must be positive and unique"));
        }
        context.report_progress("generating", None);
        let (snapshot, engine_build_id, render) = match &request.source {
            OutputSource::Job { job_id } => {
                let (job, result) = self.checked_job(*job_id)?;
                let snapshot = self.job_snapshot(job.job_id)?.ok_or_else(not_found)?;
                let engine_build_id = result.render.engine_build_id.clone();
                (snapshot, engine_build_id, result.render)
            }
            OutputSource::Version { version_id } => {
                let snapshot = self.version_snapshot(*version_id)?;
                let render =
                    self.output_render(engine, &snapshot, None, RenderFormat::Svg, context)?;
                (snapshot, engine.build_id().to_owned(), render)
            }
        };
        if request.format == OutputFormat::Png && engine_build_id != engine.build_id() {
            return Err(AppError::new(
                ErrorCode::RuntimeIncompatible,
                "check again with the current engine before generating PNG",
            ));
        }
        if render.pages.is_empty() {
            return Err(compile_error(&render));
        }
        let svg_texts = render
            .pages
            .iter()
            .map(|page| {
                String::from_utf8(self.read_artifact(&page.artifact)?).map_err(storage_error)
            })
            .collect::<Result<Vec<_>>>()?;
        let svg_pages = render
            .pages
            .iter()
            .zip(&svg_texts)
            .map(|(page, svg)| ossia_score::SvgPage {
                page: page.page,
                svg,
            })
            .collect::<Vec<_>>();
        let layout =
            ossia_score::build_layout(&ossia_score::build_index(snapshot.source()), &svg_pages)?;
        let mut bounds = HashMap::new();
        for page in &layout.pages {
            let view_box = page
                .view_box
                .ok_or_else(|| AppError::invalid("page viewBox is unavailable"))?;
            bounds.insert(
                page.page,
                GeometryBounds {
                    x0: view_box.x0,
                    y0: view_box.y0,
                    x1: view_box.x1,
                    y1: view_box.y1,
                },
            );
        }
        for page in &pages {
            if !render.pages.iter().any(|asset| asset.page == *page) {
                return Err(not_found());
            }
            if !bounds.contains_key(page) {
                return Err(AppError::invalid("page bounds are unavailable"));
            }
        }
        fs::create_dir_all(parent).map_err(storage_error)?;
        let staging = tempfile::Builder::new()
            .prefix("ossia-pages-")
            .tempdir_in(parent)
            .map_err(storage_error)?;
        let mut files = Vec::new();
        let total = u32::try_from(pages.len()).map_err(|_| AppError::invalid("too many pages"))?;
        context.report_progress("pages", Some((0, total)));
        for page in &pages {
            context.check()?;
            let asset = render
                .pages
                .iter()
                .find(|asset| asset.page == *page)
                .ok_or_else(not_found)?;
            let svg = crate::drawing_output::overlay_svg(
                &self.read_artifact(&asset.artifact)?,
                *page,
                drawing,
            )?;
            let bytes = match request.format {
                OutputFormat::Svg => svg,
                OutputFormat::Png => {
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
                _ => unreachable!(),
            };
            let extension = if request.format == OutputFormat::Svg {
                "svg"
            } else {
                "png"
            };
            let name = format!("page-{:03}.{}", page, extension);
            let staged = staging.path().join(&name);
            crate::crop_output::write_new(&staged, &bytes)?;
            let path = parent.join(directory_name).join(&name);
            files.push(OutputFile {
                page: *page,
                bounds: bounds[page].clone(),
                path: path_text(&path),
                bytes: bytes.len() as u64,
                sha256: ContentHash::of(&bytes),
                media_type: if request.format == OutputFormat::Svg {
                    "image/svg+xml"
                } else {
                    "image/png"
                }
                .into(),
            });
            context.report_progress("pages", Some((files.len() as u32, total)));
        }
        context.report_progress("writing", None);
        context.check()?;
        let directory = parent.join(directory_name);
        if directory.exists() {
            return Err(conflict("page output directory already exists"));
        }
        fs::rename(staging.path(), &directory).map_err(storage_error)?;
        Ok(OutputBatchOutcome {
            source: request.source.clone(),
            document_id: snapshot.document_id(),
            source_hash: snapshot.source_hash().clone(),
            engine_build_id,
            format: request.format,
            directory: path_text(&directory),
            files,
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/page_output.rs"]
mod tests;
