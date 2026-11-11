use ossia_score::{build_index, serialize, tokenize, TokenKind};

#[test]
fn text_blocks_do_not_hide_subsequent_music() {
    for prefix in [
        r#"\header { title = "中文" subtitle = \markup { \bold "音阶" } }"#,
        r"\layout { }",
        r"\paper { }",
    ] {
        for score in [r"\new Staff \Music", r"\new Staff { \new Voice \Music }"] {
            let source = format!("\u{feff}{prefix}\r\nUnused = {{ r1 | }}\r\nMusic = {{ \\clef treble \\time 4/4 c4 d e f | g4 a b c' | }}\r\n\\score {{ {score} \\layout {{ }} }}");
            let tokens = tokenize(&source);
            assert_eq!(serialize(&tokens), source);
            assert_eq!(
                tokens.iter().filter(|t| t.kind == TokenKind::Note).count(),
                8
            );
            let index = build_index(&source);
            assert_eq!(index.measure_count(), Ok(2), "{source}");
            assert_eq!(index.staves[0].voices, vec!["Music"]);
        }
        let source = format!("{prefix} \\score {{ \\new Staff {{ c'1 | }} \\layout {{ }} }}");
        assert_eq!(build_index(&source).measure_count(), Ok(1));
    }
}
