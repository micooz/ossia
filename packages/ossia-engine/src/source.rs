use crate::{failure, io_error};
use ossia_score::SourceMap;
use ossia_types::{AppError, ErrorCode, InputFormat, RenderFormat};
use std::{ops::Range, path::Path};

#[cfg(test)]
#[path = "../tests/unit/source.rs"]
mod tests;

#[derive(Debug, Clone)]
struct Lexeme {
    raw: String,
    range: Range<usize>,
}

mod interaction;
mod scanner;
mod scores;
mod version;
use scanner::scan;
pub(crate) use version::check_migration_input;
pub(crate) use version::check_version;

/// Static single-file validation is a dependency boundary, not a Scheme sandbox.
pub(crate) fn validate(source: &str, engine_root: &Path, version: &str) -> Result<(), AppError> {
    let tokens = match scan(source) {
        Ok(tokens) => tokens,
        Err(error @ scanner::ScanError::RestrictedCommand(_)) => return Err(error.into_app_error()),
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

pub(crate) fn strip_musicxml_source_paths(input: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(input) else {
        return input.to_vec();
    };
    static SOURCE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SOURCE_FILE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let source = SOURCE.get_or_init(|| {
        regex::Regex::new(r"(?s)<source>.*?</source>\s*").expect("valid source element regex")
    });
    let source_file = SOURCE_FILE.get_or_init(|| {
        regex::Regex::new(
            r#"(?s)<miscellaneous-field\b[^>]*\bname\s*=\s*"source-file"[^>]*>.*?</miscellaneous-field>\s*"#,
        )
        .expect("valid source-file field regex")
    });
    let without_source = source.replace_all(text, "");
    source_file
        .replace_all(&without_source, "")
        .into_owned()
        .into_bytes()
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

/// Preserve direct source coordinates for compiler passes using an include wrapper.
pub(crate) fn unchanged(source: &str) -> Derived {
    Derived {
        text: source.into(),
        segments: vec![(0..source.len(), Some(0..source.len()))],
    }
}

pub(crate) fn derive(
    source: &str,
    format: RenderFormat,
    anchors: bool,
) -> Result<Derived, AppError> {
    let tokens = scan(source);
    let mut edits = Vec::new();
    let omitted_score = match &tokens {
        Ok(tokens) => scores::omitted_score(tokens, format),
        Err(_) => None,
    };
    let preamble = source.len() - source.trim_start_matches('\u{feff}').len();
    if format == RenderFormat::Svg && anchors {
        edits.extend(interaction::edits(source, preamble));
    }
    if format != RenderFormat::Midi && ossia_score::is_jianpu_source(source) {
        let tokens = tokens.as_ref().map_err(|_| {
            AppError::unsupported("Jianpu requires source with an inspectable LilyPond structure")
        })?;
        let prohibited = tokens.windows(2).find_map(|pair| {
            if omitted_score
                .as_ref()
                .is_some_and(|range| range.contains(&pair[0].range.start))
            {
                return None;
            }
            let feature = match (pair[0].raw.as_str(), pair[1].raw.as_str()) {
                ("\\new" | "\\context", "TabStaff") => "the TabStaff context",
                ("\\new" | "\\context", "DrumStaff") => "the DrumStaff context",
                _ => return None,
            };
            Some((pair[0].range.start, feature))
        });
        if let Some((byte, feature)) = prohibited {
            return Err(AppError {
                code: ErrorCode::UnsupportedSource,
                message: format!("Jianpu layout does not support {feature}"),
                detail: SourceMap::new(source)
                    .position(byte)
                    .ok()
                    .map(|position| serde_json::json!({"position": position})),
            });
        }
        edits.push(Edit {
            range: preamble..preamble,
            replacement: format!(
                "#(define ossia-jianpu-svg {})\n{}\n",
                if format == RenderFormat::Svg && anchors {
                    "#t"
                } else {
                    "#f"
                },
                include_str!("jianpu.ily")
            ),
            maps_to_original: false,
        });
    }
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
    if matches!(format, RenderFormat::Svg | RenderFormat::Pdf)
        && !tokens
            .as_ref()
            .is_ok_and(|tokens| tokens.iter().any(|token| token.raw == "tagline"))
    {
        edits.push(Edit {
            range: source.len()..source.len(),
            replacement: "\n\\paper { tagline = ##f }\n".into(),
            maps_to_original: false,
        });
    }
    if format == RenderFormat::Midi {
        let tokens = tokens.map_err(scanner::ScanError::into_app_error)?;
        let tokens: Vec<_> = tokens
            .into_iter()
            .filter(|token| {
                !omitted_score
                    .as_ref()
                    .is_some_and(|range| range.contains(&token.range.start))
            })
            .collect();
        edits.extend(chord_name_edits(&tokens));
        let scores: Vec<_> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.raw == "\\score")
            .collect();
        if tokens.iter().any(|t| {
            matches!(
                t.raw.as_str(),
                "\\book" | "\\bookpart" | "\\bookOutputName" | "\\bookOutputSuffix"
            )
        }) {
            return Err(AppError::unsupported(
                "playback does not support LilyPond \\book or \\bookpart structures",
            ));
        }
        if scores.len() != 1 {
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
    if let Some(range) = omitted_score {
        edits.retain(|edit| !(edit.range.start >= range.start && edit.range.end <= range.end));
        edits.push(Edit {
            range,
            replacement: String::new(),
            maps_to_original: false,
        });
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

fn chord_name_edits(tokens: &[Lexeme]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].raw != "ChordNames"
            || i == 0
            || !matches!(tokens[i - 1].raw.as_str(), "\\context" | "\\new")
        {
            i += 1;
            continue;
        }
        let start = i - 1;
        let mut open = i + 1;
        if tokens.get(open).map(|token| token.raw.as_str()) == Some("=") {
            open += 1;
            if tokens
                .get(open)
                .is_none_or(|token| !token.raw.starts_with('"'))
            {
                i += 1;
                continue;
            }
            open += 1;
        }
        if tokens.get(open).map(|token| token.raw.as_str()) == Some("\\with") {
            open += 1;
            let Some(close) = matching_brace(tokens, open) else {
                i += 1;
                continue;
            };
            open = close + 1;
        }
        let close = if tokens.get(open).map(|token| token.raw.as_str()) == Some("{") {
            let Some(close) = matching_brace(tokens, open) else {
                i += 1;
                continue;
            };
            close
        } else if let Some(name) = tokens
            .get(open)
            .and_then(|token| token.raw.strip_prefix('\\'))
        {
            if !tokens.windows(3).any(|assignment| {
                assignment[0].raw == name
                    && assignment[1].raw == "="
                    && matches!(assignment[2].raw.as_str(), "{" | "<<" | "\\chordmode")
            }) {
                i += 1;
                continue;
            }
            open
        } else {
            i += 1;
            continue;
        };
        edits.push(Edit {
            range: tokens[start].range.start..tokens[close].range.end,
            replacement: String::new(),
            maps_to_original: false,
        });
        i = close + 1;
    }
    edits
}

fn matching_brace(tokens: &[Lexeme], open: usize) -> Option<usize> {
    if tokens.get(open).map(|token| token.raw.as_str()) != Some("{") {
        return None;
    }
    let mut depth = 0;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.raw.as_str() {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
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

pub(crate) fn normalize_conversion(
    text: &str,
    scratch: &Path,
    format: InputFormat,
) -> Result<String, AppError> {
    // Converter input is always relative; remove absolute scratch prefixes if a tool still emits one.
    let text = text
        .replace(&scratch.to_string_lossy().to_string(), "<input>")
        .replace(&scratch.to_string_lossy().replace('\\', "/"), "<input>");
    if format != InputFormat::MusicXml {
        return Ok(text);
    }
    // musicxml2ly --midi creates a second score; activate its single-score template instead.
    let midi = regex::Regex::new(
        r"(?m)^([ \t]*)% To create MIDI output, uncomment the following line:\r?\n[ \t]*% (\\midi \{ \\tempo 4 = [0-9]+(?:\.[0-9]+)? \})[ \t]*\r?$",
    ).unwrap();
    if midi.captures_iter(&text).count() != 1 {
        return Err(AppError::unsupported(
            "unexpected musicxml2ly MIDI template",
        ));
    }
    Ok(midi.replace(&text, "$1$2").into_owned())
}

pub(crate) fn normalize_svg(
    svg: &str,
    derived: &Derived,
    original: &str,
) -> Result<String, AppError> {
    let input = normalize_svg_colors(svg)?;
    use quick_xml::{
        events::{BytesStart, Event},
        Reader, Writer,
    };
    let mut reader = Reader::from_str(&input);
    let mut writer = Writer::new(Vec::new());
    let links = regex::Regex::new(r"^textedit://(.+):([0-9]+):([0-9]+):([0-9]+)$").unwrap();
    let derived_map = SourceMap::new(&derived.text);
    let original_map = SourceMap::new(original);
    let source_index = ossia_score::ElementSourceIndex::new(original);
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(error) => {
                return Err(AppError {
                    code: ErrorCode::InvalidInput,
                    message: "engine produced malformed SVG".into(),
                    detail: Some(serde_json::json!({
                        "error": error.to_string(),
                        "position": reader.buffer_position(),
                    })),
                });
            }
        };
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = input
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
                        && (value.starts_with("ossia:") || value.starts_with("ossia-hit:"))
                    {
                        attrs.extend(
                            interaction::attributes(
                                &value,
                                derived,
                                original,
                                &derived_map,
                                &source_index,
                            )
                            .ok_or_else(|| AppError::invalid("invalid interactive SVG origin"))?,
                        );
                        continue;
                    }
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

/// Rewrite CSS color functions that SVG rasterizers reject, such as LilyPond's
/// percentage alpha (`rgba(100%, 100%, 100%, 100.0000%)`).
pub(crate) fn normalize_svg_colors(svg: &str) -> Result<String, AppError> {
    use quick_xml::{events::Event, Reader, Writer};
    let mut reader = Reader::from_str(svg);
    let mut writer = Writer::new(Vec::new());
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(error) => {
                return Err(AppError {
                    code: ErrorCode::InvalidInput,
                    message: "engine produced malformed SVG".into(),
                    detail: Some(serde_json::json!({
                        "error": error.to_string(),
                        "position": reader.buffer_position(),
                    })),
                });
            }
        };
        match event {
            Event::Start(element) => write_color_element(&mut writer, &element, false)?,
            Event::Empty(element) => write_color_element(&mut writer, &element, true)?,
            Event::Eof => break,
            other => writer.write_event(other).map_err(io_error)?,
        }
    }
    String::from_utf8(writer.into_inner())
        .map_err(|_| AppError::new(ErrorCode::InvalidEncoding, "SVG is not UTF-8"))
}

fn write_color_element(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    element: &quick_xml::events::BytesStart<'_>,
    empty: bool,
) -> Result<(), AppError> {
    use quick_xml::events::{BytesStart, Event};
    let mut node = BytesStart::new(element.name().as_ref().to_owned());
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| AppError::invalid("malformed SVG attribute"))?;
        let key = attribute.key.as_ref().to_owned();
        let mut value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|_| AppError::invalid("malformed SVG attribute"))?
            .into_owned();
        if matches!(
            key.as_str(),
            "color" | "fill" | "stroke" | "stop-color" | "flood-color" | "lighting-color"
        ) {
            if let Some(normalized) = normalize_color_value(&value) {
                value = normalized;
            }
        }
        node.push_attribute((key.as_str(), value.as_str()));
    }
    writer
        .write_event(if empty {
            Event::Empty(node)
        } else {
            Event::Start(node)
        })
        .map_err(io_error)
}

/// Normalize a percentage alpha channel to the numeric form understood by SVG
/// rasterizers. Returns `None` for values that already parse.
fn normalize_color_value(value: &str) -> Option<String> {
    let open = value.find('(')?;
    let function = value[..open].trim();
    if !function.eq_ignore_ascii_case("rgb") && !function.eq_ignore_ascii_case("rgba") {
        return None;
    }
    let inner = value[open + 1..].trim().strip_suffix(')')?.trim();
    let separated = inner.replace('/', " ");
    let parts = separated
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    let percent = parts[3].strip_suffix('%')?.parse::<f64>().ok()?;
    if !percent.is_finite() {
        return None;
    }
    let alpha = (percent / 100.0).clamp(0.0, 1.0);
    Some(format!(
        "rgba({}, {}, {}, {})",
        parts[0],
        parts[1],
        parts[2],
        format_alpha(alpha)
    ))
}

fn format_alpha(alpha: f64) -> String {
    format!("{alpha:.4}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
