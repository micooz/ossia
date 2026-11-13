use crate::{index::Syntax, SourceMap, SourceSpan, TokenKind};

/// A source expression which can carry an interactive rendering origin.
#[derive(Debug, Clone)]
pub struct SourceElement {
    pub kind: String,
    pub span: SourceSpan,
    pub value: SourceSpan,
}

/// Find header values and context-setting commands without interpreting Scheme.
pub fn source_elements(source: &str) -> Vec<SourceElement> {
    let syntax = Syntax::new(source);
    let tokens = &syntax.tokens;
    let map = SourceMap::new(source);
    let mut result = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if token.command() == Some("header") {
            let Some((open, end)) = syntax.expression(i + 1, tokens.len()) else {
                continue;
            };
            for j in open + 1..end.saturating_sub(2) {
                if syntax.depth[j] != syntax.depth[open] + 1 || tokens[j + 1].raw != "=" {
                    continue;
                }
                let from = j + 2;
                let value_end = (from..end - 1)
                    .find(|&k| {
                        k + 1 < end - 1
                            && syntax.depth[k] == syntax.depth[open] + 1
                            && tokens[k + 1].raw == "="
                    })
                    .unwrap_or(end - 1);
                if value_end <= from || tokens[from].raw == "##f" {
                    continue;
                }
                let span = map
                    .span(tokens[from].byte_start, tokens[value_end - 1].byte_end)
                    .unwrap();
                result.push(SourceElement {
                    kind: format!("header:{}", tokens[j].raw),
                    span,
                    value: span,
                });
            }
        }
        let Some(command) = token.command() else {
            continue;
        };
        if token.mode != crate::Mode::Music {
            continue;
        }
        let args = match command {
            "clef" | "time" => 1,
            "key" => 2,
            _ => continue,
        };
        if i + args < tokens.len() {
            let span = map
                .span(token.byte_start, tokens[i + args].byte_end)
                .unwrap();
            result.push(SourceElement {
                kind: match command {
                    "clef" => "Clef",
                    "time" => "TimeSignature",
                    _ => "KeySignature",
                }
                .into(),
                span,
                value: span,
            });
        }
    }
    result
}

/// Position inside the owning music body at which a default property can be made explicit.
pub fn property_insertion_anchor(
    index: &crate::ScoreIndex,
    staff_index: usize,
) -> Option<SourceSpan> {
    let staff = index.staves.get(staff_index)?;
    if let Some(anchor) = staff.property_anchor {
        return Some(anchor);
    }
    if staff.voices.len() != 1 {
        return None;
    }
    let voice = index.voice(&staff.voices[0])?;
    let syntax = Syntax::new(index.source());
    let open = syntax.tokens.iter().find(|t| {
        t.raw == "{"
            && t.byte_start >= voice.music_span.byte_start
            && t.byte_end <= voice.music_span.byte_end
    })?;
    SourceMap::new(index.source())
        .span(open.byte_end, open.byte_end)
        .ok()
}

fn expression_end(syntax: &Syntax, from: usize, limit: usize) -> usize {
    let Some(t) = syntax.tokens.get(from) else {
        return from;
    };
    if from >= limit {
        return from;
    }
    if t.raw == "{" {
        return syntax.close[from].map_or(from, |c| c + 1);
    }
    if t.kind == TokenKind::Cmd {
        if matches!(t.command(), Some("default" | "segno" | "coda" | "fine")) {
            return from + 1;
        }
        return expression_end(syntax, from + 1, limit);
    }
    from + 1
}

/// Reusable source lookup for all origins in a render.
pub struct ElementSourceIndex<'a> {
    syntax: Syntax,
    elements: Vec<SourceElement>,
    map: SourceMap<'a>,
}

impl<'a> ElementSourceIndex<'a> {
    /// Build one lookup shared by all SVG origins for the source snapshot.
    pub fn new(source: &'a str) -> Self {
        Self {
            syntax: Syntax::new(source),
            elements: source_elements(source),
            map: SourceMap::new(source),
        }
    }

