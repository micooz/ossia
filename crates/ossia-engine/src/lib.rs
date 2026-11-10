#![doc = include_str!("../README.md")]
mod cache;
mod diagnostics;
mod process;
mod source;

use cache::Cache;
pub use ossia_types::*;
use process::{run, tool_path, Scratch, Tool};
pub use process::{Cancellation, RunContext};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn failure(reason: &str, message: &str) -> AppError {
    AppError {
        code: ErrorCode::CompileFailed,
        message: message.into(),
        detail: Some(serde_json::json!({"reason":reason})),
    }
}
pub(crate) fn io_error(error: std::io::Error) -> AppError {
    failure(
        &format!("io-{:?}", error.kind()),
        "engine resource or artifact I/O failed",
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Lock {
    schema_version: u32,
    version: String,
    targets: BTreeMap<String, Target>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    engine_build_id: String,
    archive: Option<Archive>,
    tools: BTreeMap<String, Tool>,
    required_directories: Vec<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct Archive {
    sha256: ContentHash,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Inventory {
    schema_version: u32,
    target: String,
    engine_build_id: String,
    archive_sha256: ContentHash,
    files: Vec<ResourceFile>,
}
#[derive(Debug, Deserialize)]
struct ResourceFile {
    path: String,
    sha256: ContentHash,
    bytes: u64,
    executable: bool,
}

#[derive(Clone, Debug)]
pub struct Engine {
    root: PathBuf,
    scratch_root: PathBuf,
    version: String,
    target: Target,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileOptions {
    pub formats: Vec<RenderFormat>,
    pub point_and_click: bool,
}
impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            formats: vec![RenderFormat::Svg],
            point_and_click: true,
        }
    }
}

pub fn host_target() -> Result<&'static str, AppError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("darwin-arm64"),
        ("macos", "x86_64") => Ok("darwin-x86_64"),
        ("windows", "x86_64") => Ok("mingw-x86_64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("linux", "x86_64") => Ok("linux-x86_64"),
        _ => Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            "unsupported engine target",
        )),
    }
}

