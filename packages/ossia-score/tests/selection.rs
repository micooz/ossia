use ossia_score::*;

#[test]
fn adjacent_dynamics_articulations_and_hairpins_keep_the_measure_index() {
    let source = r#"Music = { c'4\p-. d'4\< e'4 f'4 | g'1\! | } \score { \new Staff \Music }"#;
    let tokens = tokenize(source);
    for command in ["\\p", "\\<", "\\!"] {
        assert!(tokens.iter().any(|t| t.raw == command), "{tokens:?}");
    }
    assert_eq!(build_index(source).capability, LocationCapability::Measure);
}

#[test]
fn measure_selection_excludes_prefix_symbols_and_stays_inside_barlines() {
    let source = "Music = { c'1 | d'1 | } \\score { \\new Staff \\Music }";
    let note = source.find("c'1").unwrap();
    let svg = format!(
        r#"<svg viewBox="0 0 100 60">
      <g><line x1="5" x2="90" y1="20" y2="20"/><line x1="5" x2="90" y1="21" y2="21"/>
      <line x1="5" x2="90" y1="22" y2="22"/><line x1="5" x2="90" y1="23" y2="23"/>
      <line x1="5" x2="90" y1="24" y2="24"/></g>
      <g data-ossia-kind="Clef" data-ossia-bounds="7 18 11 26"/>
      <g data-ossia-kind="TimeSignature" data-ossia-bounds="13 20 16 24"/>
      <g data-ossia-kind="NoteHead" data-ossia-bounds="20 21 22 23" data-ossia-source="{note}:{}"/>
      <rect x="49.9" y="20" width="0.2" height="4"/>
      <rect x="89.8" y="20" width="0.2" height="4"/>
      <a href="textedit://score.ly:1:{note}:0"><path d="M20 21 L22 23"/></a>
    </svg>"#,
        note + 3
    );
    let index = build_index(source);
    let layout = build_layout(&index, &[SvgPage { page: 1, svg: &svg }]).unwrap();
    let geometry = selection_geometry(&index, &layout).unwrap();
    let first = &geometry[0].regions[0];
    assert!(first.bounds.x0 > 16. && first.bounds.x0 < 20., "{first:?}");
    assert!((first.bounds.x1 - 49.9).abs() < 1e-8, "{first:?}");
    let second = &geometry[0].regions[1];
    assert!((second.bounds.x0 - 50.1).abs() < 1e-8, "{second:?}");
    assert!((second.bounds.x1 - 89.8).abs() < 1e-8, "{second:?}");
}

#[test]
fn svg_element_bounds_follow_visible_ink_and_the_svg_y_axis() {
    let source = "Music = { c'1 } \\score { \\new Staff \\Music }";
    let svg = r#"<svg viewBox="0 0 100 60">
      <g transform="translate(10, 30)">
        <a data-ossia-kind="Clef" data-ossia-id="clef" data-ossia-source="0:1">
          <rect x="0" y="-2" width="2" height="4" fill="none"/>
        </a>
      </g>
      <g data-ossia-graphic="clef">
        <g transform="translate(10, 30)"><path d="M0 -4 L2 2" fill="black"/></g>
      </g>
      <g transform="translate(20, 30)">
        <a data-ossia-kind="Tie" data-ossia-id="tie" data-ossia-owners="a,b" data-ossia-hit="true">
          <rect x="0" y="1" width="3" height="1" fill="none"/>
        </a>
      </g>
      <g data-ossia-graphic="tie">
        <g transform="translate(20, 30)"><path d="M0 -2 L3 -1" fill="none"/></g>
      </g>
    </svg>"#;
    let index = build_index(source);
    let layout = build_layout(&index, &[SvgPage { page: 1, svg }]).unwrap();
    let page = &layout.pages[0];
    let clef = page
        .objects
        .iter()
        .find(|object| object.kind == "Clef")
        .unwrap();
    assert_eq!(
        clef.bbox,
        BBox {
            x0: 10.,
            y0: 26.,
            x1: 12.,
            y1: 32.
        }
    );
    let hit = page
        .objects
        .iter()
        .find(|object| object.kind == "Tie")
        .unwrap();
    assert_eq!(
        hit.bbox,
        BBox {
            x0: 20.,
            y0: 28.,
            x1: 23.,
            y1: 29.
        }
    );
}
