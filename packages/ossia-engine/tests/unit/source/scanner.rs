use super::*;

#[test]
fn embedded_music_ignores_literal_delimiters_and_nested_scheme() {
    let source = r##"#(list
      #{ \markup { "中文 #} ) \\\"" % #} )
         %{ #} %{ nested %} %}
         #(list #{ \markup "inner" #} ")")
       } #})
    \score { c'1 }
    "##;
    let tokens = scan(source).unwrap();
    assert!(tokens[0].raw.starts_with("#(list"));
    assert_eq!(tokens[1].raw, "\\score");
    assert_eq!(
        tokens.iter().filter(|token| token.raw == "\\score").count(),
        1
    );
}

#[test]
fn incomplete_or_opaque_readers_cannot_supply_edit_boundaries() {
    for source in [
        "#(list #{ \\markup A)",
        "#(list #{ \\markup A #}",
        "#(list #| comment ) |#)",
        "#(list #\\))",
        "#(list #{ \\include \"external.ly\" #})",
        "#(list #{ \\score { c1 } #})",
        "#(list #{ \\pointAndClickOff #})",
    ] {
        assert!(scan(source).is_err(), "{source}");
    }
    let deep = format!("{}A{}", "#(list #{ ".repeat(65), " #})".repeat(65));
    assert!(scan(&deep).is_err());
}