impl Engine {
    /// Verifies prepared, immutable resources and runs native converter/version and SVG smoke probes.
    pub fn verify(
        root: &Path,
        scratch_root: &Path,
        context: &RunContext,
    ) -> Result<Self, AppError> {
        context.check()?;
        let lock: Lock = serde_json::from_str(include_str!("../../../vendor/ENGINE.lock"))
            .map_err(|_| failure("engine-lock", "invalid embedded engine lock"))?;
        if lock.schema_version != 1 || lock.version != "2.26.0" {
            return Err(failure("engine-lock", "unsupported engine lock"));
        }
        let target_name = host_target()?;
        let target = lock
            .targets
            .get(target_name)
            .ok_or_else(|| failure("target", "engine target is not locked"))?
            .clone();
        let archive = target.archive.as_ref().ok_or_else(|| {
            AppError::new(
                ErrorCode::RuntimeIncompatible,
                "engine target has not been built and locked",
            )
        })?;
        let root = root.canonicalize().map_err(io_error)?;
        let inventory: Inventory = serde_json::from_slice(
            &fs::read(cache::confined(&root, "resources.json")?).map_err(io_error)?,
        )
        .map_err(|_| {
            failure(
                "integrity",
                "invalid engine resource inventory; prepare the engine again",
            )
        })?;
        if inventory.schema_version != 1
            || inventory.target != target_name
            || inventory.engine_build_id != target.engine_build_id
            || inventory.archive_sha256 != archive.sha256
        {
            return Err(failure(
                "integrity",
                "engine resources do not match the locked build",
            ));
        }
        let mut paths = BTreeSet::new();
        for file in &inventory.files {
            context.check()?;
            if !paths.insert(file.path.clone()) {
                return Err(failure("integrity", "duplicate resource entry"));
            }
            let path = cache::confined(&root, &file.path)?;
            let bytes = fs::read(&path).map_err(io_error)?;
            if bytes.len() as u64 != file.bytes || ContentHash::of(&bytes) != file.sha256 {
                return Err(failure("integrity", "engine resource is damaged"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if file.executable
                    && fs::metadata(path).map_err(io_error)?.permissions().mode() & 0o111 == 0
                {
                    return Err(failure(
                        "integrity",
                        "engine resource lost executable permissions",
                    ));
                }
            }
            #[cfg(not(unix))]
            let _ = file.executable;
        }
        let mut actual = BTreeSet::new();
        resource_files(&root, &root, &mut actual)?;
        if actual != paths {
            return Err(failure(
                "integrity",
                "engine resource inventory is incomplete",
            ));
        }
        for required in &target.required_directories {
            let path = cache::confined(&root, required)?;
            if !path.is_dir() || fs::read_dir(path).map_err(io_error)?.next().is_none() {
                return Err(failure(
                    "missing-resource",
                    "engine dependency directory is missing or empty",
                ));
            }
        }
        for name in ["lilypond", "musicxml2ly", "midi2ly"] {
            let tool = target
                .tools
                .get(name)
                .ok_or_else(|| failure("missing-tool", "engine tool is not declared"))?;
            tool_path(&root, &tool.program)?;
            for arg in &tool.args {
                tool_path(&root, arg)?;
            }
        }
        fs::create_dir_all(scratch_root).map_err(io_error)?;
        let engine = Self {
            root,
            scratch_root: scratch_root.canonicalize().map_err(io_error)?,
            version: lock.version,
            target,
        };
        let version = regex::Regex::new(r"\b2\.26\.0(?:\s|$)").unwrap();
        for name in ["lilypond", "musicxml2ly", "midi2ly"] {
            let scratch = Scratch::new(&engine.scratch_root)?;
            let output = engine.run(name, &["--version"], &scratch, context)?;
            if !output.success || !version.is_match(&output.log) {
                return Err(AppError::new(
                    ErrorCode::RuntimeIncompatible,
                    "engine tool version does not match 2.26.0",
                ));
            }
        }
        let scratch = Scratch::new(&engine.scratch_root)?;
        fs::write(
            scratch.path().join("score.ly"),
            "\\version \"2.26.0\"\n\\score { { c'1 } \\layout {} }\n",
        )
        .map_err(io_error)?;
        let output = engine.run(
            "lilypond",
            &["--svg", "-o", "score", "score.ly"],
            &scratch,
            context,
        )?;
        if !output.success || !scratch.path().join("score.svg").is_file() {
            return Err(failure("engine-smoke", "engine SVG smoke test failed"));
        }
        Ok(engine)
    }
    /// `resources` is resolved by the executable/bundle owner, never from the process cwd.
    pub fn from_resources(
        resources: &Path,
        scratch_root: &Path,
        context: &RunContext,
    ) -> Result<Self, AppError> {
        Self::verify(&resources.join("engine"), scratch_root, context)
    }
    pub fn build_id(&self) -> &str {
        &self.target.engine_build_id
    }
    fn run(
        &self,
        name: &str,
        args: &[&str],
        scratch: &Scratch,
        context: &RunContext,
    ) -> Result<process::Output, AppError> {
        let tool = &self.target.tools[name];
        let prefix: Vec<_> = tool
            .args
            .iter()
            .map(|arg| cache::confined(&self.root, arg).map(|p| p.to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?;
        run(
            &tool_path(&self.root, &tool.program)?,
            &prefix,
            args,
            scratch,
            context,
        )
    }

    pub fn convert(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport, AppError> {
        context.check()?;
        let input_hash = ContentHash::of(input);
        let mut losses = Vec::new();
        let mut blocking = Vec::new();
        let mut diagnostics = Vec::new();
        let text = match format {
            InputFormat::Ly => std::str::from_utf8(input)
                .map_err(|_| {
                    AppError::new(ErrorCode::InvalidEncoding, "LilyPond source must be UTF-8")
                })?
                .to_owned(),
            InputFormat::MusicXml | InputFormat::Midi => {
                if format == InputFormat::Midi {
                    ossia_score::parse_midi(input)?;
                }
                let scratch = Scratch::new(&self.scratch_root)?;
                let (tool, name) = if format == InputFormat::MusicXml {
                    ("musicxml2ly", "input.musicxml")
                } else {
                    ("midi2ly", "input.mid")
                };
                fs::write(scratch.path().join(name), input).map_err(io_error)?;
                let output = self.run(tool, &["-o", "converted.ly", name], &scratch, context)?;
                diagnostics =
                    diagnostics::parse(&output.log, scratch.path(), &self.root, None, None);
                for line in output.log.lines() {
                    context.check()?;
                    let lower = line.to_lowercase();
                    if lower.contains("negative skip") {
                        blocking.push(diagnostics::redact(line, scratch.path(), &self.root));
                    } else if [
                        "warning",
                        "error",
                        "not supported",
                        "ignoring",
                        "skipping element",
                        "unknown",
                    ]
                    .iter()
                    .any(|s| lower.contains(s))
                    {
                        losses.push(diagnostics::redact(line, scratch.path(), &self.root));
                    }
                }
                if !output.success {
                    blocking.push("converter failed".into());
                }
                match fs::read_to_string(scratch.path().join("converted.ly")) {
                    Ok(text) => source::normalize_conversion(&text, scratch.path()),
                    Err(_) if !output.success => String::new(),
                    Err(_) => {
                        blocking.push("converter produced no valid UTF-8 source".into());
                        String::new()
                    }
                }
            }
        };
        if !text.is_empty() {
            source::validate(&text, &self.root, &self.version)?;
        } else {
            blocking.push("empty input or converted source".into());
        }
        losses.sort();
        losses.dedup();
        blocking.sort();
        blocking.dedup();
        let status = if !blocking.is_empty() {
            ImportStatus::Refused
        } else if !losses.is_empty() {
            ImportStatus::NeedsConfirmation
        } else {
            ImportStatus::Clean
        };
        let source_hash = (!text.is_empty()).then(|| ContentHash::of(text.as_bytes()));
        let token = if status == ImportStatus::NeedsConfirmation {
            Some(ContentHash::of(
                &serde_json::to_vec(&(&input_hash, self.build_id(), &source_hash, &losses))
                    .unwrap(),
            ))
        } else {
            None
        };
        if let Some(supplied) = confirmation {
            if status == ImportStatus::Refused {
                return Err(AppError::new(
                    ErrorCode::ImportRefused,
                    "blocking conversion errors cannot be confirmed",
                ));
            }
            if token.as_ref() != Some(supplied) {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "import confirmation does not match this input and loss report",
                ));
            }
        }
        let index = ossia_score::build_index(&text);
        Ok(ImportReport {
            status,
            input_hash,
            engine_build_id: self.build_id().into(),
            source: if status == ImportStatus::Refused {
                None
            } else {
                Some(text)
            },
            source_hash,
            confirmed: token.is_some() && confirmation == token.as_ref(),
            confirmation: token,
            measures: index.max_measures,
            voices: index.voices.len(),
            note_events: ossia_score::tokenize(index.source())
                .iter()
                .filter(|t| t.kind == ossia_score::TokenKind::Note)
                .count(),
            losses,
            blocking,
            diagnostics,
        })
    }

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
        let options_hash = ContentHash::of(
            &serde_json::to_vec(
                &serde_json::json!({"pointAndClick":options.point_and_click,"derivationVersion":1}),
            )
            .unwrap(),
        );
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
        source::validate_render_structure(snapshot.source())?;
        let derived = source::derive(snapshot.source(), format, options.point_and_click)?;
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
            Some(snapshot.source()),
        );
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
                    bytes = source::normalize_svg(svg, &derived, snapshot.source())?.into_bytes();
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

fn resource_files(
    root: &Path,
    directory: &Path,
    out: &mut BTreeSet<String>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "resources.json"
            || relative.split('/').any(|p| p == "__pycache__")
            || relative.ends_with(".pyc")
        {
            continue;
        }
        let ty = entry.file_type().map_err(io_error)?;
        if ty.is_dir() {
            resource_files(root, &path, out)?;
        } else if ty.is_file() {
            out.insert(relative);
        } else {
            return Err(failure(
                "integrity",
                "unexpected engine resource symlink or special file",
            ));
        }
    }
    Ok(())
}

fn collect(directory: &Path, format: RenderFormat) -> Result<Vec<(u32, PathBuf)>, AppError> {
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

#[cfg(test)]
mod tests;
