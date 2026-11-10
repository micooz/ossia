use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenKind {
    Ws,
    Comment,
    Scheme,
    Cmd,
    String,
    Seq,
    Brace,
    ChordC,
    ChordO,
    Bar,
    Beam,
    Tie,
    Fing,
    Num,
    Slash,
    Star,
    Artic,
    Note,
    Rest,
    Skip,
    Ident,
    Punct,
    Raw,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Top,
    Music,
    Text,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub kind: TokenKind,
    pub raw: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub mode: Mode,
}
impl Token {
    pub(crate) fn trivia(&self) -> bool {
        matches!(self.kind, TokenKind::Ws | TokenKind::Comment)
    }
    pub(crate) fn command(&self) -> Option<&str> {
        (self.kind == TokenKind::Cmd).then(|| &self.raw[1..])
    }
}

fn rules() -> &'static Vec<(TokenKind, Regex)> {
    static RULES: OnceLock<Vec<(TokenKind, Regex)>> = OnceLock::new();
    RULES.get_or_init(|| {
        use TokenKind::*;
        [
            (Ws, r"[ \t\r\n\x0c]+"),
            (Comment, r"%[^\n]*"),
            (String, r#""(?:[^"\\]|\\.)*""#),
            (Cmd, r#"\\(?:[a-zA-Z-]+|[(){}\[\]|%;!"/\\])"#),
            (Seq, r"<<|>>"),
            (ChordC, r">(?:\d+(?:\.\d*)?\.{0,2})?"),
            (ChordO, r"<"),
            (Bar, r"\|"),
            (Beam, r"[\[\]]"),
            (Tie, r"~"),
            (Fing, r"-(?:[0-9]+|\\[a-zA-Z-]+|->|\.)"),
            (
                Num,
                r"(?:[0-9]+(?:\.[0-9]*)?\s*/\s*[0-9]+|[0-9]+(?:\.[0-9]*)?)",
            ),
            (Slash, r"/"),
            (Star, r"\*"),
            (Punct, r"="),
            (Artic, r"[!^_.:>\-]"),
            (Brace, r"[{}]"),
        ]
        .into_iter()
        .map(|(kind, p)| {
            (
                kind,
                Regex::new(&format!("^(?:{p})")).expect("static lexer rule"),
            )
        })
        .collect()
    })
}
fn word() -> &'static Regex {
    static WORD: OnceLock<Regex> = OnceLock::new();
    WORD.get_or_init(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9_.'!,]*").unwrap())
}
fn pitchish(raw: &str, chord: bool) -> bool {
    static PITCH: OnceLock<Regex> = OnceLock::new();
    static BARE: OnceLock<Regex> = OnceLock::new();
    let p = PITCH.get_or_init(|| Regex::new(r"^(?:[a-h](?:isis|eses|ses|is|es|ss|ff|s|f)?[',]*!?(?:\d+(?:\.\d*)?\.{0,2})?|[rR][',]*\d+(?:\.\d*)?\.{0,2}|s\d+(?:\.\d*)?\.{0,2})$").unwrap());
    let b = BARE
        .get_or_init(|| Regex::new(r"^[a-h](?:isis|eses|ses|is|es|ss|ff|s|f)?[',]*!?$").unwrap());
    p.is_match(raw) || (chord && b.is_match(raw))
}
fn text_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "header"
            | "paper"
            | "layout"
            | "context"
            | "midi"
            | "markup"
            | "concat"
            | "fontsize"
            | "smaller"
            | "larger"
            | "normal-text"
            | "rhythm"
            | "override"
            | "revert"
            | "set"
            | "char"
            | "chordmode"
            | "column-line"
            | "center-column"
            | "general-column"
            | "bold"
            | "italic"
            | "medium"
            | "huge"
            | "raise"
            | "translate"
            | "scale"
            | "with-color"
            | "abs-fontsize"
            | "path"
            | "circle"
            | "musicglyph"
            | "bracket"
            | "box"
            | "note"
            | "center-align"
            | "left-align"
            | "right-align"
            | "size"
            | "sharp"
    )
}
fn argument_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "key"
            | "time"
            | "meter"
            | "tie"
            | "slur"
            | "text"
            | "markup"
            | "override"
            | "revert"
            | "set"
            | "change"
            | "context"
            | "numericTimeSignature"
            | "relative"
            | "clef"
            | "ottava"
            | "partial"
            | "repeat"
            | "unfold"
            | "skip"
            | "tempo"
            | "barNumberCheck"
            | "setStaffIdentNumbering"
            | "transposition"
    )
}

