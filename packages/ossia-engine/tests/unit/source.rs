use crate::{source, *};

#[test]
fn derived_text_respects_comments_strings_scheme_and_actual_score_boundaries() {
    let text = r#"\pointAndClickOff
%{ \score { fake } %{ nested %} %}
name = "\\score { escaped \" quote"
#(display "}")
\score { { c'1 } \layout {} }
\header { title = "tail" }"#;
    let derived = source::derive(text, RenderFormat::Midi, true).unwrap();
    assert_eq!(derived.text.matches("\\midi {}").count(), 1);
    assert!(derived.text.find("\\midi {}").unwrap() < derived.text.find("\\header").unwrap());
    for (byte, ch) in text.char_indices() {
        if let Some(mapped) = derived
            .text
            .char_indices()
            .find_map(|(d, _)| (derived.original_byte(d) == Some(byte)).then_some(d))
        {
            if byte >= "\\pointAndClickOff".len() {
                assert_eq!(derived.text[mapped..].chars().next(), Some(ch));
            }
        }
    }
    assert_eq!(text.matches("\\midi").count(), 0);
    for opaque in [
        "{ c'1 }",
        "\\score { c'1 } \\score { d'1 }",
        "\\score \\Music",
        "\\score { c'1",
        "\\score { #{ c #} }",
    ] {
        assert_eq!(
            source::derive(opaque, RenderFormat::Midi, true)
                .unwrap_err()
                .code,
            ErrorCode::UnsupportedSource
        );
    }
    let active = "\\score { { c'1 } \\midi { \\tempo 4 = 123 } }";
    assert_eq!(
        source::derive(active, RenderFormat::Midi, true)
            .unwrap()
            .text,
        active
    );
}

#[test]
fn svg_links_map_original_unicode_and_never_map_library_files() {
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
    let svg = format!("<svg xmlns:xlink='http://www.w3.org/1999/xlink'><a xlink:href='textedit:///private/tmp/job/score.ly:{}:{}:{}'><text>A &amp; B</text></a><a xlink:href='textedit:///engine/english.ly:1:0:1'/></svg>",p.line,p.column-1,column);
    let normalized = source::normalize_svg(&svg, &derived, original).unwrap();
    assert!(normalized.contains(&format!(
        "textedit://score.ly:{}:{}:{}",
        p.line,
        p.column - 1,
        column
    )));
    assert!(!normalized.contains("/private/tmp") && !normalized.contains("english.ly"));
}
