use crate::{tokenize, Mode, SourceMap, Token, TokenKind};
use ossia_types::LocationCapability;
use std::collections::HashSet;

use super::VoiceRegion;

pub(crate) struct Syntax {
    pub(crate) tokens: Vec<Token>,
    pub(crate) close: Vec<Option<usize>>,
    pub(crate) depth: Vec<usize>,
    pub(crate) malformed: bool,
}

impl Syntax {
    pub(crate) fn new(source: &str) -> Self {
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter(|t| !t.trivia())
            .collect();
        let mut close = vec![None; tokens.len()];
        let mut depth = vec![0; tokens.len()];
        let mut stack: Vec<usize> = Vec::new();
        let mut malformed = false;
        for (i, t) in tokens.iter().enumerate() {
            depth[i] = stack.len();
            if t.raw == "{" || t.raw == "<<" {
                stack.push(i);
            } else if t.raw == "}" || t.raw == ">>" {
                if let Some(open) = stack.pop() {
                    if (tokens[open].raw == "{") != (t.raw == "}") {
                        malformed = true;
                    } else {
                        close[open] = Some(i);
                    }
                } else {
                    malformed = true;
                }
            }
        }
        malformed |= !stack.is_empty();
        Self {
            tokens,
            close,
            depth,
            malformed,
        }
    }

    pub(crate) fn expression(&self, from: usize, limit: usize) -> Option<(usize, usize)> {
        let t = self.tokens.get(from)?;
        if t.raw == "{" || t.raw == "<<" {
            return self.close[from]
                .filter(|&c| c < limit)
                .map(|c| (from, c + 1));
        }
        if matches!(
            t.command(),
            Some(
                "relative"
                    | "absolute"
                    | "transpose"
                    | "chordmode"
                    | "notemode"
                    | "drummode"
                    | "lyricmode"
                    | "repeat"
            )
        ) {
            for j in from + 1..limit {
                if self.tokens[j].raw == "="
                    || self.tokens[j].raw == "}"
                    || self.tokens[j].raw == ">>"
                {
                    break;
                }
                if self.tokens[j].raw == "{" || self.tokens[j].raw == "<<" {
                    return self.close[j].filter(|&c| c < limit).map(|c| (j, c + 1));
                }
            }
            return None;
        }
        (t.kind == TokenKind::Cmd).then_some((from, from + 1))
    }
}

#[derive(Clone)]
pub(crate) struct Assignment {
    pub(crate) name: String,
    pub(crate) start: usize,
    pub(crate) rhs: usize,
    pub(crate) open: usize,
    pub(crate) end: usize,
}

pub(crate) fn unsupported_token(t: &Token) -> bool {
    matches!(
        t.command(),
        Some(
            "chordmode"
                | "repeat"
                | "unfoldRepeats"
                | "relative-includes"
                | "lyricmode"
                | "drummode"
        )
    ) || (t.kind == TokenKind::Scheme
        && (t.raw.starts_with("#(")
            || t.raw.starts_with("$(")
            || t.raw.starts_with("#{")
            || t.raw.starts_with("#'(")))
}

pub(crate) fn region(
    source: &str,
    name: String,
    start: usize,
    rhs: usize,
    open: usize,
    end: usize,
    syntax: &Syntax,
    decorations: &HashSet<String>,
) -> VoiceRegion {
    let map = SourceMap::new(source);
    let tokens = &syntax.tokens;
    let byte_end = tokens[end - 1].byte_end;
    let span = map.span(tokens[start].byte_start, byte_end).unwrap();
    let music_span = map.span(tokens[rhs].byte_start, byte_end).unwrap();
    let body_start = if tokens[open].raw == "{" {
        tokens[open].byte_end
    } else {
        tokens[open].byte_start
    };
    let body_end = if tokens[end - 1].raw == "}" {
        tokens[end - 1].byte_start
    } else {
        byte_end
    };
    let mut reason = tokens[rhs..end]
        .iter()
        .find(|t| unsupported_token(t))
        .map(|t| format!("unsupported music structure: {}", t.raw));
    if tokens[rhs..end].iter().any(|t| {
        t.mode != Mode::Text
            && t.command().is_some_and(|cmd| {
                !crate::pitch::known_command(cmd)
                    && !matches!(cmd, "markup" | "compressMMRests" | "notemode" | "skip")
                    && !decorations.contains(cmd)
            })
    }) {
        reason.get_or_insert("unresolved music command or variable".into());
    }
    let mut measure_groups = Vec::new();
    let base_depth = syntax.depth[open].saturating_add(1);
    let simultaneous =
        (open + 1..end - 1).find(|&i| tokens[i].raw == "<<" && syntax.depth[i] == base_depth);
    if let Some(seq) = simultaneous {
        if let Some(seq_end) = syntax.close[seq] {
            let mut starts = vec![seq + 1];
            starts.extend(
                (seq + 1..seq_end)
                    .filter(|&i| {
                        syntax.depth[i] == base_depth.saturating_add(1)
                            && tokens[i].command() == Some("\\")
                    })
                    .map(|i| i + 1),
            );
            let mut ends = starts[1..]
                .iter()
                .map(|start| start.saturating_sub(1))
                .collect::<Vec<_>>();
            ends.push(seq_end);
            for (start, end) in starts.into_iter().zip(ends) {
                let Some((body_start, body_end)) = expression_body(tokens, syntax, start, end)
                else {
                    reason.get_or_insert(
                        "simultaneous music branch is not a music expression".into(),
                    );
                    break;
                };
                let (measures, branch_reason) =
                    parse_measure_body(source, body_start, body_end, &map, true);
                if let Some(branch_reason) = branch_reason {
                    reason.get_or_insert(branch_reason);
                }
                measure_groups.push(measures);
            }
        } else {
            reason.get_or_insert("unbalanced simultaneous music".into());
        }
        if measure_groups.is_empty() {
            measure_groups.push(Vec::new());
        }
    } else {
        let (measures, body_reason) = parse_measure_body(source, body_start, body_end, &map, false);
        if let Some(body_reason) = body_reason {
            reason.get_or_insert(body_reason);
        }
        measure_groups.push(measures);
    }
    if reason.is_some() {
        measure_groups.iter_mut().for_each(Vec::clear);
    }
    let measures = measure_groups.iter().flatten().cloned().collect();
    VoiceRegion {
        name,
        span,
        music_span,
        measures,
        measure_groups,
        capability: if reason.is_some() {
            LocationCapability::AnchorOnly
        } else {
            LocationCapability::Measure
        },
        reason,
    }
}

