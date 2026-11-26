use crate::*;
use regex::Regex;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::OnceLock,
};

enum CropDestination<'a> {
    Batch {
        parent: &'a Path,
        directory_name: &'a str,
    },
    File(&'a Path),
}

impl Library {
    /// Generate a complete visual crop batch and publish it atomically.
    pub fn crop_output(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        parent: &Path,
        directory_name: &str,
        context: &RunContext,
    ) -> Result<OutputBatchOutcome> {
        let drawing = self.drawing_for_output(request)?;
        self.crop_into(
            engine,
            request,
            CropDestination::Batch {
                parent,
                directory_name,
            },
            context,
            drawing.as_ref(),
        )
    }

    /// Generate one visual crop at an exact path.
    pub fn crop_output_file(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        output_path: &Path,
        context: &RunContext,
    ) -> Result<OutputBatchOutcome> {
        let drawing = self.drawing_for_output(request)?;
        self.crop_into(
            engine,
            request,
            CropDestination::File(output_path),
            context,
            drawing.as_ref(),
        )
    }

    /// Generate a visual crop batch with an immutable drawing snapshot.
    pub fn crop_output_with_drawing(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        parent: &Path,
        directory_name: &str,
        context: &RunContext,
        drawing: Option<&Drawing>,
    ) -> Result<OutputBatchOutcome> {
        self.crop_into(
            engine,
            request,
            CropDestination::Batch {
                parent,
                directory_name,
            },
            context,
            drawing,
        )
    }

    /// Generate one visual crop with an immutable drawing snapshot.
    pub fn crop_output_file_with_drawing(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        output_path: &Path,
        context: &RunContext,
        drawing: Option<&Drawing>,
    ) -> Result<OutputBatchOutcome> {
        self.crop_into(
            engine,
            request,
            CropDestination::File(output_path),
            context,
            drawing,
        )
    }

