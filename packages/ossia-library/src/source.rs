use crate::{db, *};
use ossia_score::SourceMap;

impl Library {
    const DEFAULT_READ_LINES: usize = 200;
    const MAX_READ_LINES: usize = 1_000;
    const DEFAULT_READ_BYTES: usize = 32 * 1024;
    const MAX_READ_BYTES: usize = 256 * 1024;

    /// Read a bounded page from one exact public-source hash.
    pub fn read_source(&self, args: &ReadSourceArgs) -> Result<ReadSourceOutcome> {
        let snapshot = {
            let conn = self.db()?;
            let doc = db::document(&conn, args.document_id)?;
            self.expect_snapshot(&doc, &args.expected_source_hash)?
        };
        read_snapshot_page(&snapshot, args)
    }
}

fn read_snapshot_page(
    snapshot: &SourceSnapshot,
    args: &ReadSourceArgs,
) -> Result<ReadSourceOutcome> {
    let source = snapshot.source();
    let map = SourceMap::new(source);
    let total_bytes = source.len();
    let total_lines = source.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let max_lines = args.max_lines.unwrap_or(Library::DEFAULT_READ_LINES);
    let max_bytes = args.max_bytes.unwrap_or(Library::DEFAULT_READ_BYTES);
    if !(1..=Library::MAX_READ_LINES).contains(&max_lines) {
        return Err(AppError::invalid("maxLines must be 1..1000"));
    }
    if !(1..=Library::MAX_READ_BYTES).contains(&max_bytes) {
        return Err(AppError::invalid("maxBytes must be 1..262144"));
    }
    let (start, requested_end) = match args.start.as_ref().unwrap_or(&SourceReadStart::Start) {
        SourceReadStart::Start => (0, total_bytes),
        SourceReadStart::Line { line } => {
            let start = map
                .anchor_byte(*line as usize, 0)
                .ok_or_else(|| AppError::invalid("source read line is outside the file"))?;
            (start, total_bytes)
        }
        SourceReadStart::Span { span } => {
            map.slice(*span)?;
            (span.byte_start, span.byte_end)
        }
    };
    let mut end = requested_end;
    let mut seen_lines = 0;
    for (offset, byte) in source.as_bytes()[start..requested_end].iter().enumerate() {
        if *byte == b'\n' {
            seen_lines += 1;
            if seen_lines == max_lines {
                end = start + offset + 1;
                break;
            }
        }
    }
    end = end.min(start.saturating_add(max_bytes));
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    if end == start && start < requested_end {
        end = source[start..]
            .char_indices()
            .nth(1)
            .map_or(requested_end, |(offset, _)| start + offset);
    }
    let span = map.span(start, end)?;
    let next = (end < requested_end)
        .then(|| map.span(end, requested_end))
        .transpose()?;
    let line_spans = if args.include_line_spans {
        let mut spans = Vec::new();
        let mut cursor = start;
        while cursor < end {
            let line_end = source.as_bytes()[cursor..end]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(end, |offset| cursor + offset + 1);
            spans.push(map.span(cursor, line_end)?);
            cursor = line_end;
        }
        spans
    } else {
        Vec::new()
    };
    Ok(ReadSourceOutcome {
        document_id: snapshot.document_id(),
        source_hash: snapshot.source_hash().clone(),
        content: source[start..end].to_owned(),
        span,
        next,
        complete: next.is_none(),
        returned_bytes: end - start,
        total_bytes,
        total_lines,
        line_spans,
    })
}
