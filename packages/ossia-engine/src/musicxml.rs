use super::*;
use ossia_score::SourceMap;

mod events;
mod measures;
mod note_xml;
mod notes;
mod score;
mod tablature;
mod write;

type Result<T> = std::result::Result<T, AppError>;

const MAX_NOTATION_BYTES: u64 = 32 * 1024 * 1024;

impl Engine {
    /// Export one engravable score snapshot as uncompressed MusicXML 4.0.
    pub fn export_musicxml(
        &self,
        snapshot: &SourceSnapshot,
        context: &RunContext,
    ) -> Result<Vec<u8>> {
        context.check()?;
        let original = snapshot.source();
        source::validate(original, &self.root, &self.version)?;
        let derived = source::unchanged(original);
        let scratch = Scratch::new(&self.scratch_root)?;
        fs::write(scratch.path().join("score.ly"), &derived.text).map_err(io_error)?;
        fs::write(
            scratch.path().join("extract.ily"),
            include_str!("musicxml/extract.ily"),
        )
        .map_err(io_error)?;
        fs::write(
            scratch.path().join("tablature.ily"),
            include_str!("musicxml/tablature.ily"),
        )
        .map_err(io_error)?;
        fs::write(
            scratch.path().join("export.ly"),
            "\\version \"2.26.0\"\n\\include \"extract.ily\"\n\\include \"score.ly\"\n",
        )
        .map_err(io_error)?;
        let output = self.run(
            "lilypond",
            &[
                "-dno-print-pages",
                "-dpoint-and-click=#f",
                "-o",
                "score",
                "export.ly",
            ],
            &scratch,
            context,
        )?;
        if !output.success {
            let diagnostics = diagnostics::parse(
                &output.log,
                scratch.path(),
                &self.root,
                Some(&derived),
                Some(original),
            );
            return Err(AppError {
                code: ErrorCode::CompileFailed,
                message: "LilyPond could not extract MusicXML notation".into(),
                detail: Some(serde_json::json!({"diagnostics": diagnostics})),
            });
        }

        let path = scratch.path().join("notation.jsonl");
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if !metadata.is_file() || metadata.len() > MAX_NOTATION_BYTES {
            return Err(failure(
                "output-limit",
                "invalid or oversized MusicXML event stream",
            ));
        }
        let data = fs::read_to_string(path).map_err(io_error)?;
        let notation = events::Notation::read(&data, original, context)?;
        let score = score::Score::build(notation, original, context)?;
        let xml = write::encode(&score, context)?;
        context.check()?;
        if xml.len() as u64 > MAX_NOTATION_BYTES {
            return Err(failure("output-limit", "MusicXML output exceeds 32 MiB"));
        }
        Ok(xml)
    }
}

fn unsupported(source: &str, location: &[usize], feature: impl std::fmt::Display) -> AppError {
    source_error(
        source,
        location,
        format!("MusicXML export does not support {feature}"),
    )
}

fn source_error(source: &str, location: &[usize], message: String) -> AppError {
    let position = if let [line, column] = location {
        let map = SourceMap::new(source);
        map.display_byte(*line, column.saturating_add(1))
            .and_then(|byte| map.position(byte).ok())
    } else {
        None
    };
    AppError {
        code: ErrorCode::UnsupportedSource,
        message,
        detail: position.map(|position| serde_json::json!({"position": position})),
    }
}

fn invalid_events() -> AppError {
    failure(
        "musicxml-events",
        "invalid or incomplete LilyPond notation events",
    )
}
