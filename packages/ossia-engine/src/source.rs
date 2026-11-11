use crate::{failure, io_error};
use ossia_score::SourceMap;
use ossia_types::{AppError, ErrorCode, RenderFormat};
use std::{ops::Range, path::Path};

#[cfg(test)]
#[path = "../tests/unit/source.rs"]
mod tests;

#[derive(Debug, Clone)]
struct Lexeme {
    raw: String,
    range: Range<usize>,
}

// This scanner only recognizes structural boundaries; it does not interpret music.
fn scan(source: &str) -> Result<Vec<Lexeme>, AppError> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        match bytes[i] {
            b'%' if bytes.get(i + 1) == Some(&b'{') => {
                i += 2;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    if bytes[i..].starts_with(b"%{") {
                        depth += 1;
                        i += 2;
                    } else if bytes[i..].starts_with(b"%}") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if depth != 0 {
                    return Err(AppError::unsupported("unterminated block comment"));
                }
                continue;
            }
            b'%' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(AppError::unsupported("unterminated source string"));
                }
                i += 1;
            }
            b'#' | b'$' if bytes.get(i + 1) == Some(&b'(') => {
                i += 2;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'"' => {
                            i += 1;
                            while i < bytes.len() && bytes[i] != b'"' {
                                if bytes[i] == b'\\' {
                                    i += 1;
                                }
                                i += 1;
                            }
                            if i >= bytes.len() {
                                return Err(AppError::unsupported("unterminated Scheme string"));
                            }
                            i += 1;
                        }
                        b';' => {
                            while i < bytes.len() && bytes[i] != b'\n' {
                                i += 1;
                            }
                        }
                        b'(' => {
                            depth += 1;
                            i += 1;
                        }
                        b')' => {
                            depth -= 1;
                            i += 1;
                        }
                        // Opaque reader extensions need an interpreter, not guessed brace matching.
                        b'#' if matches!(
                            bytes.get(i + 1),
                            Some(b'|') | Some(b'{') | Some(b'\\')
                        ) =>
                        {
                            return Err(AppError::unsupported(
                                "unsupported Scheme reader in structural scan",
                            ));
                        }
                        _ => i += 1,
                    }
                }
                if depth != 0 {
                    return Err(AppError::unsupported("unterminated Scheme expression"));
                }
            }
            b'#' if bytes.get(i + 1) == Some(&b'{') => {
                return Err(AppError::unsupported(
                    "embedded LilyPond in Scheme is opaque",
                ));
            }
            b'\\' => {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'-') {
                    i += 1;
                }
                if i == start + 1 && i < bytes.len() {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
            }
            b'{' | b'}' => i += 1,
            x if x.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'#' | b'$' if matches!(bytes.get(i + 1), Some(b'\'') | Some(b'`') | Some(b'\\')) => {
                return Err(AppError::unsupported("opaque Scheme reader prefix"));
            }
            _ => {
                i += source[i..].chars().next().unwrap().len_utf8();
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'\\' | b'{' | b'}' | b'%' | b'"' | b'#' | b'$')
                {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
            }
        }
        out.push(Lexeme {
            raw: source[start..i].to_string(),
            range: start..i,
        });
    }
    Ok(out)
}

