use ossia_engine::*;
use ossia_score::{build_index, build_layout, selection_geometry, SvgPage};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn interactive_elements_have_exact_source_bounds_without_changing_the_score() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let source = r#"\version "2.26.0"
\header {
  "title" = "点选测试"
  subtitle = \markup { \italic "第二行" }
  composer = "作者"
  copyright = "版权"
  "id: software" = "music21 v.10.5.2"
  "id: encoding-date" = "2026-11-07"
}
\layout { \context { \Staff printKeyCancellation = ##f } \context { \Score autoBeaming = ##f } }
Music = { \clef treble \key g \major \time 4/4 \tempo "Andante" 4 = 80
  <c' e' g'>4\p-. d'4^\markup { "表情" } e'4\< ( f'4 | g'1 )\! \mark \markup { "A" } \break
  r1 | c'1 \bar "|." }
\score { \new Staff \Music \layout {} \midi {} }
"#;
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let mut renders = Vec::new();
    for point_and_click in [false, true] {
        let render = engine
            .compile(
                &snapshot,
                &CompileOptions {
                    formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                    point_and_click,
                },
                &work.path().join("artifacts"),
                &RunContext::default(),
            )
            .unwrap();
        assert!(
            render.format_errors.is_empty(),
            "{:#?}",
            render.format_errors
        );
        renders.push(render);
    }
    assert_eq!(
        renders[0].midi.as_ref().unwrap().sha256,
        renders[1].midi.as_ref().unwrap().sha256
    );
    let pngs = renders
        .iter()
        .map(|render| {
            let svg = fs::read(
                work.path()
                    .join("artifacts")
                    .join(&render.pages[0].artifact.path),
            )
            .unwrap();
            engine
                .rasterize_svg(
                    &svg,
                    1200,
                    &work.path().join("pixels"),
                    &RunContext::default(),
                )
                .unwrap()
                .sha256
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pngs[0], pngs[1],
        "interactive metadata must not change visible pixels"
    );
    let svgs: Vec<_> = renders[1]
        .pages
        .iter()
        .map(|p| {
            (
                p.page,
                fs::read_to_string(work.path().join("artifacts").join(&p.artifact.path)).unwrap(),
            )
        })
        .collect();
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
    let elements = pages.iter().flat_map(|p| &p.elements).collect::<Vec<_>>();
    for kind in ["Slur", "Hairpin"] {
        let curve = elements
            .iter()
            .find(|e| e.region.element_kind.as_deref() == Some(kind))
            .unwrap();
        assert!(
            !curve.hit_bounds.is_empty(),
            "missing ink bounds for {kind}"
        );
    }
    for (kind, raw) in [
        ("Clef", "\\clef treble"),
        ("TimeSignature", "\\time 4/4"),
        ("KeySignature", "\\key g \\major"),
        ("DynamicText", "\\p"),
        ("TextScript", "^\\markup { \"表情\" }"),
        ("RehearsalMark", "\\mark \\markup { \"A\" }"),
    ] {
        let object = elements
            .iter()
            .find(|e| e.region.element_kind.as_deref() == Some(kind))
            .unwrap();
        assert_eq!(
            &source[object.region.spans[0].byte_start..object.region.spans[0].byte_end],
            raw,
            "{kind}"
        );
    }
    for kind in [
        "header-title",
        "header-subtitle",
        "header-composer",
        "header-copyright",
        "NoteHead",
        "Rest",
        "Clef",
        "KeySignature",
        "TimeSignature",
        "DynamicText",
        "Script",
        "TextScript",
        "MetronomeMark",
        "RehearsalMark",
        "Hairpin",
        "Slur",
    ] {
        assert!(
            elements
                .iter()
                .any(|e| e.region.element_kind.as_deref() == Some(kind)),
            "missing {kind}: {elements:#?}"
        );
    }
    let chord: Vec<_> = elements
        .iter()
        .filter(|e| {
            e.region.element_kind.as_deref() == Some("NoteHead")
                && e.region
                    .spans
                    .iter()
                    .any(|s| ["c'", "e'", "g'"].contains(&&source[s.byte_start..s.byte_end]))
        })
        .collect();
    assert!(chord.len() >= 3, "{chord:#?}");
    assert!(
        pages.iter().any(|p| !p.regions.is_empty()),
        "index={index:?}; columns={:?}",
        layout.columns
    );
    for e in elements {
        assert!(
            e.bounds.x1 > e.bounds.x0 && e.bounds.y1 > e.bounds.y0,
            "{e:?}"
        );
        for span in &e.region.spans {
            assert!(!ossia_score::SourceMap::new(source)
                .slice(*span)
                .unwrap()
                .is_empty())
        }
    }
}

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn default_symbols_reused_voices_and_dynamic_headers_keep_their_source_identity() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let source = r#"\version "2.26.0"
TitleText = "标题变量"
NoAuthor = ##f
\header { title = \TitleText subtitle = #(string-append "动态" "标题") arranger = \NoAuthor copyright = #(if #f "hidden" #f) }
Shared = { c'1( | \break d'1) \bar "|." }
\score { \new PianoStaff << \new Staff = "upper" \Shared \new Staff = "lower" \Shared >> \layout {} }
"#;
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Svg],
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
    let svg = fs::read_to_string(
        work.path()
            .join("artifacts")
            .join(&render.pages[0].artifact.path),
    )
    .unwrap();
    assert!(!svg.contains("ossia:") && !svg.contains("/private/"));
    let index = build_index(source);
    let layout = build_layout(&index, &[SvgPage { page: 1, svg: &svg }]).unwrap();
    let pages = selection_geometry(&index, &layout).unwrap();
    assert_eq!(pages[0].regions.len(), 4, "{:#?}", layout.columns);
    for kind in ["Clef", "TimeSignature"] {
        let generated = pages[0]
            .elements
            .iter()
            .find(|e| e.region.element_kind.as_deref() == Some(kind))
            .unwrap();
        assert!(generated.region.spans.is_empty());
        assert_eq!(
            source.as_bytes()[generated.region.insertion_anchor.unwrap().byte_start - 1],
            b'{'
        );
        assert!(generated.region.occurrences.len() >= 2);
    }
    let note = pages[0]
        .elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("NoteHead"))
        .unwrap();
    assert_eq!(note.region.occurrences.len(), 2);
    let slur = pages[0]
        .elements
        .iter()
        .find(|e| e.region.element_kind.as_deref() == Some("Slur"))
        .unwrap();
    assert_eq!(slur.region.occurrences.len(), 4);
    for kind in ["header-title", "header-subtitle"] {
        assert!(pages[0]
            .elements
            .iter()
            .any(|e| e.region.element_kind.as_deref() == Some(kind)));
    }
    assert!(!pages[0].elements.iter().any(|e| matches!(
        e.region.element_kind.as_deref(),
        Some("header-copyright" | "header-arranger")
    )));
}

