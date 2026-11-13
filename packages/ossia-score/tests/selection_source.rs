use ossia_score::*;

#[test]
fn header_values_and_commands_keep_complete_unicode_source_ranges() {
    let source = "\u{feff}\\header { title = \\markup { \\bold \"中文\" } composer = \"作者\" copyright = ##f }\r\nMusic = { \\clef bass \\time 3/4 c4 | } \\score { \\new Staff \\Music }";
    let fields = source_elements(source);
    let map = SourceMap::new(source);
    for (kind, text) in [
        ("header:title", "\\markup { \\bold \"中文\" }"),
        ("header:composer", "\"作者\""),
        ("Clef", "\\clef bass"),
        ("TimeSignature", "\\time 3/4"),
    ] {
        assert_eq!(
            map.slice(fields.iter().find(|e| e.kind == kind).unwrap().span)
                .unwrap(),
            text
        );
    }
    assert!(!fields.iter().any(|e| e.kind == "header:copyright"));
    let index = build_index(source);
    let anchor = property_insertion_anchor(&index, 0).unwrap();
    assert_eq!(&source[anchor.byte_start - 1..anchor.byte_start], "{");
}

#[test]
fn unchanged_elements_remap_but_deleted_and_ambiguous_ones_do_not() {
    let old = "Music = { c'4 d'4 e'4 f'4 | }";
    let at = old.find("d'4").unwrap();
    let span = SourceMap::new(old).span(at, at + 3).unwrap();
    let new = format!("% annotation\n{old}");
    let remapped = remap_selection_span(old, &new, span).unwrap();
    assert_eq!(SourceMap::new(&new).slice(remapped).unwrap(), "d'4");
    assert!(remap_selection_span(old, &old.replace("d'4", "g'4"), span).is_none());
    let duplicate = old.replace("d'4", "d'4 d'4");
    assert!(remap_selection_span(old, &duplicate, span).is_none());
    let unrelated = old.replace("d'4", "g'4") + " Other = { d'4 }";
    assert!(remap_selection_span(old, &unrelated, span).is_none());
}

#[test]
fn spanning_marks_resolve_both_ends_and_default_marks_do_not_consume_music() {
    let source = r#"Music = { c'4( d'4) e'4\< f'4\! | \mark \default g'1 | }"#;
    let map = SourceMap::new(source);
    for (kind, start, expected) in [
        ("Slur", "(", "( d'4)"),
        ("Hairpin", "\\<", "\\< f'4\\!"),
        ("RehearsalMark", "\\mark", "\\mark \\default"),
    ] {
        let span = element_source_span(source, source.find(start).unwrap(), kind).unwrap();
        assert_eq!(map.slice(span).unwrap(), expected);
    }
}

#[test]
fn generated_properties_have_a_staff_anchor_when_the_staff_has_multiple_voices() {
    let source = r#"One = { c'1 | } Two = { e'1 | } \score { \new Staff << \context Voice = "one" \One \context Voice = "two" \Two >> }"#;
    let index = build_index(source);
    assert_eq!(index.staves[0].voices.len(), 2);
    let anchor = property_insertion_anchor(&index, 0).unwrap();
    assert_eq!(&source[anchor.byte_start - 2..anchor.byte_start], "<<");
}

#[test]
fn neutral_text_attachments_and_multiplied_rests_are_complete_expressions() {
    let source = r#"Music = { c'1-\markup { "dolce" } | R1*4 | }"#;
    let map = SourceMap::new(source);
    for (kind, start, expected) in [
        ("TextScript", "-\\markup", "-\\markup { \"dolce\" }"),
        ("MultiMeasureRest", "R1", "R1*4"),
    ] {
        let span = element_source_span(source, source.find(start).unwrap(), kind).unwrap();
        assert_eq!(map.slice(span).unwrap(), expected);
    }
}