    /// Resolve an origin to a complete expression, or an individual pitch token.
    pub fn span(&self, byte: usize, kind: &str) -> Option<SourceSpan> {
        if let Some(element) = self
            .elements
            .iter()
            .find(|e| e.kind == kind && e.span.byte_start == byte)
        {
            return Some(element.span);
        }
        let syntax = &self.syntax;
        let position = syntax.tokens.partition_point(|t| t.byte_end <= byte);
        if syntax.tokens.get(position)?.byte_start > byte {
            return None;
        }
        let token = &syntax.tokens[position];
        let mut end = position + 1;
        if matches!(kind, "Slur" | "PhrasingSlur") && matches!(token.raw.as_str(), "(" | "\\(") {
            let close = if token.raw == "(" { ")" } else { "\\)" };
            let mut depth = 1;
            for (i, next) in syntax.tokens.iter().enumerate().skip(position + 1) {
                if next.raw == token.raw {
                    depth += 1
                }
                if next.raw == close {
                    depth -= 1
                }
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
        } else if matches!(kind, "Hairpin" | "TextSpanner") {
            for (i, next) in syntax.tokens.iter().enumerate().skip(position + 1) {
                let stop = if kind == "TextSpanner" {
                    next.command() == Some("stopTextSpan")
                } else {
                    matches!(
                        next.command(),
                        Some(
                            "!" | "p"
                                | "pp"
                                | "ppp"
                                | "mp"
                                | "mf"
                                | "f"
                                | "ff"
                                | "fff"
                                | "fp"
                                | "sfz"
                                | "<"
                                | ">"
                        )
                    )
                };
                if stop {
                    end = i + 1;
                    break;
                }
            }
        } else if (kind == "TextScript"
            && (matches!(token.raw.as_str(), "^" | "_" | "-")
                || token.raw.starts_with("-\\markup")))
            || matches!(
                token.command(),
                Some("markup" | "mark" | "textMark" | "sectionLabel")
            )
        {
            end = expression_end(syntax, position + 1, syntax.tokens.len());
        } else if token.command() == Some("tempo") {
            while let Some(next) = syntax.tokens.get(end) {
                if matches!(
                    next.kind,
                    TokenKind::String | TokenKind::Num | TokenKind::Punct
                ) {
                    end += 1
                } else {
                    break;
                }
            }
        }
        if end <= position {
            return None;
        }
        if matches!(kind, "NoteHead" | "Rest" | "MultiMeasureRest")
            && syntax
                .tokens
                .get(end)
                .is_some_and(|t| t.kind == TokenKind::Star)
            && syntax
                .tokens
                .get(end + 1)
                .is_some_and(|t| t.kind == TokenKind::Num)
        {
            end += 2;
        }
        self.map
            .span(token.byte_start, syntax.tokens[end - 1].byte_end)
            .ok()
    }
}

/// Resolve one source origin without retaining a lookup index.
pub fn element_source_span(source: &str, byte: usize, kind: &str) -> Option<SourceSpan> {
    ElementSourceIndex::new(source).span(byte, kind)
}

/// Remap an unchanged source expression only when its correspondence is unambiguous.
pub fn remap_selection_span(old: &str, new: &str, span: SourceSpan) -> Option<SourceSpan> {
    let text = SourceMap::new(old).slice(span).ok()?;
    if text.is_empty() {
        return None;
    }
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut starts = Vec::new();
    if span.byte_end <= prefix {
        starts.push(span.byte_start)
    }
    if span.byte_start >= old.len().saturating_sub(suffix) {
        starts.push(new.len().checked_sub(old.len() - span.byte_start)?);
    }
    starts.sort_unstable();
    starts.dedup();
    if starts.is_empty() {
        let tokens: Vec<_> = crate::tokenize(old)
            .into_iter()
            .filter(|t| !t.trivia())
            .collect();
        let position = tokens
            .iter()
            .position(|t| t.byte_start <= span.byte_start && t.byte_end > span.byte_start)?;
        let last = tokens.iter().rposition(|t| t.byte_start < span.byte_end)?;
        let from = tokens[position.saturating_sub(3)].byte_start;
        let to = tokens[(last + 3).min(tokens.len() - 1)].byte_end;
        let context = &old[from..to];
        if old.match_indices(context).count() != 1 {
            return None;
        }
        starts.extend(
            new.match_indices(context)
                .map(|(start, _)| start + span.byte_start - from),
        );
    }
    if starts.len() != 1 {
        return None;
    }
    SourceMap::new(new)
        .span(starts[0], starts[0] + text.len())
        .ok()
}
