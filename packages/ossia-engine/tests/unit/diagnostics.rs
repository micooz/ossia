use crate::{diagnostics, source, *};
use std::path::Path;

#[test]
fn diagnostics_map_original_unicode_and_redact_library_files() {
    let original = "\\pointAndClickOff\r\n\\score { { \\mark \\markup { \"中文😀\" }\tc'1 } }\r\n";
    let derived = source::derive(original, RenderFormat::Midi, true).unwrap();
    let byte = derived.text.find("c'1").unwrap();
    let p = ossia_score::SourceMap::new(&derived.text)
        .position(byte)
        .unwrap();
    let start = derived.text[..byte].rfind('\n').unwrap() + 1;
    let column = derived.text[start..byte].chars().fold(1, |n, c| {
        if c == '\t' {
            n + 8 - (n - 1) % 8
        } else {
            n + 1
        }
    });
    let diagnostics = diagnostics::parse(
        &format!(
            "score.ly:{}:{}: error: bad\n/engine/english.ly:1:1: warning: standard",
            p.line, column
        ),
        Path::new("/private/tmp/job"),
        Path::new("/engine"),
        Some(&derived),
        Some(original),
    );
    assert_eq!(
        diagnostics[0].position.unwrap().byte,
        original.find("c'1").unwrap()
    );
    assert_eq!(diagnostics[1].position, None);
    assert!(diagnostics::redact(
        "C:\\Users\\somebody\\file.ly /Users/person/private/file.ly",
        Path::new("/scratch"),
        Path::new("/engine")
    )
    .contains("<path>"));
}
