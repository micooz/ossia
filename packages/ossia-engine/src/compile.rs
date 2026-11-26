use super::*;
use crate::cache::Cache;

/// Cache identity of the current render derivation and point-and-click setting.
pub fn render_options_hash(point_and_click: bool) -> ContentHash {
    ContentHash::of(
        &serde_json::to_vec(&serde_json::json!({
            "pointAndClick": point_and_click, "derivationVersion": 8,
            "interactionVersion": ossia_types::INTERACTION_VERSION,
        }))
        .expect("render options serialize"),
    )
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;

impl Engine {
    pub fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest, AppError> {
        context.check()?;
        source::validate(snapshot.source(), &self.root, &self.version)?;
        let formats: BTreeSet<_> = options.formats.iter().copied().collect();
        if formats.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidRequest,
                "request at least one output format",
            ));
        }
        let options_hash = render_options_hash(options.point_and_click);
        let mut result = RenderManifest {
            source_hash: snapshot.source_hash().clone(),
            engine_build_id: self.build_id().into(),
            options_hash,
            pages: Vec::new(),
            pdf: None,
            midi: None,
            diagnostics: Vec::new(),
            format_errors: Vec::new(),
        };
        let cache = Cache::new(artifact_root)?;
        for format in formats {
            context.check()?;
            let key = Cache::key(
                snapshot.source_hash(),
                self.build_id(),
                &result.options_hash,
                format,
            );
            let _lock = cache.lock(&key, context)?;
            let one = match cache.read(&key, &result, format)? {
                Some(one) => Ok(one),
                None => self.compile_format(snapshot, options, format, &cache, &result, context),
            };
            match one {
                Ok(one) => {
                    context.check()?;
                    cache.publish(&key, &one)?;
                    result.pages.extend(one.pages);
                    if one.pdf.is_some() {
                        result.pdf = one.pdf;
                    }
                    if one.midi.is_some() {
                        result.midi = one.midi;
                    }
                    result.diagnostics.extend(one.diagnostics);
                }
                Err(error)
                    if error.code == ErrorCode::Cancelled
                        || error
                            .detail
                            .as_ref()
                            .and_then(|d| d.get("reason"))
                            .and_then(|r| r.as_str())
                            == Some("timeout") =>
                {
                    return Err(error)
                }
                Err(error) => {
                    if let Some(ds) = error.detail.as_ref().and_then(|v| v.get("diagnostics")) {
                        if let Ok(ds) = serde_json::from_value::<Vec<Diagnostic>>(ds.clone()) {
                            result.diagnostics.extend(ds);
                        }
                    }
                    result.format_errors.push(FormatError { format, error });
                }
            }
        }
        // Preserve order without duplicating the same source diagnostic across passes.
        let mut unique = Vec::new();
        for diagnostic in result.diagnostics {
            if !unique.contains(&diagnostic) {
                unique.push(diagnostic);
            }
        }
        result.diagnostics = unique;
        Ok(result)
    }

    fn compile_format(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        format: RenderFormat,
        cache: &Cache,
        template: &RenderManifest,
        context: &RunContext,
    ) -> Result<RenderManifest, AppError> {
        let original = snapshot.source();
        let jianpu = format != RenderFormat::Midi && ossia_score::is_jianpu_source(original);
        let derived = source::derive(original, format, options.point_and_click)?;
        source::validate_render_structure(&derived.text)?;
        let scratch = Scratch::new(&self.scratch_root)?;
        fs::write(scratch.path().join("score.ly"), &derived.text).map_err(io_error)?;
        let mut args = vec!["-dno-strip-output-dir", "-o", "score"];
        if options.point_and_click {
            args.push("-dpoint-and-click=#t");
        } else {
            args.push("-dpoint-and-click=#f");
        }
        if format == RenderFormat::Svg {
            args.push("--svg");
        }
        if format == RenderFormat::Midi {
            args.push("-dno-print-pages");
        }
        args.push("score.ly");
        let output = self.run("lilypond", &args, &scratch, context)?;
        let diagnostics = diagnostics::parse(
            &output.log,
            scratch.path(),
            &self.root,
            Some(&derived),
            Some(original),
        );
        if let Some(feature) = diagnostics
            .iter()
            .find_map(|diagnostic| {
                diagnostic
                    .message
                    .split_once("warning: Ossia Jianpu: ")
                    .map(|(_, feature)| feature.to_owned())
                    .or_else(|| {
                        diagnostic
                            .message
                            .split_once("warning: cannot find context to change to: Staff = ")
                            .map(|(_, target)| format!("an unresolved staff change to {target}"))
                    })
            })
            .filter(|_| jianpu)
        {
            return Err(AppError {
                code: ErrorCode::UnsupportedSource,
                message: format!("Jianpu layout does not support {feature}"),
                detail: Some(serde_json::json!({"diagnostics": diagnostics})),
            });
        }
        if !output.success {
            return Err(AppError {
                code: ErrorCode::CompileFailed,
                message: "LilyPond compilation failed".into(),
                detail: Some(serde_json::json!({"diagnostics":diagnostics})),
            });
        }
        let files = collect(scratch.path(), format)?;
        let mut result = RenderManifest {
            source_hash: template.source_hash.clone(),
            engine_build_id: template.engine_build_id.clone(),
            options_hash: template.options_hash.clone(),
            pages: Vec::new(),
            pdf: None,
            midi: None,
            diagnostics,
            format_errors: Vec::new(),
        };
        for (page, path) in files {
            let mut bytes = fs::read(path).map_err(io_error)?;
            if bytes.is_empty() {
                return Err(failure(
                    "missing-output",
                    "engine produced an empty artifact",
                ));
            }
            let extension = match format {
                RenderFormat::Svg => {
                    let svg = std::str::from_utf8(&bytes)
                        .map_err(|_| AppError::invalid("SVG is not UTF-8"))?;
                    let svg = source::normalize_svg(svg, &derived, original)?;
                    if options.point_and_click
                        && jianpu
                        && !svg.contains("data-ossia-kind=\"JianpuStaff\"")
                    {
                        return Err(AppError::unsupported(
                            "Jianpu staff geometry is unavailable",
                        ));
                    }
                    bytes = svg.into_bytes();
                    "svg"
                }
                RenderFormat::Pdf => {
                    if !bytes.starts_with(b"%PDF-") {
                        return Err(AppError::invalid("invalid PDF output"));
                    }
                    "pdf"
                }
                RenderFormat::Midi => {
                    let midi = ossia_score::parse_midi(&bytes)?;
                    if midi.tracks.iter().all(|t| t.notes.is_empty()) {
                        return Err(AppError::unsupported("MIDI output has no note events"));
                    }
                    "midi"
                }
            };
            let artifact = cache.asset(&bytes, extension)?;
            match format {
                RenderFormat::Svg => result.pages.push(RenderPage { page, artifact }),
                RenderFormat::Pdf => result.pdf = Some(artifact),
                RenderFormat::Midi => result.midi = Some(artifact),
            }
        }
        Ok(result)
    }
}

