use crate::source::Derived;
use ossia_score::SourceMap;
use ossia_types::{Diagnostic, EnginePosition, Severity};
use std::path::Path;

pub(crate) fn redact(text: &str, scratch: &Path, engine: &Path) -> String {
    let mut text = text.to_owned();
    for (path, replacement) in [(scratch, "<scratch>"), (engine, "<engine>")] {
        let raw = path.to_string_lossy();
        text = text
            .replace(raw.as_ref(), replacement)
            .replace(&raw.replace('\\', "/"), replacement);
    }
    let paths = regex::Regex::new(
        r#"(?m)(^|[\s="'(:])((?:[A-Za-z]:[\\/]|/)(?:[^\s:<>"'()]+[\\/])*[^\s:<>"'()]*)"#,
    )
    .unwrap();
    paths.replace_all(&text, "${1}<path>").into_owned()
}

pub(crate) fn parse(
    log: &str,
    scratch: &Path,
    engine: &Path,
    derived: Option<&Derived>,
    original: Option<&str>,
) -> Vec<Diagnostic> {
    let position = regex::Regex::new(r"^(.*?):([0-9]+):([0-9]+):\s*(.*)$").unwrap();
    log.lines()
        .filter(|l| !l.trim().is_empty())
        .take(200)
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let severity = if lower.contains("error") || lower.contains("fatal") {
                Severity::Error
            } else if lower.contains("warning") || lower.contains("negative skip") {
                Severity::Warning
            } else {
                Severity::Info
            };
            let mut raw_position = None;
            let mut normalized = None;
            if let Some(c) = position.captures(line) {
                if let (Ok(number), Ok(column)) = (c[2].parse::<usize>(), c[3].parse::<usize>()) {
                    let file = c[1].replace('\\', "/");
                    let input = file == "score.ly" || Path::new(&file) == scratch.join("score.ly");
                    raw_position = Some(EnginePosition {
                        file: if input {
                            "score.ly".into()
                        } else {
                            redact(&c[1], scratch, engine)
                        },
                        line: number,
                        column,
                        char_offset: None,
                    });
                    if input {
                        if let (Some(derived), Some(original)) = (derived, original) {
                            normalized = SourceMap::new(&derived.text)
                                .display_byte(number, column)
                                .and_then(|b| derived.original_byte(b))
                                .and_then(|b| SourceMap::new(original).position(b).ok());
                        }
                    }
                }
            }
            Diagnostic {
                severity,
                message: redact(line, scratch, engine),
                raw_position,
                position: normalized,
            }
        })
        .collect()
}
