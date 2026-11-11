use crate::{Token, TokenKind};
use ossia_types::{
    AppError, ContentHash, ErrorCode, LocatedRegion, LocationCapability, Locator, SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod syntax;
pub(crate) use syntax::{region, unsupported_token, Assignment, Syntax};

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