#[test]
#[ignore = "requires OSSIA_TEST_SELECTION_SOURCE and native LilyPond 2.26.0"]
fn native_source_selection_matches_static_render() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let source_path =
        std::env::var_os("OSSIA_TEST_SELECTION_SOURCE").expect("set OSSIA_TEST_SELECTION_SOURCE");
    let source = fs::read_to_string(source_path).unwrap();
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let mut renders = Vec::new();
    for point_and_click in [false, true] {
        let render = engine
            .compile(
                &snapshot,
                &CompileOptions {
                    formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                    point_and_click,
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
        renders.push(render);
    }
    assert_eq!(
        renders[0].midi.as_ref().unwrap().sha256,
        renders[1].midi.as_ref().unwrap().sha256
    );
    assert_eq!(renders[0].pages.len(), renders[1].pages.len());
    let mut svgs = Vec::new();
    for (plain, interactive) in renders[0].pages.iter().zip(&renders[1].pages) {
        let images = [plain, interactive].map(|page| {
            let svg = fs::read(work.path().join("artifacts").join(&page.artifact.path)).unwrap();
            let png = engine
                .rasterize_svg(
                    &svg,
                    1200,
                    &work.path().join("pixels"),
                    &RunContext::default(),
                )
                .unwrap();
            (svg, png.sha256)
        });
        assert_eq!(images[0].1, images[1].1, "page {} changed", plain.page);
        svgs.push((
            interactive.page,
            String::from_utf8(images[1].0.clone()).unwrap(),
        ));
    }
    let index = build_index(&source);
    let layout = build_layout(
        &index,
        &svgs
            .iter()
            .map(|(page, svg)| SvgPage { page: *page, svg })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let pages = selection_geometry(&index, &layout).unwrap();
    let mut notes = 0;
    for page in &pages {
        let note = page
            .elements
            .iter()
            .find(|e| {
                e.region.element_kind.as_deref() == Some("NoteHead")
                    && e.region.staff_index == Some(0)
            })
            .expect("page has no selectable upper-staff notes");
        notes += page
            .elements
            .iter()
            .filter(|e| e.region.element_kind.as_deref() == Some("NoteHead"))
            .count();
        let selected = ossia_score::hit_selection(
            &index,
            &pages,
            page.page,
            (note.bounds.x0 + note.bounds.x1) / 2.,
            (note.bounds.y0 + note.bounds.y1) / 2.,
        )
        .unwrap()
        .unwrap();
        assert_eq!(selected.target, note.region.target);
        let region = page
            .regions
            .iter()
            .find(|r| r.locator.staff_index == 0)
            .unwrap();
        let prefix = page
            .elements
            .iter()
            .filter(|e| {
                e.region.staff_index == Some(0)
                    && matches!(
                        e.region.element_kind.as_deref(),
                        Some("Clef" | "TimeSignature" | "KeySignature")
                    )
                    && e.bounds.x0 < note.bounds.x0
                    && e.bounds.y0 < region.bounds.y1
                    && e.bounds.y1 > region.bounds.y0
            })
            .map(|e| e.bounds.x1)
            .max_by(f64::total_cmp)
            .unwrap();
        assert!(
            region.bounds.x0 > prefix,
            "page {} includes the prefix",
            page.page
        );
    }
    println!(
        "verified {} pages, {notes} selectable note heads; static pixels and MIDI match",
        pages.len()
    );
}
