use ossia_types::{AppError, SourcePosition, SourceSpan};

/// Maps offsets in unchanged UTF-8 source. Lines split only at LF; CR and BOM remain scalars.
pub struct SourceMap<'a> {
    source: &'a str,
    starts: Vec<usize>,
}
impl<'a> SourceMap<'a> {
    pub fn new(source: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(
                source
                    .bytes()
                    .enumerate()
                    .filter_map(|(i, b)| (b == b'\n').then_some(i + 1)),
            )
            .collect();
        Self { source, starts }
    }
    pub fn position(&self, byte: usize) -> Result<SourcePosition, AppError> {
        if !self.source.is_char_boundary(byte) {
            return Err(AppError::invalid(
                "offset is outside source or splits a UTF-8 scalar",
            ));
        }
        let line = self.starts.partition_point(|&s| s <= byte) - 1;
        Ok(SourcePosition {
            byte,
            line: line + 1,
            column: self.source[self.starts[line]..byte].chars().count() + 1,
        })
    }
    pub fn span(&self, start: usize, end: usize) -> Result<SourceSpan, AppError> {
        if start > end {
            return Err(AppError::invalid("reversed source span"));
        }
        Ok(SourceSpan {
            byte_start: start,
            byte_end: end,
            start: self.position(start)?,
            end: self.position(end)?,
        })
    }
    /// Validates both byte boundaries and their associated line/column metadata.
    pub fn slice(&self, span: SourceSpan) -> Result<&'a str, AppError> {
        if self.span(span.byte_start, span.byte_end)? != span {
            return Err(AppError::invalid("source span position mismatch"));
        }
        Ok(&self.source[span.byte_start..span.byte_end])
    }
    /// LilyPond SVG `char` is a zero-based scalar offset, not a byte or display column.
    pub fn anchor_byte(&self, line: usize, char_offset: usize) -> Option<usize> {
        let text = self.line(line)?;
        if char_offset == text.chars().count() {
            return Some(self.starts[line - 1] + text.len());
        }
        text.char_indices()
            .nth(char_offset)
            .map(|(i, _)| self.starts[line - 1] + i)
    }
    /// Probe-verified tab stops are eight columns; positions inside a tab are ambiguous.
    pub fn display_byte(&self, line: usize, column: usize) -> Option<usize> {
        let text = self.line(line)?;
        let mut col = 1usize;
        for (i, c) in text.char_indices() {
            if col == column {
                return Some(self.starts[line - 1] + i);
            }
            col += if c == '\t' { 8 - (col - 1) % 8 } else { 1 };
            if col > column {
                return None;
            }
        }
        (col == column).then_some(self.starts[line - 1] + text.len())
    }
    fn line(&self, line: usize) -> Option<&'a str> {
        let start = *self.starts.get(line.checked_sub(1)?)?;
        let end = self.starts.get(line).map_or(self.source.len(), |v| v - 1);
        Some(&self.source[start..end])
    }
}
