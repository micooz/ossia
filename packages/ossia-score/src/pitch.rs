use crate::{
    index::{unsupported_token, Syntax},
    tokenize, Mode, SourceMap, TokenKind,
};
use ossia_types::{AppError, SourceSpan};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PitchMember {
    pub raw: String,
    pub midi: i32,
    pub di: i32,
    pub alter: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PitchEventKind {
    Note,
    Chord,
    Rest,
    Skip,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PitchEvent {
    pub kind: PitchEventKind,
    pub raw: String,
    pub span: SourceSpan,
    pub pitches: Vec<i32>,
    pub members: Vec<PitchMember>,
    pub ref_di: i32,
    pub out_of_range: bool,
    #[serde(default)]
    pub voice: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PitchResult {
    pub events: Vec<PitchEvent>,
    pub end_ref_di: Option<i32>,
    /// When present, the prediction is incomplete; events may cover only supported branches.
    pub unsupported: Option<String>,
}

/// Canonical scientific spelling for a MIDI pitch. MIDI has no enharmonic intent,
/// so this intentionally uses sharps; source tokens remain authoritative.
pub fn midi_pitch_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = i16::from(pitch) / 12 - 1;
    format!("{}{}", NAMES[usize::from(pitch % 12)], octave)
}
#[derive(Clone, Copy)]
struct Pitch {
    step: i32,
    alter: i32,
    marks: i32,
}
fn parse_pitch(s: &str) -> Option<Pitch> {
    let step = match *s.as_bytes().first()? {
        b'c' => 0,
        b'd' => 1,
        b'e' => 2,
        b'f' => 3,
        b'g' => 4,
        b'a' => 5,
        b'b' | b'h' => 6,
        _ => return None,
    };
    let mut tail = &s[1..];
    let mut alter = 0;
    for (acc, n) in [
        ("isis", 2),
        ("eses", -2),
        ("ses", i32::MAX),
        ("is", 1),
        ("es", -1),
        ("ss", 2),
        ("ff", -2),
        ("s", 1),
        ("f", -1),
    ] {
        if let Some(rest) = tail.strip_prefix(acc) {
            if n == i32::MAX {
                return None;
            }
            alter = n;
            tail = rest;
            break;
        }
    }
    let mut marks = 0i32;
    while let Some(c) = tail.chars().next().filter(|c| *c == '\'' || *c == ',') {
        marks = marks.checked_add(if c == '\'' { 1 } else { -1 })?;
        tail = &tail[1..];
    }
    tail = tail.strip_prefix('!').unwrap_or(tail);
    if !tail.is_empty()
        && (!tail.as_bytes()[0].is_ascii_digit()
            || !tail.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
    {
        return None;
    }
    Some(Pitch { step, alter, marks })
}
fn di(p: Pitch) -> i32 {
    p.marks.saturating_mul(7).saturating_add(p.step)
}
fn midi(di: i32, alter: i32) -> i32 {
    48i32
        .saturating_add(12i32.saturating_mul(di.div_euclid(7)))
        .saturating_add([0, 2, 4, 5, 7, 9, 11][di.rem_euclid(7) as usize])
        .saturating_add(alter)
}
fn unsupported(reason: impl Into<String>) -> PitchResult {
    PitchResult {
        events: Vec::new(),
        end_ref_di: None,
        unsupported: Some(reason.into()),
    }
}
pub(crate) fn known_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "relative"
            | "absolute"
            | "transpose"
            | "set"
            | "key"
            | "major"
            | "minor"
            | "time"
            | "numericTimeSignature"
            | "clef"
            | "tempo"
            | "ottava"
            | "partial"
            | "bar"
            | "barNumberCheck"
            | "change"
            | "break"
            | "noBreak"
            | "pageBreak"
            | "noPageBreak"
            | "voiceOne"
            | "voiceTwo"
            | "voiceThree"
            | "voiceFour"
            | "oneVoice"
            | "stemUp"
            | "stemDown"
            | "stemNeutral"
            | "tieUp"
            | "tieDown"
            | "slurUp"
            | "slurDown"
            | "slurNeutral"
            | "autoBeamOff"
            | "autoBeamOn"
            | "tuplet"
            | "times"
            | "fermata"
            | "arpeggio"
            | "laissezVibrer"
            | "breathe"
            | "bold"
            | "mp"
            | "mf"
            | "ppp"
            | "pp"
            | "p"
            | "f"
            | "ff"
            | "fff"
            | "sfz"
            | "fp"
            | "cresc"
            | "decresc"
            | "dim"
            | "<"
            | ">"
            | "!"
            | "("
            | ")"
            | "["
            | "]"
            | "\\"
            | "compressMMRests"
            | "mergeDifferentlyDottedOn"
            | "mergeDifferentlyHeadedOn"
            | "mark"
            | "square"
    )
}

/// Resolve sounding pitches in one complete music expression while retaining
/// literal source tokens. Opaque music returns no prediction. Note-on timing,
/// ties, articulations and duration are outside this interface.
pub fn resolve_pitches(source: &str, span: SourceSpan) -> Result<PitchResult, AppError> {
    let map = SourceMap::new(source);
    let text = map.slice(span)?;
    let document_tokens = tokenize(source);
    for (i, token) in document_tokens.iter().enumerate() {
        if token.command() == Some("language") {
            return Ok(unsupported(
                "document note language is outside the verified pitch subset",
            ));
        }
        if token.command() == Some("include") {
            let next = document_tokens[i + 1..].iter().find(|t| !t.trivia());
            if !next
                .is_some_and(|t| matches!(t.raw.as_str(), "\"english.ly\"" | "\"nederlands.ly\""))
            {
                return Ok(unsupported(
                    "included definitions may change pitch semantics",
                ));
            }
        }
    }
    let syntax = Syntax::new(text);
    if syntax.malformed {
        return Err(AppError::invalid("unbalanced pitch expression"));
    }
    let tokens = tokenize(text);
    let mut active = true;
    for t in &tokens {
        // Scheme inside markup is text, but executable Scheme inside music is opaque.
        if t.mode == Mode::Text {
            continue;
        }
        if unsupported_token(t) {
            return Ok(unsupported(format!(
                "{} is outside the verified pitch subset",
                t.raw
            )));
        }
    }
    if let Some(result) = resolve_simultaneous(source, span, &syntax)? {
        return Ok(result);
    }
    let mut events = Vec::new();
    let mut ref_di = 0i32;
    let mut relative = false;
    let mut transpose = 0i32;
    let mut depth = 0usize;
    let mut states: Vec<(usize, bool, i32, i32)> = Vec::new();
    let mut pending: Option<(bool, i32)> = None;
    let mut pending_transpose = None;
    let mut chord: Option<(usize, Vec<PitchMember>)> = None;
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.mode == Mode::Text {
            i += 1;
            continue;
        }
        if t.command() == Some("markup") {
            // Lexer inheritance suppresses all markup notes. Only this command is a text transition.
            active = false;
            i += 1;
            continue;
        }
        if !active {
            if t.raw == "{" {
                i += 1;
                active = true;
                continue;
            }
            if t.kind == TokenKind::String {
                active = true;
                i += 1;
                continue;
            }
            if t.kind == TokenKind::Ws
                || t.kind == TokenKind::Comment
                || t.kind == TokenKind::Cmd
                || t.kind == TokenKind::Num
            {
                i += 1;
                continue;
            }
            return Ok(unsupported("unbounded markup expression"));
        }
        if let Some(cmd) = t.command() {
            if cmd == "relative" {
                let next = (i + 1..tokens.len()).find(|&j| !tokens[j].trivia());
                let Some(j) = next else {
                    return Ok(unsupported("relative has no reference pitch"));
                };
                let Some(p) = parse_pitch(&tokens[j].raw) else {
                    return Ok(unsupported(
                        "relative reference is outside the verified subset",
                    ));
                };
                pending = Some((true, di(p)));
                i = j + 1;
                continue;
            }
            if cmd == "transpose" {
                let mut pitches = (i + 1..tokens.len()).filter(|&j| !tokens[j].trivia());
                let Some(from_index) = pitches.next() else {
                    return Ok(unsupported("transpose has no source pitch"));
                };
                let Some(to_index) = pitches.next() else {
                    return Ok(unsupported("transpose has no target pitch"));
                };
                let Some(from) = parse_pitch(&tokens[from_index].raw) else {
                    return Ok(unsupported(
                        "transpose source is outside the verified pitch subset",
                    ));
                };
                let Some(to) = parse_pitch(&tokens[to_index].raw) else {
                    return Ok(unsupported(
                        "transpose target is outside the verified pitch subset",
                    ));
                };
                pending_transpose =
                    Some(midi(di(to), to.alter).saturating_sub(midi(di(from), from.alter)));
                i = to_index + 1;
                continue;
            }
            if cmd == "absolute" {
                pending = Some((false, ref_di));
                i += 1;
                continue;
            }
            if !known_command(cmd) {
                return Ok(unsupported(format!("unresolved music command: \\{cmd}")));
            }
            i += 1;
            continue;
        }
        if t.raw == "<<" || t.raw == ">>" {
            return Ok(unsupported(
                "simultaneous music needs separate pitch states",
            ));
        }
        if t.kind == TokenKind::Brace {
            if t.raw == "{" {
                depth += 1;
                states.push((depth, relative, ref_di, transpose));
                if let Some((rel, new_ref)) = pending.take() {
                    relative = rel;
                    ref_di = new_ref;
                }
                if let Some(offset) = pending_transpose.take() {
                    transpose = transpose.saturating_add(offset);
                }
            } else {
                if let Some((_, rel, old_ref, old_transpose)) = states.pop_if(|s| s.0 == depth) {
                    // The outer expression exposes its final reference; nested wrappers restore the parent.
                    if depth > 1 {
                        relative = rel;
                        ref_di = old_ref;
                        transpose = old_transpose;
                    }
                }
                depth = depth.saturating_sub(1);
            }
            i += 1;
            continue;
        }
        if t.kind == TokenKind::ChordO {
            if chord.is_some() {
                return Err(AppError::invalid("nested chord"));
            }
            chord = Some((t.byte_start, Vec::new()));
            i += 1;
            continue;
        }
        if t.kind == TokenKind::ChordC {
            let Some((start, mut members)) = chord.take() else {
                return Err(AppError::invalid("unmatched chord close"));
            };
            if members.is_empty() {
                return Ok(unsupported("empty chord"));
            }
            ref_di = members[0].di;
            for member in &mut members {
                member.midi = member.midi.saturating_add(transpose);
            }
            let mut pitches: Vec<_> = members.iter().map(|p| p.midi).collect();
            pitches.sort_unstable();
            pitches.dedup();
            let event_span = map.span(span.byte_start + start, span.byte_start + t.byte_end)?;
            events.push(PitchEvent {
                kind: PitchEventKind::Chord,
                raw: map.slice(event_span)?.into(),
                span: event_span,
                out_of_range: pitches.iter().any(|n| !(0..=127).contains(n)),
                pitches,
                members,
                ref_di,
                voice: None,
            });
            i += 1;
            continue;
        }
        if t.mode == Mode::Music
            && matches!(t.kind, TokenKind::Note | TokenKind::Rest | TokenKind::Skip)
        {
            let event_span =
                map.span(span.byte_start + t.byte_start, span.byte_start + t.byte_end)?;
            if matches!(t.kind, TokenKind::Rest | TokenKind::Skip) {
                events.push(PitchEvent {
                    kind: if t.kind == TokenKind::Rest {
                        PitchEventKind::Rest
                    } else {
                        PitchEventKind::Skip
                    },
                    raw: t.raw.clone(),
                    span: event_span,
                    pitches: Vec::new(),
                    members: Vec::new(),
                    ref_di,
                    out_of_range: false,
                    voice: None,
                });
            } else {
                let Some(p) = parse_pitch(&t.raw) else {
                    return Ok(unsupported(format!(
                        "unsupported pitch spelling: {}",
                        t.raw
                    )));
                };
                let d = if relative {
                    let delta = (p.step - ref_di.rem_euclid(7) + 3).rem_euclid(7) - 3;
                    ref_di
                        .saturating_add(delta)
                        .saturating_add(p.marks.saturating_mul(7))
                } else {
                    di(p)
                };
                let n = midi(d, p.alter).saturating_add(transpose);
                ref_di = d;
                let member = PitchMember {
                    raw: t.raw.clone(),
                    midi: n,
                    di: d,
                    alter: p.alter,
                };
                if let Some((_, members)) = &mut chord {
                    members.push(member);
                } else {
                    events.push(PitchEvent {
                        kind: PitchEventKind::Note,
                        raw: t.raw.clone(),
                        span: event_span,
                        pitches: vec![n],
                        members: vec![member],
                        ref_di,
                        out_of_range: !(0..=127).contains(&n),
                        voice: None,
                    });
                }
            }
        }
        i += 1;
    }
    if chord.is_some() {
        return Err(AppError::invalid("unclosed chord"));
    }
    if pending.is_some() {
        return Ok(unsupported("pitch wrapper has no music body"));
    }
    Ok(PitchResult {
        events,
        end_ref_di: Some(ref_di),
        unsupported: None,
    })
}