/// Scan balanced Scheme and block comments iteratively so deep input cannot exhaust the stack.
fn opaque_len(s: &str) -> Option<(TokenKind, usize)> {
    if s.starts_with("%{") {
        let mut depth = 1usize;
        let mut i = 2;
        while i < s.len() {
            if s[i..].starts_with("%{") {
                depth += 1;
                i += 2;
            } else if s[i..].starts_with("%}") {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    break;
                }
            } else {
                i += s[i..].chars().next()?.len_utf8();
            }
        }
        return Some((TokenKind::Comment, i));
    }
    let bytes = s.as_bytes();
    let prefix = if s.starts_with("#'(") {
        3
    } else if s.starts_with("#(") || s.starts_with("$(") {
        2
    } else {
        0
    };
    if prefix > 0 || s.starts_with("#{") {
        let lily = s.starts_with("#{");
        let mut depth = 1usize;
        let mut quoted = false;
        let mut i = if lily { 2 } else { prefix };
        while i < bytes.len() {
            if bytes[i] == b'\\' && quoted {
                i = (i + 2).min(bytes.len());
                continue;
            }
            if bytes[i] == b'"' {
                quoted = !quoted;
                i += 1;
                continue;
            }
            if !quoted {
                if !lily && bytes[i] == b';' {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                if lily && s.get(i..).is_some_and(|s| s.starts_with("#{")) {
                    depth += 1;
                    i += 2;
                    continue;
                }
                if lily && s.get(i..).is_some_and(|s| s.starts_with("#}")) {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                if !lily && bytes[i] == b'(' {
                    depth += 1;
                }
                if !lily && bytes[i] == b')' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
            }
            i += 1;
        }
        return Some((TokenKind::Scheme, i));
    }
    if s.starts_with('#') || s.starts_with('$') {
        static SCHEME: OnceLock<Regex> = OnceLock::new();
        let re = SCHEME.get_or_init(|| Regex::new(r##"^[#$](?:#[a-zA-Z][a-zA-Z0-9_-]*|-?(?:[0-9]+(?:\.[0-9]*)?|[a-z]+)|[a-zA-Z][a-zA-Z0-9_-]*|"(?:[^"\\]|\\.)*")"##).unwrap());
        return re.find(s).map(|m| (TokenKind::Scheme, m.end()));
    }
    None
}

/// Tokenize any UTF-8 string without dropping or normalizing a byte. Unknown scalars are RAW.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut stack = Vec::new();
    let mut mode = Mode::Top;
    let mut last_cmd = "";
    let mut arg_pending = false;
    let mut chord = 0usize;
    let mut i = 0;
    while i < source.len() {
        let s = &source[i..];
        let hit = opaque_len(s).or_else(|| {
            rules()
                .iter()
                .find_map(|(k, r)| r.find(s).map(|m| (*k, m.end())))
        });
        let (kind, len) = if let Some(hit) = hit {
            hit
        } else if let Some(m) = word().find(s) {
            let raw = m.as_str();
            let kind = if mode == Mode::Music && !arg_pending && pitchish(raw, chord > 0) {
                match raw.as_bytes()[0].to_ascii_lowercase() {
                    b'r' => TokenKind::Rest,
                    b's' => TokenKind::Skip,
                    _ => TokenKind::Note,
                }
            } else {
                TokenKind::Ident
            };
            arg_pending = false;
            (kind, m.end())
        } else {
            (TokenKind::Raw, s.chars().next().unwrap().len_utf8())
        };
        let raw = &source[i..i + len];
        tokens.push(Token {
            kind,
            raw: raw.into(),
            byte_start: i,
            byte_end: i + len,
            mode,
        });
        if kind == TokenKind::Cmd {
            last_cmd = &source[i + 1..i + len];
            arg_pending = argument_command(last_cmd);
        } else {
            if !matches!(kind, TokenKind::Ws | TokenKind::Comment) {
                arg_pending = false;
            }
            if raw == "{" || raw == "<<" {
                stack.push(mode);
                mode = if raw == "<<" {
                    Mode::Music
                } else if text_command(last_cmd) || mode == Mode::Text {
                    Mode::Text
                } else {
                    Mode::Music
                };
            } else if raw == "}" || raw == ">>" {
                mode = stack.pop().unwrap_or(Mode::Top);
            }
            if kind == TokenKind::ChordO {
                chord += 1;
            }
            if kind == TokenKind::ChordC {
                chord = chord.saturating_sub(1);
            }
        }
        i += len;
    }
    tokens
}

pub fn serialize(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.raw.as_str()).collect()
}
