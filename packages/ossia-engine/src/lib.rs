#![doc = include_str!("../README.md")]
mod audio;
pub use audio::render_wav;
mod cache;
mod compile;
pub use compile::render_options_hash;
mod diagnostics;
mod musicxml;
mod mxl;
mod pdf;
mod process;
mod raster;
mod resources;
mod source;
mod verify;

pub use ossia_types::*;
pub use pdf::{overlay_pdf_drawing, protect_pdf, select_pdf_pages};
use process::{executable_path, run, tool_path, Scratch};
pub use process::{Cancellation, RunContext};
use resources::{resource_files, Inventory, Target};
use serde::Serialize;
use std::{
    collections::BTreeSet,
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

#[derive(Clone, Debug)]
pub struct Engine {
    root: PathBuf,
    scratch_root: PathBuf,
    version: String,
    target: Target,
    audio_root: Option<PathBuf>,
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

/// Options for MusicXML conversion.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConvertOptions {
    /// Let musicxml2ly discard page layout and break information.
    pub ignore_page_layout: bool,
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
        let lock = resources::load_lock()?;
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
        let paths = verify::resources(&root, &inventory.files, context)?;
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
        for name in ["lilypond", "musicxml2ly", "midi2ly", "convert-ly"] {
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
        let scratch_root = executable_path(&scratch_root.canonicalize().map_err(io_error)?);
        let engine = Self {
            root,
            scratch_root,
            version: lock.version,
            target,
            audio_root: None,
        };
        verify::probes(&engine, context)?;
        Ok(engine)
    }
    /// Configure the shared audio resource directory used for WAV export.
    pub fn with_audio_root(mut self, root: PathBuf) -> Self {
        self.audio_root = Some(root);
        self
    }

    /// Render audio using the configured shared resource directory.
    pub fn render_wav(
        &self,
        playback: &ScorePlayback,
        context: &RunContext,
    ) -> Result<Vec<u8>, AppError> {
        let root = self
            .audio_root
            .as_deref()
            .ok_or_else(|| AppError::invalid("audio resource directory is not configured"))?;
        audio::render_wav(playback, root, context)
    }
    /// `resources` is resolved by the executable/bundle owner, never from the process cwd.
    pub fn from_resources(
        resources: &Path,
        scratch_root: &Path,
        context: &RunContext,
    ) -> Result<Self, AppError> {
        Ok(
            Self::verify(&resources.join("engine"), scratch_root, context)?
                .with_audio_root(resources.join("ui/audio")),
        )
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
            .map(|arg| {
                cache::confined(&self.root, arg)
                    .map(|p| executable_path(&p).to_string_lossy().into_owned())
            })
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
        self.convert_with_options(
            input,
            format,
            confirmation,
            context,
            ConvertOptions::default(),
        )
    }

    /// Migrate a LilyPond input in scratch space and return the candidate without publishing it.
    pub fn migrate_lilypond(
        &self,
        input: &[u8],
        context: &RunContext,
    ) -> Result<ImportReport, AppError> {
        let source = std::str::from_utf8(input).map_err(|_| {
            AppError::new(ErrorCode::InvalidEncoding, "LilyPond source must be UTF-8")
        })?;
        source::check_migration_input(source, &self.version).map_err(AppError::invalid)?;
        let scratch = Scratch::new(&self.scratch_root)?;
        fs::write(scratch.path().join("input.ly"), input).map_err(io_error)?;
        let output = self.run(
            "convert-ly",
            &["--edit", "--current-version", "input.ly"],
            &scratch,
            context,
        )?;
        let converted = fs::read(scratch.path().join("input.ly")).map_err(io_error)?;
        let mut report = self.convert(&converted, InputFormat::Ly, None, context)?;
        report.input_hash = ContentHash::of(input);
        report.diagnostics.extend(diagnostics::parse(
            &output.log,
            scratch.path(),
            &self.root,
            None,
            None,
        ));
        if !output.success {
            report.status = ImportStatus::Refused;
            report
                .blocking
                .push("convert-ly failed; inspect the conversion diagnostics".into());
            report.confirmation = None;
            report.confirmed = false;
        } else {
            for line in output.log.lines() {
                let lower = line.to_lowercase();
                if lower.contains("warning")
                    || lower.contains("not smart enough")
                    || lower.contains("error")
                {
                    report
                        .losses
                        .push(diagnostics::redact(line, scratch.path(), &self.root));
                }
            }
            if !report.losses.is_empty() && report.status != ImportStatus::Refused {
                report.status = ImportStatus::NeedsConfirmation;
                report.confirmation = Some(ContentHash::of(
                    &serde_json::to_vec(&(
                        &report.input_hash,
                        self.build_id(),
                        &report.source_hash,
                        &report.losses,
                    ))
                    .map_err(|error| AppError::invalid(error.to_string()))?,
                ));
            }
        }
        Ok(report)
    }

    /// Convert with options that do not affect LilyPond or MIDI inputs.
    pub fn convert_with_options(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
        options: ConvertOptions,
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
            InputFormat::MusicXml | InputFormat::Mxl | InputFormat::Midi => {
                if format == InputFormat::Midi {
                    ossia_score::parse_midi(input)?;
                }
                let extracted = if format == InputFormat::Mxl {
                    Some(mxl::extract(input)?)
                } else {
                    None
                };
                let input =
                    source::strip_musicxml_source_paths(extracted.as_deref().unwrap_or(input));
                let scratch = Scratch::new(&self.scratch_root)?;
                let (tool, name, conversion_format) = match format {
                    InputFormat::MusicXml | InputFormat::Mxl => {
                        ("musicxml2ly", "input.musicxml", InputFormat::MusicXml)
                    }
                    _ => ("midi2ly", "input.mid", InputFormat::Midi),
                };
                fs::write(scratch.path().join(name), &input).map_err(io_error)?;
                let mut args = vec!["-o", "converted.ly"];
                if options.ignore_page_layout
                    && matches!(format, InputFormat::MusicXml | InputFormat::Mxl)
                {
                    args.push("--npl");
                }
                args.push(name);
                let output = self.run(tool, &args, &scratch, context)?;
                diagnostics =
                    diagnostics::parse(&output.log, scratch.path(), &self.root, None, None);
                for line in output.log.lines() {
                    context.check()?;
                    let lower = line.to_lowercase();
                    if [
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
                    Ok(text) => {
                        source::normalize_conversion(&text, scratch.path(), conversion_format)?
                    }
                    Err(_) if !output.success => String::new(),
                    Err(_) => {
                        blocking.push("converter produced no valid UTF-8 source".into());
                        String::new()
                    }
                }
            }
            InputFormat::Pdf | InputFormat::Png | InputFormat::Jpeg | InputFormat::Tiff => {
                return Err(AppError::unsupported(
                    "PDF and image imports require the remote OMR engine backend",
                ));
            }
        };
        let mut lilypond_version = None;
        if format == InputFormat::Ly && !text.is_empty() {
            match source::check_version(&text, &self.version) {
                Ok(version) => lilypond_version = Some(version),
                Err(reason) => blocking.push(reason),
            }
        }
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
        let (measures, measure_count_reason) = match index.measure_count() {
            Ok(count) => (Some(count), None),
            Err(reason) => (None, Some(reason)),
        };
        Ok(ImportReport {
            status,
            input_hash,
            engine_build_id: self.build_id().into(),
            omr_source: None,
            source: (!text.is_empty()).then_some(text),
            source_hash,
            lilypond_version,
            playback_error: None,
            confirmed: token.is_some() && confirmation == token.as_ref(),
            confirmation: token,
            measures,
            measure_count_reason,
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
}