fn resolve_simultaneous(
    source: &str,
    span: SourceSpan,
    syntax: &Syntax,
) -> Result<Option<PitchResult>, AppError> {
    let tokens = &syntax.tokens;
    let Some(first) = (0..tokens.len()).find(|&i| tokens[i].raw == "<<") else {
        return Ok(None);
    };
    let Some(last) = syntax.close[first] else {
        return Ok(None);
    };
    if tokens[..first]
        .iter()
        .chain(tokens[last + 1..].iter())
        .any(music_token)
        || tokens[..first]
            .iter()
            .any(|token| matches!(token.command(), Some("relative" | "transpose")))
    {
        return Ok(None);
    }
    let base = syntax.depth[first].saturating_add(1);
    let mut starts = vec![first + 1];
    starts.extend(
        (first + 1..last)
            .filter(|&i| syntax.depth[i] == base && tokens[i].command() == Some("\\"))
            .map(|i| i + 1),
    );
    let mut ends = starts[1..]
        .iter()
        .map(|start| start.saturating_sub(1))
        .collect::<Vec<_>>();
    ends.push(last);
    let map = SourceMap::new(source);
    let mut events = Vec::new();
    let mut unsupported = None;
    for (branch, (start, end)) in starts.into_iter().zip(ends).enumerate() {
        let Some((body_start, body_end)) = branch_body(tokens, syntax, start, end) else {
            unsupported.get_or_insert("simultaneous music branch is not a music expression".into());
            continue;
        };
        let branch_span = map.span(
            span.byte_start.saturating_add(body_start),
            span.byte_start.saturating_add(body_end),
        )?;
        let result = resolve_pitches(source, branch_span)?;
        if let Some(reason) = result.unsupported {
            unsupported.get_or_insert(reason);
        }
        for mut event in result.events {
            event.voice = Some(branch + 1);
            events.push(event);
        }
    }
    if events.is_empty() && unsupported.is_none() {
        unsupported = Some("simultaneous music has no verifiable notes".into());
    }
    Ok(Some(PitchResult {
        events,
        end_ref_di: None,
        unsupported,
    }))
}

fn music_token(token: &crate::Token) -> bool {
    matches!(
        token.kind,
        crate::TokenKind::Note
            | crate::TokenKind::Rest
            | crate::TokenKind::Skip
            | crate::TokenKind::ChordO
    )
}

fn branch_body(
    tokens: &[crate::Token],
    syntax: &Syntax,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    let first = (start..end).find(|&i| !tokens[i].trivia())?;
    let last = (start..end).rev().find(|&i| !tokens[i].trivia())?;
    if tokens[first].raw == "{" && tokens[last].raw == "}" && syntax.close[first] != Some(last) {
        return None;
    }
    Some((tokens[first].byte_start, tokens[last].byte_end))
}
