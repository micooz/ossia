use ossia_engine::*;
use ossia_score::{build_index, build_layout, expand_selection, selection_geometry, SvgPage};
use std::{collections::BTreeSet, fs, path::PathBuf};

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn note_ink_ownership_ties_graces_and_chords_are_exact() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let source = include_str!("../../../tests/fixtures/inputs/selection-ink.ly");
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                point_and_click: true,
            },
            &work.path().join("artifacts"),
            &RunContext::default(),
        )
        .unwrap();
    assert!(
        render.format_errors.is_empty(),
        "{:?}",
        render.format_errors
    );
    let plain = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                point_and_click: false,
            },
            &work.path().join("plain"),
            &RunContext::default(),
        )
        .unwrap();
    assert!(plain.format_errors.is_empty(), "{:?}", plain.format_errors);
    assert_eq!(
        plain.midi.as_ref().unwrap().sha256,
        render.midi.as_ref().unwrap().sha256
    );
    assert_eq!(plain.pages.len(), render.pages.len());
    for (static_page, interactive_page) in plain.pages.iter().zip(&render.pages) {
        let static_svg =
            fs::read(work.path().join("plain").join(&static_page.artifact.path)).unwrap();
        let interactive_svg = fs::read(
            work.path()
                .join("artifacts")
                .join(&interactive_page.artifact.path),
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&static_svg).contains("data-ossia-graphic"));
        let pixels = |svg: &[u8]| {
            engine
                .rasterize_svg(
                    svg,
                    1200,
                    &work.path().join("pixels"),
                    &RunContext::default(),
                )
                .unwrap()
                .sha256
        };
        assert_eq!(
            pixels(&static_svg),
            pixels(&interactive_svg),
            "interaction must preserve page pixels"
        );
    }
    let svgs: Vec<_> = render
        .pages
        .iter()
        .map(|p| {
            (
                p.page,
                fs::read_to_string(work.path().join("artifacts").join(&p.artifact.path)).unwrap(),
            )
        })
        .collect();
    assert!(svgs.len() > 1);
    for (_, svg) in &svgs {
        assert!(svg.contains("data-ossia-graphic="));
    }
    let index = build_index(source);
    let layout = build_layout(
        &index,
        &svgs
            .iter()
            .map(|(page, svg)| SvgPage { page: *page, svg })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let pages = selection_geometry(&index, &layout).unwrap();
    let elements: Vec<_> = pages.iter().flat_map(|p| &p.elements).collect();
    let text = |e: &&ossia_types::GeometryElement| {
        e.region
            .spans
            .first()
            .map(|s| &source[s.byte_start..s.byte_end])
            .unwrap_or("")
    };
    let c = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "c'")
        .unwrap();
    let e = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "e'")
        .unwrap();
    let d = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "d'8")
        .unwrap();
    let grace = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "b16")
        .unwrap();
    assert!(c.linked_targets.contains(&grace.region.target));
    assert!(
        !c.linked_targets.contains(&e.region.target),
        "grace ownership must not merge chord pitches"
    );
    assert!(
        !c.linked_targets.contains(&d.region.target),
        "beams must not merge neighboring notes"
    );
    let object_kinds = |ids: &[String]| {
        layout
            .pages
            .iter()
            .flat_map(|p| &p.objects)
            .filter(|o| o.graphic_id.as_ref().is_some_and(|id| ids.contains(id)))
            .map(|o| o.kind.as_str())
            .collect::<BTreeSet<_>>()
    };
    let kinds = object_kinds(&c.graphic_ids);
    for kind in ["NoteHead", "Stem", "Beam", "Script", "Fingering"] {
        assert!(kinds.contains(kind), "missing {kind}: {kinds:?}");
    }
    let grace_kinds = object_kinds(&grace.graphic_ids);
    assert!(grace_kinds.contains("Flag"), "{grace_kinds:?}");
    let ees = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "ees'8.")
        .unwrap();
    for kind in ["Dots", "Accidental"] {
        assert!(object_kinds(&ees.graphic_ids).contains(kind));
    }
    let grace_mark = elements
        .iter()
        .find(|e| {
            e.region.element_kind.as_deref() == Some("Script")
                && e.region.spans[0].byte_start < c.region.spans[0].byte_start
        })
        .unwrap();
    assert!(c.linked_targets.contains(&grace_mark.region.target));
    for page in &pages {
        let raw = layout.pages.iter().find(|p| p.page == page.page).unwrap();
        for region in &page.regions {
            let line = raw
                .objects
                .iter()
                .find(|o| {
                    o.kind == "BarLine"
                        && o.bbox.y0 <= region.bounds.y0 + 0.1
                        && o.bbox.y1 >= region.bounds.y1 - 0.1
                        && (o.bbox.x0 - region.bounds.x1).abs() < 1.
                })
                .unwrap();
            assert!(
                (line.bbox.x0 - region.bounds.x1).abs() < 1e-6,
                "measure must meet the line's inner edge"
            );
        }
    }
    let after_main = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "a'2")
        .unwrap();
    let after_grace = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "b'16")
        .unwrap();
    let following = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "c''2")
        .unwrap();
    assert!(
        after_main
            .linked_targets
            .contains(&after_grace.region.target),
        "afterGrace belongs to its preceding main note"
    );
    assert!(!following
        .linked_targets
        .contains(&after_grace.region.target));
    let fingered = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "d''")
        .unwrap();
    let other_fingered = elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead") && text(e) == "f''")
        .unwrap();
    let fingerings = |e: &ossia_types::GeometryElement| {
        elements
            .iter()
            .filter(|mark| {
                mark.region.element_kind.as_deref() == Some("Fingering")
                    && e.linked_targets.contains(&mark.region.target)
            })
            .map(|mark| text(mark).to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(fingerings(fingered), vec!["-2"]);
    assert_eq!(fingerings(other_fingered), vec!["-3"]);
    let tied: Vec<_> = elements
        .iter()
        .filter(|e| {
            e.region.element_kind.as_deref() == Some("NoteHead") && text(e).starts_with("g'")
        })
        .collect();
    assert_eq!(tied.len(), 3);
    let expanded = expand_selection(&pages, std::slice::from_ref(&tied[0].region.target));
    for note in &tied {
        assert!(
            expanded.contains(&note.region.target),
            "missing tied note: {expanded:?}"
        );
    }
    for tie in elements
        .iter()
        .filter(|e| e.region.element_kind.as_deref() == Some("Tie"))
    {
        assert!(expanded.contains(&tie.region.target));
    }
    for note in &tied {
        assert_eq!(
            expand_selection(&pages, std::slice::from_ref(&note.region.target)).len(),
            expanded.len()
        );
    }
}
