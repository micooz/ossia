use ossia_score::*;
use serde_json::Value;
use std::{fs, path::PathBuf};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}
fn text(file: &str) -> String {
    fs::read_to_string(root().join(file)).unwrap()
}
fn rendered(id: &str) -> (String, Vec<String>) {
    let manifest: Value = serde_json::from_str(&text("rendered/manifest.json")).unwrap();
    let f = manifest
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap();
    (
        text(f["source"].as_str().unwrap()),
        f["pages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| text(p.as_str().unwrap()))
            .collect(),
    )
}
fn layout(source: &str, pages: &[String]) -> (ScoreIndex, ScoreLayout) {
    let index = build_index(source);
    let input: Vec<_> = pages
        .iter()
        .enumerate()
        .map(|(i, s)| SvgPage {
            page: i as u32 + 1,
            svg: s,
        })
        .collect();
    let layout = build_layout(&index, &input).unwrap();
    (index, layout)
}
fn point(index: &ScoreIndex, layout: &ScoreLayout, page: u32, x: f64, y: f64) -> LocationResult {
    locate(index, layout, &LocateQuery::Point { page, x, y }).unwrap()
}

#[test]
fn parallel_staves_and_all_pages_have_one_global_measure_grid() {
    let (source, pages) = rendered("polyphonic-pages");
    let (index, layout) = layout(&source, &pages);
    assert_eq!(
        layout
            .pages
            .iter()
            .map(|p| p.systems.len())
            .collect::<Vec<_>>(),
        vec![1, 1],
        "{layout:?}"
    );
    assert!(layout.pages.iter().all(|p| p.systems[0].staves.len() == 2));
    assert_eq!(
        layout
            .columns
            .iter()
            .map(|c| (c.page, c.measure_start, c.measure_end))
            .collect::<Vec<_>>(),
        vec![
            (1, Some(1), Some(1)),
            (1, Some(2), Some(2)),
            (2, Some(3), Some(3)),
            (2, Some(4), Some(4))
        ]
    );
    for c in &layout.columns {
        let page = &layout.pages[(c.page - 1) as usize];
        for (staff, s) in page.systems[c.system_index].staves.iter().enumerate() {
            let hit = point(
                &index,
                &layout,
                c.page,
                (c.x0 + c.x1) / 2.,
                (s.top + s.bot) / 2.,
            );
            let region = hit.region.expect("measure hit");
            assert_eq!(region.locator.staff_index, staff);
            assert_eq!(Some(region.locator.measure_start), c.measure_start);
            assert_eq!(region.spans.len(), if staff == 0 { 2 } else { 1 });
        }
    }
}

#[test]
fn compressed_rest_geometry_resolves_to_complete_source_measure_range() {
    let (source, pages) = rendered("compressed-rest");
    let (index, layout) = layout(&source, &pages);
    assert_eq!(
        layout
            .columns
            .iter()
            .map(|c| (c.measure_start, c.measure_end))
            .collect::<Vec<_>>(),
        (1..=5).map(|n| (Some(n), Some(n))).collect::<Vec<_>>(),
        "{layout:?}"
    );
    let c = &layout.columns[1];
    let s = &layout.pages[0].systems[0].staves[0];
    let hit = point(&index, &layout, 1, (c.x0 + c.x1) / 2., (s.top + s.bot) / 2.);
    let region = hit.region.unwrap();
    assert_eq!(
        (region.locator.measure_start, region.locator.measure_end),
        (2, 2)
    );
    let voice = &index.voices[0];
    assert_eq!(region.spans, vec![voice.measures[1].span]);
    assert_eq!(
        (
            voice.measures[1].measure_start,
            voice.measures[1].measure_end
        ),
        (2, 5)
    );
}

#[test]
fn invalid_pages_whitespace_staff_gaps_and_off_page_clicks_are_no_hit() {
    let (source, pages) = rendered("polyphonic-pages");
    let (index, layout) = layout(&source, &pages);
    let p = &layout.pages[0];
    let c = &layout.columns[0];
    let sys = &p.systems[0];
    let x = (c.x0 + c.x1) / 2.;
    let y = (sys.staves[0].top + sys.staves[0].bot) / 2.;
    for (page, x, y) in [
        (999, x, y),
        (1, -1000., y),
        (1, 1000., y),
        (1, x, 0.),
        (1, x, (sys.staves[0].bot + sys.staves[1].top) / 2.),
    ] {
        assert_eq!(
            point(&index, &layout, page, x, y).capability,
            LocationCapability::None,
            "{page},{x},{y}"
        );
    }
    let changed = build_index(&(source + "\n% changed"));
    assert_eq!(
        locate(&changed, &layout, &LocateQuery::Point { page: 1, x, y })
            .unwrap_err()
            .code,
        ErrorCode::StaleSelection
    );
}

#[test]
fn all_native_pages_have_explicit_capability_and_probe_anchors_map_to_original_bytes() {
    let manifest: Value = serde_json::from_str(&text("rendered/manifest.json")).unwrap();
    for f in manifest.as_array().unwrap() {
        let id = f["id"].as_str().unwrap();
        let (source, pages) = rendered(id);
        let (_, layout) = layout(&source, &pages);
        assert_eq!(layout.pages.len(), pages.len());
        for p in &layout.pages {
            if matches!(id, "chordmode") {
                continue;
            }
            assert_ne!(
                p.capability,
                LocationCapability::None,
                "{id}: {:?}",
                p.reason
            );
            for a in p
                .elements
                .iter()
                .filter_map(|e| e.anchor.as_ref())
                .filter(|a| a.byte.is_some())
            {
                let b = a.byte.unwrap();
                assert!(source.is_char_boundary(b));
                let pos = SourceMap::new(&source).position(b).unwrap();
                assert_eq!(pos.line, a.line);
                assert_eq!(pos.column - 1, a.char_offset);
            }
        }
    }
}

#[test]
fn a_failed_middle_page_requires_anchor_evidence_before_numbering_resumes() {
    let (source, pages) = rendered("polyphonic-pages");
    let index = build_index(&source);
    let broken = "<svg viewBox=\"0 0 100 100\"><g>";
    let layout = build_layout(
        &index,
        &[
            SvgPage {
                page: 1,
                svg: &pages[0],
            },
            SvgPage {
                page: 2,
                svg: broken,
            },
            SvgPage {
                page: 3,
                svg: &pages[1],
            },
        ],
    )
    .unwrap();
    assert_eq!(layout.pages[1].capability, LocationCapability::None);
    assert!(layout.reason.is_some());
    assert_eq!(
        layout
            .columns
            .iter()
            .filter(|c| c.page == 3)
            .map(|c| c.measure_start)
            .collect::<Vec<_>>(),
        vec![Some(3), Some(4)]
    );
    let no_anchors = pages[1].replace("textedit:", "opaque:");
    let layout = build_layout(
        &index,
        &[
            SvgPage {
                page: 1,
                svg: broken,
            },
            SvgPage {
                page: 2,
                svg: &no_anchors,
            },
        ],
    )
    .unwrap();
    assert!(layout.columns.iter().all(|c| c.measure_start.is_none()));
    assert_eq!(layout.pages[1].capability, LocationCapability::AnchorOnly);
}

#[test]
fn xml_transforms_paths_and_anomalies_have_checked_results() {
    assert_eq!(
        path_bbox("M 10 10 l 2 3 C 9 20 20 9 30 30 z").unwrap(),
        BBox {
            x0: 9.,
            y0: 9.,
            x1: 30.,
            y1: 30.
        }
    );
    assert!(path_bbox("M 0 0 A 1 1 0 0 0 4 4").is_err());
    assert!(path_bbox("M 0").is_err());
    assert!(path_bbox("M 1e999 2").is_err());
    let source = "Music = { c'1 | } \\score { \\new Staff \\Music }";
    let index = build_index(source);
    let simple="<svg viewBox='0 0 100 100'><g transform='translate(10 20) scale(2,-2)'><a href='textedit:///score.ly:1:10:11'><path d='M 1 2 L 3 4'/></a></g></svg>";
    // No staff lines means no measure layout, even though the XML itself is valid.
    assert_ne!(
        build_layout(
            &index,
            &[SvgPage {
                page: 1,
                svg: simple
            }]
        )
        .unwrap()
        .pages[0]
            .capability,
        LocationCapability::Measure
    );
    for svg in [
        "",
        "<svg/>",
        "<svg viewBox='0 0 100 100'><g></svg>",
        "<svg viewBox='0 0 100 100'><g transform='rotate(2)'/></svg>",
        "<!DOCTYPE svg><svg viewBox='0 0 1 1'/>",
        "<svg viewBox='0 0 NaN 100'/>",
    ] {
        let layout = build_layout(&index, &[SvgPage { page: 1, svg }]).unwrap();
        assert_eq!(
            layout.pages[0].capability,
            LocationCapability::None,
            "{svg}"
        );
        assert!(layout.pages[0].reason.is_some());
    }
    assert!(build_layout(
        &index,
        &[SvgPage {
            page: 0,
            svg: simple
        }]
    )
    .is_err());
}

#[test]
fn normalized_ts_geometry_agrees_for_supported_two_staff_pages() {
    #[derive(serde::Deserialize)]
    struct Observed {
        pages: Vec<OldPage>,
    }
    #[derive(serde::Deserialize)]
    struct OldPage {
        page: u32,
        #[serde(flatten)]
        layout: Value,
    }
    let old: Observed = serde_json::from_str(&text("analysis/geometry.json")).unwrap();
    let (source, pages) = rendered("polyphonic-pages");
    let (_, actual) = layout(&source, &pages);
    for (page, expected) in actual.pages.iter().zip(&old.pages) {
        assert_eq!(page.page, expected.page);
        assert!((page.staff_space - expected.layout["staffSpace"].as_f64().unwrap()).abs() < 1e-6);
        for (sys, old) in page
            .systems
            .iter()
            .zip(expected.layout["systems"].as_array().unwrap())
        {
            assert_eq!(
                sys.bar_xs,
                old["barXs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_f64().unwrap())
                    .collect::<Vec<_>>()
            );
            for (staff, old) in sys.staves.iter().zip(old["staffs"].as_array().unwrap()) {
                for (n, key) in [
                    (staff.top, "top"),
                    (staff.bot, "bot"),
                    (staff.x0, "x0"),
                    (staff.x1, "x1"),
                ] {
                    assert!((n - old[key].as_f64().unwrap()).abs() < 0.001);
                }
            }
        }
    }
}

#[test]
fn nested_transform_and_anchor_stacks_preserve_geometry_and_restore_context() {
    let source = "Music = { c'1 | } \\score { \\new Staff \\Music }";
    let byte = source.find("c'1").unwrap();
    let mut svg =
        "<svg viewBox='0 0 100 100'><g transform='translate(10 20) scale(2)'>".to_string();
    for y in 0..5 {
        svg += &format!("<line x1='0' x2='10' y1='{y}' y2='{y}'/>");
    }
    svg += "<rect x='0' y='0' width='.1' height='4'/><rect x='10' y='0' width='.1' height='4'/>";
    svg+=&format!("<a href='textedit:///a&amp;b/score.ly:1:{byte}:{}'><g transform='translate(1 1)'><path d='M 0 0 L 1 1' transform='scale(1 -1)'/></g></a><path d='M 3 2 L 4 3'/></g></svg>",byte+1);
    let (_, layout) = layout(source, &[svg]);
    let page = &layout.pages[0];
    assert_eq!(page.capability, LocationCapability::Measure, "{layout:?}");
    assert_eq!(page.systems[0].staves[0].top, 20.);
    assert_eq!(page.systems[0].staves[0].bot, 28.);
    let paths: Vec<_> = page.elements.iter().filter(|e| e.kind == "path").collect();
    assert_eq!(
        paths[0].bbox,
        BBox {
            x0: 12.,
            y0: 20.,
            x1: 14.,
            y1: 22.
        }
    );
    assert_eq!(paths[0].anchor.as_ref().unwrap().byte, Some(byte));
    assert!(paths[0].anchor.as_ref().unwrap().href.contains("a&b"));
    assert!(paths[1].anchor.is_none());
}

#[test]
fn whole_staff_stems_do_not_create_duplicate_source_measures() {
    let source = "Music = { c'4 d'4 e'4 f'4 | g'1 | } \\score { \\new Staff \\Music }";
    let first = source.find("c'4").unwrap();
    let second = source.find("e'4").unwrap();
    let last = source.find("g'1").unwrap();
    let mut svg = "<svg viewBox='0 0 100 100'>".to_string();
    for y in 10..15 {
        svg += &format!("<line x1='10' x2='50' y1='{y}' y2='{y}'/>");
    }
    for x in [10, 20, 30, 50] {
        svg += &format!("<rect x='{x}' y='10' width='.13' height='4'/>");
    }
    for (x, byte) in [(15, first), (25, second), (40, last)] {
        svg += &format!(
            "<a href='textedit:///score.ly:1:{byte}:{}'><path d='M {x} 12 l 1 1'/></a>",
            byte + 1
        );
    }
    svg += "</svg>";
    let (_, layout) = layout(source, &[svg]);
    assert_eq!(
        layout
            .columns
            .iter()
            .map(|c| (c.column_index, c.measure_start, c.x0, c.x1))
            .collect::<Vec<_>>(),
        vec![(0, Some(1), 10., 30.), (1, Some(2), 30., 50.)]
    );
}

#[test]
fn malformed_and_unsupported_svg_are_distinct_structured_errors() {
    let index = build_index("Music={c'1 |} \\score{\\new Staff \\Music}");
    for (svg, code) in [
        ("<svg viewBox='0 0 100 100'><g>", ErrorCode::InvalidInput),
        (
            "<svg viewBox='0 0 100 100'><g transform='rotate(3)'/></svg>",
            ErrorCode::UnsupportedSource,
        ),
    ] {
        let layout = build_layout(&index, &[SvgPage { page: 1, svg }]).unwrap();
        assert_eq!(layout.pages[0].error.as_ref().unwrap().code, code);
    }
}
