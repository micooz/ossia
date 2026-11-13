use super::Edit;
use ossia_score::{source_elements, tokenize, TokenKind};

pub(super) fn edits(source: &str) -> Vec<Edit> {
    let mut edits = vec![Edit {
        range: 0..0,
        replacement: format!("{}\n", include_str!("../interaction.ily")),
        maps_to_original: false,
    }];
    for element in source_elements(source) {
        if let Some(field) = element.kind.strip_prefix("header:") {
            let field = field.trim_matches('"');
            // Only fields rendered by the titling templates are graphic elements.
            if !matches!(
                field,
                "dedication"
                    | "title"
                    | "subtitle"
                    | "subsubtitle"
                    | "composer"
                    | "poet"
                    | "arranger"
                    | "translator"
                    | "instrument"
                    | "meter"
                    | "piece"
                    | "opus"
                    | "copyright"
                    | "tagline"
            ) {
                continue;
            }
            let kind = format!("header-{field}");
            let value = &source[element.value.byte_start..element.value.byte_end];
            let tokens = tokenize(value);
            let Some(first) = tokens
                .iter()
                .find(|t| !matches!(t.kind, TokenKind::Ws | TokenKind::Comment))
            else {
                continue;
            };
            if first.kind == TokenKind::Scheme {
                let at = element.value.byte_start + first.byte_start;
                edits.push(Edit {
                    range: at..at + 1,
                    replacement: format!(
                        "#(ossia-header-value \"{}\" \"{}:{}\" ",
                        kind, element.span.byte_start, element.span.byte_end
                    ),
                    maps_to_original: false,
                });
                edits.push(Edit {
                    range: element.value.byte_end..element.value.byte_end,
                    replacement: ")".into(),
                    maps_to_original: false,
                });
                continue;
            }
            if first.kind == TokenKind::Cmd && first.raw != "\\markup" {
                edits.push(Edit {
                    range: element.value.byte_start..element.value.byte_end,
                    replacement: format!(
                        "#(ossia-header-value \"{}\" \"{}:{}\" (ly:parser-lookup '{}))",
                        kind,
                        element.span.byte_start,
                        element.span.byte_end,
                        &first.raw[1..]
                    ),
                    maps_to_original: false,
                });
                continue;
            }
            if first.kind != TokenKind::String && first.raw != "\\markup" {
                continue;
            }
            let (at, prefix) = if first.raw == "\\markup" {
                (element.value.byte_start + first.byte_end, "")
            } else {
                (element.value.byte_start, "\\markup ")
            };
            edits.push(Edit {
                range: at..at,
                replacement: format!(
                    "{prefix} \\ossiaHeader \"{}\" \"{}:{}\" ",
                    kind, element.span.byte_start, element.span.byte_end
                ),
                maps_to_original: false,
            });
        } else {
            edits.push(Edit {
                range: element.span.byte_start..element.span.byte_start,
                replacement: format!(
                    "\\applyContext #(ossia-set-origin '{} \"{}:{}\") ",
                    element.kind, element.span.byte_start, element.span.byte_end
                ),
                maps_to_original: false,
            });
        }
    }
    edits
}

pub(super) fn attributes(
    value: &str,
    derived: &super::Derived,
    original: &str,
    derived_map: &ossia_score::SourceMap<'_>,
    source_index: &ossia_score::ElementSourceIndex<'_>,
) -> Option<Vec<(String, String)>> {
    let (value, hit) = if let Some(value) = value.strip_prefix("ossia-hit:") {
        (value, true)
    } else {
        (value.strip_prefix("ossia:")?, false)
    };
    let fields: Vec<_> = value.split(':').collect();
    if fields.len() != 4 {
        return None;
    }
    let mut attrs = vec![
        ("data-ossia-kind".into(), fields[0].into()),
        ("data-ossia-staff".into(), fields[1].into()),
    ];
    if hit {
        attrs.push(("data-ossia-hit".into(), "true".into()));
    }
    let span = if let Some(start) = fields[2].strip_prefix('s') {
        let start = start.parse::<usize>().ok()?;
        let end = fields[3].parse::<usize>().ok()?;
        ossia_score::SourceMap::new(original).span(start, end).ok()
    } else if let (Ok(line), Ok(char_offset)) = (fields[2].parse(), fields[3].parse()) {
        derived_map
            .anchor_byte(line, char_offset)
            .and_then(|b| derived.original_byte(b))
            .and_then(|b| source_index.span(b, fields[0]))
    } else {
        None
    };
    if let Some(span) = span {
        attrs.push((
            "data-ossia-source".into(),
            format!("{}:{}", span.byte_start, span.byte_end),
        ));
    }
    Some(attrs)
}
