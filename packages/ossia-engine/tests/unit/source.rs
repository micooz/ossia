use crate::{source, *};

#[test]
fn musicxml_conversion_activates_only_the_generated_tempo_block() {
    let converted = "\\score { { c'1 }\n  % To create MIDI output, uncomment the following line:\n  % \\midi { \\tempo 4 = 96.5 }\n}\n";
    let expected = "\\score { { c'1 }\n  \\midi { \\tempo 4 = 96.5 }\n}\n";
    assert_eq!(
        source::normalize_conversion(converted, Path::new("/scratch"), InputFormat::MusicXml)
            .unwrap(),
        expected
    );
    assert_eq!(
        source::normalize_conversion(converted, Path::new("/scratch"), InputFormat::Midi).unwrap(),
        converted
    );
    for unexpected in ["\\score { c'1 }".to_owned(), converted.repeat(2)] {
        assert!(source::normalize_conversion(
            &unexpected,
            Path::new("/scratch"),
            InputFormat::MusicXml
        )
        .is_err());
    }
}

#[test]
fn converter_markup_does_not_block_anchors_or_midi() {
    let text = r#"\pointAndClickOff
#(define-markup-command (square layout props arg)
   (markup?)
   (interpret-markup layout props
     #{ \markup \box \with-dimension-from #X \rotate #90 #arg #arg #}))
\score { { c'1 } \layout {} }
"#;
    for format in [RenderFormat::Svg, RenderFormat::Pdf, RenderFormat::Midi] {
        let derived = source::derive(text, format, true).unwrap();
        assert!(!derived.text.contains("\\pointAndClickOff"));
        let note = derived.text.find("c'1").unwrap();
        assert_eq!(derived.original_byte(note), text.find("c'1"));
        assert!(derived.text.contains("#{ \\markup \\box"));
        if format == RenderFormat::Midi {
            assert_eq!(derived.text.matches("\\midi {}").count(), 1);
        }
    }
}

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

#[test]
fn print_derivation_hides_only_the_default_tagline() {
    let without_tagline =
        "\\version \"2.26.0\"\n\\header { title = \"Scale\" }\n\\score { { c'1 } \\layout {} }\n";
    let derived = source::derive(without_tagline, RenderFormat::Svg, false).unwrap();
    assert_eq!(
        derived.text,
        format!("{without_tagline}\n\\paper {{ tagline = ##f }}\n")
    );

    let explicit =
        "\\version \"2.26.0\"\n\\header { tagline = \"Ossia\" }\n\\score { { c'1 } \\layout {} }\n";
    assert_eq!(
        source::derive(explicit, RenderFormat::Pdf, true)
            .unwrap()
            .text,
        explicit
    );
    let midi = source::derive(without_tagline, RenderFormat::Midi, true).unwrap();
    assert!(!midi.text.contains("tagline"));
    assert!(midi.text.contains("\\midi {}"));
}