    fn crop_into(
        &self,
        engine: &dyn EngineBackend,
        request: &OutputArgs,
        destination: CropDestination<'_>,
        context: &RunContext,
        drawing: Option<&Drawing>,
    ) -> Result<OutputBatchOutcome> {
        if let CropDestination::File(path) = &destination {
            if !path.is_absolute() {
                return Err(AppError::invalid("outputPath must be absolute"));
            }
        }
        let crop = request
            .crop
            .as_ref()
            .ok_or_else(|| AppError::invalid("crop output requires crop"))?;
        if !matches!(request.format, OutputFormat::Svg | OutputFormat::Png) {
            return Err(AppError::invalid("crop output supports only SVG and PNG"));
        }
        let (snapshot, engine_build_id, render) = match &request.source {
            OutputSource::Job { job_id } => {
                let (job, result) = self.checked_job(*job_id)?;
                let snapshot = self.job_snapshot(job.job_id)?.ok_or_else(not_found)?;
                let engine_build_id = result.render.engine_build_id.clone();
                (snapshot, engine_build_id, result.render)
            }
            OutputSource::Version { version_id } => {
                let snapshot = self.version_snapshot(*version_id)?;
                let render = if ossia_score::is_jianpu_source(snapshot.source()) {
                    self.render_version(engine, snapshot.document_id(), *version_id, context)?
                        .manifest
                } else {
                    self.output_render(engine, &snapshot, None, RenderFormat::Svg, context)?
                };
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
        let query = match crop {
            OutputCrop::Measures { range, staff } => ossia_score::CropQuery {
                page: request
                    .pages
                    .as_ref()
                    .and_then(|pages| pages.first().copied()),
                system_index: None,
                measure_range: Some(*range),
                staff: *staff,
                padding_staff_spaces: request.padding_staff_spaces.unwrap_or(2.0),
            },
            OutputCrop::System {
                page,
                system_index,
                staff,
            } => {
                if request
                    .pages
                    .as_ref()
                    .and_then(|pages| pages.first())
                    .is_some_and(|requested| requested != page)
                {
                    return Err(AppError::invalid("page must match the crop system page"));
                }
                ossia_score::CropQuery {
                    page: Some(*page),
                    system_index: Some(*system_index),
                    measure_range: None,
                    staff: *staff,
                    padding_staff_spaces: request.padding_staff_spaces.unwrap_or(2.0),
                }
            }
        };
        if query.page == Some(0) {
            return Err(AppError::invalid("crop output page must be at least 1"));
        }
        let regions = ossia_score::crop_regions(&layout, &query)?;
        if matches!(destination, CropDestination::File(_)) && regions.len() != 1 {
            return Err(AppError::invalid(
                "outputPath requires a crop that resolves to exactly one page; use outputDirectory for multi-page crops",
            ));
        }
        let staging = match &destination {
            CropDestination::Batch { parent, .. } => {
                fs::create_dir_all(parent).map_err(storage_error)?;
                Some(
                    tempfile::Builder::new()
                        .prefix("ossia-crop-")
                        .tempdir_in(parent)
                        .map_err(storage_error)?,
                )
            }
            CropDestination::File(_) => None,
        };
        let mut files = Vec::new();
        for region in regions {
            context.check()?;
            let page = render
                .pages
                .iter()
                .find(|page| page.page == region.page)
                .ok_or_else(not_found)?;
            let svg = crate::drawing_output::overlay_svg(
                &self.read_artifact(&page.artifact)?,
                region.page,
                drawing,
            )?;
            let cropped = crop_svg(&svg, region.bounds)?;
            let bytes = match request.format {
                OutputFormat::Svg => cropped,
                OutputFormat::Png => {
                    let raster = engine.rasterize_svg_with_options(
                        &cropped,
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
            let name = format!("page-{:03}.{}", region.page, extension);
            let (target, path) = match &destination {
                CropDestination::Batch {
                    parent,
                    directory_name,
                } => {
                    let staged = staging
                        .as_ref()
                        .expect("batch crop staging")
                        .path()
                        .join(&name);
                    (staged, parent.join(directory_name).join(&name))
                }
                CropDestination::File(path) => (path.to_path_buf(), path.to_path_buf()),
            };
            write_new(&target, &bytes)?;
            files.push(OutputFile {
                page: region.page,
                bounds: GeometryBounds {
                    x0: region.bounds.x0,
                    y0: region.bounds.y0,
                    x1: region.bounds.x1,
                    y1: region.bounds.y1,
                },
                path: path_text(path),
                bytes: bytes.len() as u64,
                sha256: ContentHash::of(&bytes),
                media_type: if request.format == OutputFormat::Svg {
                    "image/svg+xml"
                } else {
                    "image/png"
                }
                .into(),
            });
        }
        if let Err(error) = context.check() {
            if let CropDestination::File(path) = &destination {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
        let directory = match &destination {
            CropDestination::Batch {
                parent,
                directory_name,
            } => {
                let directory = parent.join(directory_name);
                if directory.exists() {
                    return Err(conflict("crop output directory already exists"));
                }
                fs::rename(
                    staging.as_ref().expect("batch crop staging").path(),
                    &directory,
                )
                .map_err(storage_error)?;
                directory
            }
            CropDestination::File(path) => path
                .parent()
                .ok_or_else(|| AppError::invalid("outputPath must have a parent directory"))?
                .to_path_buf(),
        };
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

fn crop_svg(svg: &[u8], bounds: ossia_score::BBox) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(svg).map_err(storage_error)?;
    let start = text
        .find("<svg")
        .ok_or_else(|| AppError::invalid("SVG root is missing"))?;
    let open_end = text[start..]
        .find('>')
        .map(|offset| start + offset + 1)
        .ok_or_else(|| AppError::invalid("SVG root is incomplete"))?;
    let close = text
        .rfind("</svg>")
        .ok_or_else(|| AppError::invalid("SVG root is incomplete"))?;
    static ROOT_DIMENSIONS: OnceLock<Regex> = OnceLock::new();
    let attributes = ROOT_DIMENSIONS
        .get_or_init(|| {
            Regex::new(r#"(?i)\s(?:width|height|viewBox)\s*=\s*"[^"]*""#)
                .expect("static SVG attribute regex")
        })
        .replace_all(&text[..open_end], "")
        .into_owned();
    let width = (bounds.x1 - bounds.x0).ceil().max(1.0);
    let height = (bounds.y1 - bounds.y0).ceil().max(1.0);
    let output = format!(
        "{attributes} width=\"{width}\" height=\"{height}\" viewBox=\"{} {} {} {}\">{}</svg>",
        bounds.x0,
        bounds.y0,
        bounds.x1 - bounds.x0,
        bounds.y1 - bounds.y0,
        &text[open_end..close]
    );
    Ok(output.into_bytes())
}

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(conflict("output destination already exists"));
        }
        Err(error) => {
            return Err(storage_error(format!(
                "cannot create output {}: {error}",
                path.display()
            )))
        }
    };
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(storage_error(format!(
            "cannot write output {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/crop_output.rs"]
mod tests;