/// Static single-file validation is a dependency boundary, not a Scheme sandbox.
pub(crate) fn validate(source: &str, engine_root: &Path, version: &str) -> Result<(), AppError> {
    let tokens = match scan(source) {
        Ok(tokens) => tokens,
        // Syntax failures remain repairable; use the lossless lexer for literal dependencies.
        Err(_) => ossia_score::tokenize(source)
            .into_iter()
            .filter(|t| {
                !matches!(
                    t.kind,
                    ossia_score::TokenKind::Ws | ossia_score::TokenKind::Comment
                )
            })
            .map(|t| Lexeme {
                raw: t.raw,
                range: t.byte_start..t.byte_end,
            })
            .collect(),
    };
    for (i, token) in tokens.iter().enumerate() {
        if token.raw == "\\include" {
            let value = tokens
                .get(i + 1)
                .ok_or_else(|| AppError::unsupported("dynamic or incomplete include"))?;
            let Some(name) = value
                .raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
            else {
                return Err(AppError::unsupported("dynamic include is not supported"));
            };
            let standard = engine_root.join("share/lilypond").join(version).join("ly");
            let file = crate::cache::confined(&standard, name)
                .map_err(|_| AppError::unsupported("external include is not supported"))?;
            if !file.is_file() {
                return Err(AppError::unsupported(
                    "include is not an engine standard-library file",
                ));
            }
        }
        if matches!(
            token.raw.as_str(),
            "\\epsfile" | "\\image" | "\\include-string" | "\\verbatim-file"
        ) {
            return Err(AppError::unsupported(
                "external file dependency is not supported",
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct Edit {
    range: Range<usize>,
    replacement: String,
    maps_to_original: bool,
}

#[derive(Debug)]
pub(crate) struct Derived {
    pub text: String,
    // Derived byte intervals mapped to original byte intervals. Inserted text has no source position.
    segments: Vec<(Range<usize>, Option<Range<usize>>)>,
}
impl Derived {
    pub fn original_byte(&self, derived: usize) -> Option<usize> {
        for (dst, src) in &self.segments {
            if dst.contains(&derived) {
                let src = src.as_ref()?;
                return (dst.len() == src.len()).then_some(src.start + derived - dst.start);
            }
        }
        if derived == self.text.len() {
            return self
                .segments
                .last()
                .and_then(|(_, src)| src.as_ref().map(|r| r.end));
        }
        None
    }
}

pub(crate) fn derive(
    source: &str,
    format: RenderFormat,
    anchors: bool,
) -> Result<Derived, AppError> {
    let tokens = scan(source);
    let mut edits = Vec::new();
    if let Ok(tokens) = &tokens {
        {
            let override_command = if anchors {
                "\\pointAndClickOff"
            } else {
                "\\pointAndClickOn"
            };
            for token in tokens.iter().filter(|t| t.raw == override_command) {
                // Whitespace preserves all original scalar and display-column offsets.
                edits.push(Edit {
                    range: token.range.clone(),
                    replacement: " ".repeat(token.range.len()),
                    maps_to_original: true,
                });
            }
        }
    } else if source.contains(if anchors {
        "\\pointAndClickOff"
    } else {
        "\\pointAndClickOn"
    }) {
        return Err(AppError::unsupported(
            "cannot safely override point-and-click in opaque source",
        ));
    }
    if format == RenderFormat::Midi {
        let tokens = tokens?;
        let scores: Vec<_> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.raw == "\\score")
            .collect();
        if scores.len() != 1
            || tokens.iter().any(|t| {
                matches!(
                    t.raw.as_str(),
                    "\\book" | "\\bookpart" | "\\bookOutputName" | "\\bookOutputSuffix"
                )
            })
        {
            return Err(AppError::unsupported(
                "MIDI requires one unambiguous explicit score",
            ));
        }
        let start = scores[0].0 + 1;
        if tokens.get(start).map(|t| t.raw.as_str()) != Some("{") {
            return Err(AppError::unsupported("MIDI requires a literal score block"));
        }
        let mut depth = 0;
        let mut end = None;
        let mut midi = false;
        for (i, token) in tokens.iter().enumerate().skip(start) {
            if depth == 1 && token.raw == "\\midi" {
                if tokens.get(i + 1).map(|t| t.raw.as_str()) != Some("{") {
                    return Err(AppError::unsupported("MIDI block is not literal"));
                }
                midi = true;
            }
            if token.raw == "{" {
                depth += 1;
            }
            if token.raw == "}" {
                depth -= 1;
                if depth == 0 {
                    end = Some(token.range.start);
                    break;
                }
            }
        }
        let end = end.ok_or_else(|| AppError::unsupported("unbalanced score block"))?;
        if !midi {
            edits.push(Edit {
                range: end..end,
                replacement: "\n\\midi {}\n".into(),
                maps_to_original: false,
            });
        }
    }
    edits.sort_by_key(|e| e.range.start);
    let mut text = String::new();
    let mut segments = Vec::new();
    let mut previous = 0;
    for edit in edits {
        if edit.range.start < previous {
            return Err(failure("source-map", "overlapping source derivations"));
        }
        let begin = text.len();
        text.push_str(&source[previous..edit.range.start]);
        segments.push((begin..text.len(), Some(previous..edit.range.start)));
        let begin = text.len();
        text.push_str(&edit.replacement);
        segments.push((
            begin..text.len(),
            edit.maps_to_original.then_some(edit.range.clone()),
        ));
        previous = edit.range.end;
    }
    let begin = text.len();
    text.push_str(&source[previous..]);
    segments.push((begin..text.len(), Some(previous..source.len())));
    Ok(Derived { text, segments })
}

pub(crate) fn validate_render_structure(source: &str) -> Result<(), AppError> {
    if let Ok(tokens) = scan(source) {
        if tokens.iter().filter(|t| t.raw == "\\score").count() > 1
            || tokens.iter().filter(|t| t.raw == "\\book").count() > 1
            || tokens.iter().any(|t| t.raw == "\\bookpart")
        {
            return Err(AppError::unsupported(
                "multiple score/book outputs are not uniquely attributable",
            ));
        }
    }
    Ok(())
}

pub(crate) fn normalize_conversion(text: &str, scratch: &Path) -> String {
    // Converter input is always relative; remove absolute scratch prefixes if a tool still emits one.
    text.replace(&scratch.to_string_lossy().to_string(), "<input>")
        .replace(&scratch.to_string_lossy().replace('\\', "/"), "<input>")
}

pub(crate) fn normalize_svg(
    svg: &str,
    derived: &Derived,
    original: &str,
) -> Result<String, AppError> {
    use quick_xml::{
        events::{BytesStart, Event},
        Reader, Writer,
    };
    let mut reader = Reader::from_str(svg);
    let mut writer = Writer::new(Vec::new());
    let links = regex::Regex::new(r"^textedit://(.+):([0-9]+):([0-9]+):([0-9]+)$").unwrap();
    let derived_map = SourceMap::new(&derived.text);
    let original_map = SourceMap::new(original);
    loop {
        let event = reader
            .read_event()
            .map_err(|_| AppError::invalid("engine produced malformed SVG"))?;
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = svg
                    .as_bytes()
                    .get(reader.buffer_position().saturating_sub(2) as usize)
                    == Some(&b'/');
                let name = e.name().as_ref().to_owned();
                let mut node = BytesStart::new(name);
                let mut attrs = Vec::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|_| AppError::invalid("malformed SVG attribute"))?;
                    let key = attr.key.as_ref().to_owned();
                    let mut value = attr
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| AppError::invalid("malformed SVG attribute"))?
                        .into_owned();
                    if matches!(key.as_str(), "href" | "xlink:href")
                        && value.starts_with("textedit://")
                    {
                        let mut normalized = None;
                        if let Some(c) = links.captures(&value) {
                            let file = c[1].replace('\\', "/");
                            if file.rsplit('/').next() == Some("score.ly") {
                                if let (Ok(line), Ok(char_offset)) = (c[2].parse(), c[3].parse()) {
                                    if let Some(byte) = derived_map
                                        .anchor_byte(line, char_offset)
                                        .and_then(|b| derived.original_byte(b))
                                    {
                                        if let Ok(p) = original_map.position(byte) {
                                            let line_start =
                                                original[..byte].rfind('\n').map_or(0, |b| b + 1);
                                            let display = original[line_start..byte].chars().fold(
                                                1,
                                                |n, c| {
                                                    if c == '\t' {
                                                        n + 8 - (n - 1) % 8
                                                    } else {
                                                        n + 1
                                                    }
                                                },
                                            );
                                            normalized = Some(format!(
                                                "textedit://score.ly:{}:{}:{}",
                                                p.line,
                                                p.column - 1,
                                                display
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        // Standard-library and inserted-text anchors must not point into user source.
                        let Some(link) = normalized else {
                            continue;
                        };
                        value = link;
                    }
                    attrs.push((key, value));
                }
                for (key, value) in &attrs {
                    node.push_attribute((key.as_str(), value.as_str()));
                }
                writer
                    .write_event(if empty {
                        Event::Empty(node)
                    } else {
                        Event::Start(node)
                    })
                    .map_err(io_error)?;
            }
            Event::Eof => break,
            other => writer.write_event(other).map_err(io_error)?,
        }
    }
    String::from_utf8(writer.into_inner())
        .map_err(|_| AppError::new(ErrorCode::InvalidEncoding, "SVG is not UTF-8"))
}