fn expression_body(
    tokens: &[Token],
    syntax: &Syntax,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    let first = (start..end).find(|&i| !tokens[i].trivia())?;
    let last = (start..end).rev().find(|&i| !tokens[i].trivia())?;
    if tokens[first].raw == "{" && tokens[last].raw == "}" {
        if syntax.close[first] != Some(last) {
            return None;
        }
        return Some((tokens[first].byte_end, tokens[last].byte_start));
    }
    Some((tokens[first].byte_start, tokens[last].byte_end))
}

fn parse_measure_body(
    source: &str,
    body_start: usize,
    body_end: usize,
    map: &SourceMap<'_>,
    implicit_end: bool,
) -> (Vec<super::Measure>, Option<String>) {
    let body = &source[body_start..body_end];
    let events = tokenize(&format!("{{ {body} }}"));
    let mut measures: Vec<super::Measure> = Vec::new();
    let mut reason = None;
    let mut from = body_start;
    let mut next = 1u32;
    // The temporary brace puts plain inline context bodies into music mode. Offsets shift by two bytes.
    let mut has_music = false;
    let mut trailing_music = false;
    let mut marked = HashSet::new();
    for (index, t) in events.iter().enumerate() {
        if t.byte_start < 2 || t.byte_end > body.len() + 2 {
            continue;
        }
        if t.mode == Mode::Music
            && matches!(
                t.kind,
                TokenKind::Note | TokenKind::Rest | TokenKind::Skip | TokenKind::ChordO
            )
        {
            has_music = true;
            trailing_music = true;
        }
        if t.kind == TokenKind::Comment && t.mode == Mode::Music {
            let marker = t.raw.strip_prefix('%').unwrap_or("").trim();
            if !marker.is_empty()
                && marker.bytes().all(|b| b.is_ascii_digit())
                && marker.parse::<u32>().is_err()
            {
                reason.get_or_insert("measure marker exceeds supported range".into());
            }
            if let Some(n) = t
                .raw
                .strip_prefix('%')
                .and_then(|s| s.trim().parse::<u32>().ok())
            {
                if let Some(last) = measures.last_mut() {
                    if n < last.measure_start {
                        reason.get_or_insert("non-monotonic measure marker".into());
                    } else {
                        last.measure_end = n;
                        next = n.saturating_add(1);
                        marked.insert(last.span.byte_start);
                        let end = body_start + t.byte_end - 2;
                        last.span = map.span(last.span.byte_start, end).unwrap();
                        from = end;
                    }
                }
            }
        }
        let boundary = if t.kind == TokenKind::Bar && t.mode == Mode::Music {
            Some(t.byte_end)
        } else if t.command() == Some("bar") && t.mode != Mode::Text {
            Some(
                events[index + 1..]
                    .iter()
                    .find(|next| !next.trivia())
                    .map_or(t.byte_end, |next| next.byte_end),
            )
        } else {
            None
        };
        if let Some(boundary) = boundary.filter(|_| has_music) {
            let to = body_start + boundary - 2;
            measures.push(super::Measure {
                measure_start: next,
                measure_end: next,
                span: map.span(from, to).unwrap(),
            });
            next = next.saturating_add(1);
            from = to;
            trailing_music = false;
            has_music = false;
        }
    }
    if trailing_music && implicit_end && has_music {
        measures.push(super::Measure {
            measure_start: next,
            measure_end: next,
            span: map.span(from, body_end).unwrap(),
        });
        trailing_music = false;
    }
    if measures.is_empty() || trailing_music {
        reason.get_or_insert("music without explicit complete measure boundaries".into());
    }
    let meaningful: Vec<_> = events.iter().filter(|t| !t.trivia()).collect();
    for w in meaningful.windows(2) {
        if matches!(w[0].kind, TokenKind::Rest | TokenKind::Skip) && w[1].kind == TokenKind::Star {
            let byte = body_start + w[0].byte_start.saturating_sub(2);
            if measures.iter().any(|m| {
                byte >= m.span.byte_start
                    && byte < m.span.byte_end
                    && !marked.contains(&m.span.byte_start)
            }) {
                reason.get_or_insert("multiplied rest requires measure-number markers".into());
            }
        }
    }
    (measures, reason)
}
