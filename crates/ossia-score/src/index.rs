use crate::{tokenize, Mode, SourceMap, Token, TokenKind};
use ossia_types::{
    AppError, ContentHash, ErrorCode, LocatedRegion, LocationCapability, Locator, SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measure {
    pub measure_start: u32,
    pub measure_end: u32,
    pub span: SourceSpan,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRegion {
    pub name: String,
    pub span: SourceSpan,
    pub music_span: SourceSpan,
    pub measures: Vec<Measure>,
    pub capability: LocationCapability,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffVoices {
    pub staff_index: usize,
    pub label: Option<String>,
    pub voices: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreIndex {
    pub source_hash: ContentHash,
    pub voices: Vec<VoiceRegion>,
    pub staves: Vec<StaffVoices>,
    pub max_measures: u32,
    pub capability: LocationCapability,
    pub reason: Option<String>,
    #[serde(skip)]
    source: String,
}
impl ScoreIndex {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn voice(&self, name: &str) -> Option<&VoiceRegion> {
        self.voices.iter().find(|v| v.name == name)
    }
    pub(crate) fn measure_at(&self, byte: usize) -> Option<&Measure> {
        self.voices
            .iter()
            .flat_map(|v| &v.measures)
            .find(|m| byte >= m.span.byte_start && byte < m.span.byte_end)
    }
}

pub(crate) struct Syntax {
    pub tokens: Vec<Token>,
    pub close: Vec<Option<usize>>,
    pub depth: Vec<usize>,
    pub malformed: bool,
}
impl Syntax {
    pub fn new(source: &str) -> Self {
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
    pub fn expression(&self, from: usize, limit: usize) -> Option<(usize, usize)> {
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
struct Assignment {
    name: String,
    start: usize,
    rhs: usize,
    open: usize,
    end: usize,
}

pub(crate) fn unsupported_token(t: &Token) -> bool {
    matches!(
        t.command(),
        Some(
            "transpose"
                | "chordmode"
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

fn region(
    source: &str,
    name: String,
    start: usize,
    rhs: usize,
    open: usize,
    end: usize,
    syntax: &Syntax,
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
            })
    }) {
        reason.get_or_insert("unresolved music command or variable".into());
    }
    if tokens[open].raw == "<<" {
        reason.get_or_insert("simultaneous music requires separate voices".into());
    }
    let body = &source[body_start..body_end];
    let events = tokenize(&format!("{{ {body} }}"));
    let mut measures: Vec<Measure> = Vec::new();
    let mut from = body_start;
    let mut next = 1u32;
    // The temporary brace puts plain inline context bodies into music mode. Offsets shift by two bytes.
    let mut has_music = false;
    let mut trailing_music = false;
    let mut marked = HashSet::new();
    for t in events
        .iter()
        .filter(|t| t.byte_start >= 2 && t.byte_end <= body.len() + 2)
    {
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
        if t.kind == TokenKind::Bar && t.mode == Mode::Music && has_music {
            let to = body_start + t.byte_end - 2;
            measures.push(Measure {
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
    if reason.is_some() {
        measures.clear();
    }
    VoiceRegion {
        name,
        span,
        music_span,
        measures,
        capability: if reason.is_some() {
            LocationCapability::AnchorOnly
        } else {
            LocationCapability::Measure
        },
        reason,
    }
}

/// Index a supported subset without interpreting LilyPond or rejecting legal, opaque music.
pub fn build_index(source: &str) -> ScoreIndex {
    let syntax = Syntax::new(source);
    let t = &syntax.tokens;
    let mut assignments = HashMap::<String, Assignment>::new();
    let mut ordered = Vec::new();
    let mut duplicate = false;
    for i in 0..t.len().saturating_sub(2) {
        if syntax.depth[i] == 0 && t[i].kind == TokenKind::Ident && t[i + 1].raw == "=" {
            if let Some((open, end)) = syntax.expression(i + 2, t.len()) {
                let a = Assignment {
                    name: t[i].raw.clone(),
                    start: i,
                    rhs: i + 2,
                    open,
                    end,
                };
                ordered.push(a.clone());
                duplicate |= assignments.insert(a.name.clone(), a).is_some();
            }
        }
    }
    let mut voices = Vec::new();
    for a in &ordered {
        if t[a.open].raw != "{" {
            continue;
        }
        let body = &t[a.open + 1..a.end - 1];
        if body
            .iter()
            .any(|v| matches!(v.kind, TokenKind::Note | TokenKind::Rest | TokenKind::Skip))
            || t[a.rhs..a.end].iter().any(unsupported_token)
        {
            voices.push(region(
                source,
                a.name.clone(),
                a.start,
                a.rhs,
                a.open,
                a.end,
                &syntax,
            ));
        }
    }
    let mut staves: Vec<StaffVoices> = Vec::new();
    let mut by_label = HashMap::<String, usize>::new();
    let mut reason = syntax
        .malformed
        .then(|| "unbalanced source structure".to_string());
    if duplicate {
        reason = Some("duplicate music assignments require evaluation".into());
    }
    #[derive(Clone)]
    struct Task {
        from: usize,
        end: usize,
        staff: Option<usize>,
        path: Vec<String>,
    }
    let mut tasks = Vec::new();
    for (i, token) in t.iter().enumerate() {
        if syntax.depth[i] == 0 && token.command() == Some("score") {
            if let Some((open, end)) = syntax.expression(i + 1, t.len()) {
                tasks.push(Task {
                    from: open,
                    end,
                    staff: None,
                    path: Vec::new(),
                });
            }
        }
    }
    if tasks.len() > 1 {
        reason.get_or_insert("multiple score blocks have ambiguous engraving order".into());
    }
    if tasks.is_empty() {
        tasks.push(Task {
            from: 0,
            end: t.len(),
            staff: None,
            path: Vec::new(),
        });
    }
    let mut budget = t.len().saturating_mul(64).min(1_000_000);
    'walk: while let Some(task) = tasks.pop() {
        let mut i = task.from;
        while i < task.end {
            if budget == 0 {
                reason.get_or_insert("music reference expansion limit exceeded".into());
                break 'walk;
            }
            budget -= 1;
            let cmd = t[i].command();
            if syntax.depth[i] == 0 && i + 1 < t.len() && t[i + 1].raw == "=" {
                if let Some(a) = assignments.get(&t[i].raw) {
                    i = a.end;
                    continue;
                }
            }
            if matches!(
                cmd,
                Some("layout" | "midi" | "paper" | "header" | "with" | "markup")
            ) {
                if let Some((_, end)) = syntax.expression(i + 1, task.end) {
                    i = end;
                    continue;
                }
            }
            if matches!(cmd, Some("new" | "context")) && i + 1 < task.end {
                let what = t[i + 1].raw.as_str();
                let mut body = i + 2;
                let label = if t.get(body).is_some_and(|v| v.raw == "=") {
                    body += 2;
                    t.get(body - 1).map(|v| v.raw.trim_matches('"').to_string())
                } else {
                    None
                };
                if t.get(body).and_then(Token::command) == Some("with") {
                    if let Some((_, end)) = syntax.expression(body + 1, task.end) {
                        body = end;
                    }
                }
                if let Some((open, end)) = syntax.expression(body, task.end) {
                    let staff = if what == "Staff" {
                        if cmd == Some("context") && label.is_none() && task.staff.is_some() {
                            task.staff
                        } else if let Some(found) = label
                            .as_ref()
                            .and_then(|l| by_label.get(l))
                            .filter(|_| cmd == Some("context"))
                        {
                            Some(*found)
                        } else {
                            let n = staves.len();
                            staves.push(StaffVoices {
                                staff_index: n,
                                label: label.clone(),
                                voices: Vec::new(),
                            });
                            if let Some(label) = label {
                                by_label.insert(label, n);
                            }
                            Some(n)
                        }
                    } else {
                        task.staff
                    };
                    if what == "Staff"
                        && t[open].raw == "{"
                        && !t[open + 1..end - 1].iter().any(|v| {
                            v.command().is_some_and(|c| {
                                assignments.contains_key(c) || matches!(c, "context" | "new")
                            })
                        })
                    {
                        let name = format!("@inline:{}", t[open].byte_start);
                        let v = region(source, name.clone(), body, body, open, end, &syntax);
                        if let Some(staff) = staff {
                            staves[staff].voices.push(name);
                        }
                        voices.push(v);
                    } else {
                        tasks.push(Task {
                            from: end,
                            ..task.clone()
                        });
                        tasks.push(Task {
                            from: body,
                            end,
                            staff,
                            path: task.path.clone(),
                        });
                        break;
                    }
                    i = end;
                    continue;
                }
            }
            if let Some(a) = cmd.and_then(|c| assignments.get(c)) {
                if let Some(staff) = task.staff {
                    if voices.iter().any(|v| v.name == a.name) {
                        if !staves[staff].voices.contains(&a.name) {
                            staves[staff].voices.push(a.name.clone());
                        }
                    } else if task.path.contains(&a.name) || task.path.len() >= 128 {
                        reason.get_or_insert("cyclic or deeply nested music references".into());
                    } else {
                        let mut path = task.path.clone();
                        path.push(a.name.clone());
                        tasks.push(Task {
                            from: i + 1,
                            ..task.clone()
                        });
                        tasks.push(Task {
                            from: a.rhs,
                            end: a.end,
                            staff: Some(staff),
                            path,
                        });
                        break;
                    }
                }
                i += 1;
                continue;
            }
            if (t[i].raw == "{" || t[i].raw == "<<") && syntax.close[i].is_some() {
                let end = syntax.close[i].unwrap() + 1;
                tasks.push(Task {
                    from: end,
                    ..task.clone()
                });
                tasks.push(Task {
                    from: i + 1,
                    end: end - 1,
                    staff: task.staff,
                    path: task.path.clone(),
                });
                break;
            }
            i += 1;
        }
    }
    for v in &mut voices {
        if let Some(a) = assignments.get(&v.name) {
            if t[a.open + 1..a.end - 1]
                .iter()
                .any(|token| token.command().is_some_and(|c| assignments.contains_key(c)))
            {
                v.capability = LocationCapability::AnchorOnly;
                v.reason = Some("composed music variable requires expansion".into());
                v.measures.clear();
            }
        }
    }
    if reason.is_some() {
        for v in &mut voices {
            v.measures.clear();
            v.capability = LocationCapability::AnchorOnly;
            v.reason = reason.clone();
        }
    }
    let max_measures = voices
        .iter()
        .flat_map(|v| &v.measures)
        .map(|m| m.measure_end)
        .max()
        .unwrap_or(0);
    let capability = if voices
        .iter()
        .any(|v| v.capability == LocationCapability::Measure)
    {
        LocationCapability::Measure
    } else if !source.is_empty() {
        LocationCapability::AnchorOnly
    } else {
        LocationCapability::None
    };
    if capability != LocationCapability::Measure && reason.is_none() {
        reason = Some("no reliable measure index".into());
    }
    ScoreIndex {
        source_hash: ContentHash::of(source.as_bytes()),
        voices,
        staves,
        max_measures,
        capability,
        reason,
        source: source.into(),
    }
}

/// Resolve only voices belonging to the explicitly requested staff; no cross-staff fallback.
pub fn resolve_locator(index: &ScoreIndex, locator: &Locator) -> Result<LocatedRegion, AppError> {
    if locator.measure_start == 0 || locator.measure_end < locator.measure_start {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            "invalid measure range",
        ));
    }
    let staff = index
        .staves
        .get(locator.staff_index)
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "staff not found"))?;
    let names: Vec<_> = if let Some(name) = &locator.voice {
        if !staff.voices.contains(name) {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "voice not found on staff",
            ));
        }
        vec![name]
    } else {
        staff.voices.iter().collect()
    };
    let mut spans = Vec::new();
    let mut reason = None;
    for name in names {
        if let Some(voice) = index.voice(name) {
            if voice.capability != LocationCapability::Measure {
                reason = voice.reason.clone();
            }
            spans.extend(
                voice
                    .measures
                    .iter()
                    .filter(|m| {
                        m.measure_end >= locator.measure_start
                            && m.measure_start <= locator.measure_end
                    })
                    .map(|m| m.span),
            );
        }
    }
    let capability = if reason.is_some() {
        spans.clear();
        LocationCapability::AnchorOnly
    } else if spans.is_empty() {
        reason = Some("no-hit".into());
        LocationCapability::None
    } else {
        LocationCapability::Measure
    };
    Ok(LocatedRegion {
        locator: locator.clone(),
        spans,
        capability,
        reason,
    })
}
