use crate::{build_index, tokenize, ScoreIndex, Token, TokenKind};

/// Conservative musical identity used to decide whether a logical selection can
/// survive a source-only edit such as an annotation or layout change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicIdentity {
    pub reliable: bool,
    staves: Vec<IdentityStaff>,
    voices: Vec<IdentityVoice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityStaff {
    label: Option<String>,
    voices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityVoice {
    raw: Vec<IdentityToken>,
    measures: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityToken {
    kind: TokenKind,
    raw: String,
}

/// Derive a conservative identity from referenced music voices.
///
/// Layout, MIDI setup and markup expressions are excluded. Opaque or partially
/// indexed scores are marked unreliable, which makes selection retention fail
/// closed.
pub fn music_identity(source: &str) -> MusicIdentity {
    let index = build_index(source);
    music_identity_with_index(&index, source)
}

/// Derive identity while reusing an index already cached by the runtime.
pub fn music_identity_with_index(index: &ScoreIndex, source: &str) -> MusicIdentity {
    let mut reliable = index.capability == crate::LocationCapability::Measure
        && index.measure_count().is_ok()
        && !index.staves.is_empty();
    let mut staves = Vec::new();
    let mut voices = Vec::new();
    for staff in &index.staves {
        let normalized_voices = staff
            .voices
            .iter()
            .enumerate()
            .map(|(position, name)| {
                if name.starts_with("@inline:") {
                    format!("@inline:{position}")
                } else {
                    name.clone()
                }
            })
            .collect::<Vec<_>>();
        staves.push(IdentityStaff {
            label: staff.label.clone(),
            voices: normalized_voices,
        });
        for name in &staff.voices {
            let Some(region) = index.voice(name) else {
                reliable = false;
                continue;
            };
            if region.capability != crate::LocationCapability::Measure {
                reliable = false;
            }
            let text = &source[region.music_span.byte_start..region.music_span.byte_end];
            voices.push(IdentityVoice {
                raw: musical_tokens(&tokenize(text)),
                measures: region
                    .indexed_measures()
                    .map(|measure| (measure.measure_start, measure.measure_end))
                    .collect(),
            });
        }
    }
    MusicIdentity {
        reliable,
        staves,
        voices,
    }
}

fn musical_tokens(tokens: &[Token]) -> Vec<IdentityToken> {
    let mut result: Vec<IdentityToken> = Vec::new();
    let mut skip_until = None;
    for (position, token) in tokens.iter().enumerate() {
        if skip_until.is_some_and(|end| position < end) {
            continue;
        }
        skip_until = None;
        if matches!(token.kind, TokenKind::Ws | TokenKind::Comment) {
            continue;
        }
        if matches!(
            token.command(),
            Some("markup" | "layout" | "midi" | "paper" | "header")
        ) {
            while result
                .last()
                .is_some_and(|token| token.kind == TokenKind::Artic)
            {
                result.pop();
            }
            skip_until = expression_end(tokens, position + 1);
            continue;
        }
        result.push(IdentityToken {
            kind: token.kind,
            raw: token.raw.clone(),
        });
    }
    result
}

fn expression_end(tokens: &[Token], from: usize) -> Option<usize> {
    let (from, token) = tokens
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, token)| !matches!(token.kind, TokenKind::Ws | TokenKind::Comment))?;
    if token.kind == TokenKind::String {
        return Some(from + 1);
    }
    if token.raw != "{" {
        return Some(from + 1);
    }
    let mut depth = 0usize;
    for (position, token) in tokens.iter().enumerate().skip(from) {
        if token.raw == "{" {
            depth += 1;
        } else if token.raw == "}" {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(position + 1);
            }
        }
    }
    Some(tokens.len())
}
