use super::Lexeme;
use ossia_types::AppError;

#[cfg(test)]
#[path = "../../tests/unit/source/scanner.rs"]
mod tests;

#[derive(Debug)]
pub(super) enum ScanError {
    Opaque(&'static str),
    RestrictedCommand(String),
}

impl ScanError {
    pub(super) fn into_app_error(self) -> AppError {
        match self {
            Self::Opaque(message) => AppError::unsupported(message),
            Self::RestrictedCommand(command) => {
                AppError::unsupported(format!("unsupported command in quoted music: {command}"))
            }
        }
    }
}

// Only structural boundaries are read; Scheme and quoted music remain single tokens.
pub(super) fn scan(source: &str) -> Result<Vec<Lexeme>, ScanError> {
    let mut scanner = Scanner { source, byte: 0 };
    let mut tokens = Vec::new();
    while let Some(token) = scanner.next(0)? {
        tokens.push(token);
    }
    Ok(tokens)
}

struct Scanner<'a> {
    source: &'a str,
    byte: usize,
}

impl Scanner<'_> {
    fn starts(&self, value: &str) -> bool {
        self.source.as_bytes()[self.byte..].starts_with(value.as_bytes())
    }

    fn line_comment(&mut self) {
        while self.byte < self.source.len() && self.source.as_bytes()[self.byte] != b'\n' {
            self.byte += 1;
        }
    }

    fn string(&mut self) -> Result<(), ScanError> {
        self.byte += 1;
        while self.byte < self.source.len() {
            match self.source.as_bytes()[self.byte] {
                b'"' => {
                    self.byte += 1;
                    return Ok(());
                }
                b'\\' => self.byte += 2,
                _ => self.byte += 1,
            }
        }
        Err(ScanError::Opaque("unterminated source string"))
    }

    fn block_comment(&mut self) -> Result<(), ScanError> {
        self.byte += 2;
        let mut depth = 1;
        while self.byte < self.source.len() {
            if self.starts("%{") {
                depth += 1;
                self.byte += 2;
            } else if self.starts("%}") {
                depth -= 1;
                self.byte += 2;
                if depth == 0 {
                    return Ok(());
                }
            } else {
                self.byte += 1;
            }
        }
        Err(ScanError::Opaque("unterminated block comment"))
    }

    fn scheme(&mut self, nesting: usize) -> Result<(), ScanError> {
        self.byte += 2;
        let mut depth = 1;
        while self.byte < self.source.len() {
            match self.source.as_bytes()[self.byte] {
                b'"' => self.string()?,
                b';' => self.line_comment(),
                b'(' => {
                    depth += 1;
                    self.byte += 1;
                }
                b')' => {
                    depth -= 1;
                    self.byte += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                b'#' if self.starts("#{") => self.quoted_music(nesting + 1)?,
                b'#' if self.starts("#|") || self.starts("#\\") => {
                    return Err(ScanError::Opaque(
                        "unsupported Scheme reader in structural scan",
                    ));
                }
                _ => self.byte += 1,
            }
        }
        Err(ScanError::Opaque("unterminated Scheme expression"))
    }

    fn quoted_music(&mut self, nesting: usize) -> Result<(), ScanError> {
        if nesting > 64 {
            return Err(ScanError::Opaque(
                "source nesting exceeds structural scan limit",
            ));
        }
        self.byte += 2;
        while let Some(token) = self.next(nesting)? {
            // Quoted dependencies and output blocks require more than an outer structural scan.
            if matches!(
                token.raw.as_str(),
                "\\include"
                    | "\\epsfile"
                    | "\\image"
                    | "\\include-string"
                    | "\\verbatim-file"
                    | "\\score"
                    | "\\book"
                    | "\\bookpart"
                    | "\\bookOutputName"
                    | "\\bookOutputSuffix"
                    | "\\pointAndClickOff"
                    | "\\pointAndClickOn"
            ) {
                return Err(ScanError::RestrictedCommand(token.raw));
            }
        }
        if self.starts("#}") {
            self.byte += 2;
            return Ok(());
        }
        Err(ScanError::Opaque("unterminated quoted music"))
    }

    fn next(&mut self, nesting: usize) -> Result<Option<Lexeme>, ScanError> {
        loop {
            if self.byte >= self.source.len() {
                return Ok(None);
            }
            if nesting > 0 && self.starts("#}") {
                return Ok(None);
            }
            let start = self.byte;
            let bytes = self.source.as_bytes();
            match bytes[self.byte] {
                b'%' if self.starts("%{") => {
                    self.block_comment()?;
                    continue;
                }
                b'%' => {
                    self.line_comment();
                    continue;
                }
                b'"' => self.string()?,
                b'#' | b'$' if bytes.get(self.byte + 1) == Some(&b'(') => {
                    self.scheme(nesting)?;
                }
                b'#' if self.starts("#{") => {
                    if nesting == 0 {
                        return Err(ScanError::Opaque("top-level quoted music is opaque"));
                    }
                    self.quoted_music(nesting + 1)?;
                }
                b'\\' => {
                    self.byte += 1;
                    while self.byte < bytes.len()
                        && (bytes[self.byte].is_ascii_alphabetic() || bytes[self.byte] == b'-')
                    {
                        self.byte += 1;
                    }
                    if self.byte == start + 1 && self.byte < bytes.len() {
                        self.byte += self.source[self.byte..].chars().next().unwrap().len_utf8();
                    }
                }
                b'{' | b'}' => self.byte += 1,
                x if x.is_ascii_whitespace() => {
                    self.byte += 1;
                    continue;
                }
                b'#' | b'$'
                    if matches!(
                        bytes.get(self.byte + 1),
                        Some(b'\'') | Some(b'`') | Some(b'\\')
                    ) =>
                {
                    return Err(ScanError::Opaque("opaque Scheme reader prefix"));
                }
                _ => {
                    self.byte += self.source[self.byte..].chars().next().unwrap().len_utf8();
                    while self.byte < bytes.len()
                        && !bytes[self.byte].is_ascii_whitespace()
                        && !matches!(
                            bytes[self.byte],
                            b'\\' | b'{' | b'}' | b'%' | b'"' | b'#' | b'$'
                        )
                    {
                        self.byte += self.source[self.byte..].chars().next().unwrap().len_utf8();
                    }
                }
            }
            return Ok(Some(Lexeme {
                raw: self.source[start..self.byte].into(),
                range: start..self.byte,
            }));
        }
    }
}