pub(crate) fn collect(
    directory: &Path,
    format: RenderFormat,
) -> Result<Vec<(u32, PathBuf)>, AppError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let extension = name.rsplit('.').next().unwrap_or("");
        let requested = match format {
            RenderFormat::Svg => extension == "svg",
            RenderFormat::Pdf => extension == "pdf",
            RenderFormat::Midi => matches!(extension, "midi" | "mid"),
        };
        if !requested {
            continue;
        }
        if !entry.file_type().map_err(io_error)?.is_file() {
            return Err(AppError::unsupported("engine output is not a regular file"));
        }
        let stem = name.strip_suffix(&format!(".{extension}")).unwrap();
        let page = if stem == "score" {
            1
        } else if format == RenderFormat::Svg {
            stem.strip_prefix("score-")
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|p| *p > 0)
                .ok_or_else(|| AppError::unsupported("ambiguous engine output group"))?
        } else {
            return Err(AppError::unsupported(
                "multiple books or named outputs are not supported",
            ));
        };
        files.push((page, entry.path()));
    }
    files.sort_by_key(|(page, _)| *page);
    if files.is_empty() {
        return Err(AppError::unsupported("engine produced no requested output"));
    }
    if (format != RenderFormat::Svg && files.len() != 1)
        || files
            .iter()
            .enumerate()
            .any(|(i, (p, _))| *p as usize != i + 1)
    {
        return Err(AppError::unsupported(
            "missing pages or ambiguous output groups",
        ));
    }
    Ok(files)
}
