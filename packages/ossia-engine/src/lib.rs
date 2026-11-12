#![doc = include_str!("../README.md")]
mod cache;
mod compile;
mod diagnostics;
mod pdf;
mod process;
mod raster;
mod resources;
mod source;

pub use ossia_types::*;
pub use pdf::protect_pdf;
use process::{run, tool_path, Scratch};
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
        let (measures, measure_count_reason) = match index.measure_count() {
            Ok(count) => (Some(count), None),
            Err(reason) => (None, Some(reason)),
        };
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
