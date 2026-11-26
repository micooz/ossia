use ossia_engine::{
    CompileOptions, DocumentId, Engine, ErrorCode, RenderFormat, RunContext, SourceSnapshot,
};
use ossia_score::{
    build_index, build_layout, build_playback, parse_midi, LocationCapability, SvgPage,
};
use std::{fs, path::PathBuf, time::Duration};

const PRELUDE: &str = include_str!("../../../tests/fixtures/playback/piano-prelude.ly");
const MARKER: &str = "% ossia:notation=jianpu\n";

#[test]
#[ignore = "requires native LilyPond and OSSIA_TEST_LILYPOND_ROOT"]
fn prelude_jianpu_preserves_midi_and_maps_all_measures() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::new(Duration::from_secs(60));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    let assets = work.path().join("artifacts");
    let snapshot =
        SourceSnapshot::new(DocumentId::new(), format!("{MARKER}{PRELUDE}").into_bytes()).unwrap();
    let original = SourceSnapshot::new(DocumentId::new(), PRELUDE.as_bytes().to_vec()).unwrap();
    let compile = |snapshot, formats| {
        engine
            .compile(
                snapshot,
                &CompileOptions {
                    formats,
                    point_and_click: true,
                },
                &assets,
                &context,
            )
            .unwrap()
    };
    let jianpu = compile(
        &snapshot,
        vec![RenderFormat::Svg, RenderFormat::Pdf, RenderFormat::Midi],
    );
    assert!(
        jianpu.format_errors.is_empty(),
        "{:?}",
        jianpu.format_errors
    );
    assert!(jianpu.pdf.is_some());
    let staff = compile(&original, vec![RenderFormat::Midi]);
    let jianpu_midi = fs::read(assets.join(jianpu.midi.unwrap().path)).unwrap();
    assert_eq!(
        jianpu_midi,
        fs::read(assets.join(staff.midi.unwrap().path)).unwrap()
    );

    let pages = jianpu
        .pages
        .iter()
        .map(|p| {
            let svg = fs::read_to_string(assets.join(&p.artifact.path)).unwrap();
            assert!(svg.contains("data-ossia-kind=\"JianpuStaff\""));
            (p.page, svg)
        })
        .collect::<Vec<_>>();
    let index = build_index(snapshot.source());
    let layout = build_layout(
        &index,
        &pages
            .iter()
            .map(|(page, svg)| SvgPage { page: *page, svg })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(
        layout.capability,
        LocationCapability::Measure,
        "{:?}; {:?}",
        layout.reason,
        layout
            .pages
            .iter()
            .map(|p| (&p.reason, p.systems.len()))
            .collect::<Vec<_>>()
    );
    let measures = layout
        .columns
        .iter()
        .filter_map(|c| c.measure_start.zip(c.measure_end))
        .flat_map(|(start, end)| start..=end)
        .collect::<Vec<_>>();
    assert_eq!(measures, (1..=35).collect::<Vec<_>>());

    let midi = parse_midi(&jianpu_midi).unwrap();
    let playback = build_playback(&index, &layout, &midi, snapshot.source_hash().clone()).unwrap();
    assert!(playback.systems.iter().any(|system| system.page == 2));
    let first_page = &layout.pages[0];
    let bass = first_page
        .objects
        .iter()
        .find(|object| {
            object.staff_index == Some(1) && object.note.is_some_and(|note| note.numerator == 0)
        })
        .unwrap();
    let upper_voice_rest = first_page
        .objects
        .iter()
        .find(|object| {
            object.kind == "Rest"
                && object.staff_index == Some(1)
                && object.timing.is_some_and(|timing| timing.numerator == 0)
        })
        .unwrap();
    let center_x = |bbox: ossia_score::BBox| (bbox.x0 + bbox.x1) / 2.;
    assert!((center_x(bass.bbox) - center_x(upper_voice_rest.bbox)).abs() < 0.1);
    assert!(bass.bbox.y0 > upper_voice_rest.bbox.y1);
    let final_page = layout.pages.last().unwrap();
    assert!(final_page
        .objects
        .iter()
        .any(|object| object.kind == "Arpeggio"));
    assert!(playback
        .groups
        .iter()
        .any(|group| !group.element_ids.is_empty() && group.pitches.contains(&60)));
    assert!(playback
        .notes
        .iter()
        .all(|note| playback.groups.iter().any(|group| {
            group.part_id.as_ref() == Some(&note.part_id)
                && group.start_seconds.is_some_and(|start| {
                    if let Some(anchor) = &note.ornament_anchor {
                        group.element_ids.contains(anchor) && start <= note.start_seconds + 0.001
                    } else {
                        group.pitches.contains(&note.pitch)
                            && (start - note.start_seconds).abs() < 0.001
                    }
                })
        })));
}

#[test]
#[ignore = "requires native LilyPond and OSSIA_TEST_LILYPOND_ROOT"]
fn jianpu_keys_and_unsupported_features_use_source_positions() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::new(Duration::from_secs(60));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    let assets = work.path().join("artifacts");
    let compile = |music: &str| {
        let source = format!("\u{feff}{MARKER}\\version \"2.26.0\"\n\\score {{ \\new Staff {{ {music} }} \\layout {{ }} }}\n");
        let snapshot = SourceSnapshot::new(DocumentId::new(), source.into_bytes()).unwrap();
        engine
            .compile(
                &snapshot,
                &CompileOptions {
                    formats: vec![RenderFormat::Svg],
                    point_and_click: true,
                },
                &assets,
                &context,
            )
            .unwrap()
    };
    for (key, music, label) in [
        (
            "g \\major",
            "g'8 fis' f' e' d'4 d''4 | <g' b' d''>1",
            "1 = G",
        ),
        (
            "a \\minor",
            "a'8 gis' g' f' e'4 e''4 | <a' c'' e''>1",
            "6 = A",
        ),
    ] {
        let score = compile(&format!("\\key {key} \\time 4/4 {music}"));
        assert!(score.format_errors.is_empty(), "{:?}", score.format_errors);
        assert!(score
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("stray UTF-8 BOM")));
        let svg = fs::read_to_string(assets.join(&score.pages[0].artifact.path)).unwrap();
        assert!(svg.contains(&format!("<tspan>{label}</tspan>")));
        assert_eq!(svg.matches("<tspan>–</tspan>").count(), 3);
    }
    let dotted = compile("c'4. r4. d'4 | e'2. r4 |");
    assert!(
        dotted.format_errors.is_empty(),
        "{:?}",
        dotted.format_errors
    );
    let svg = fs::read_to_string(assets.join(&dotted.pages[0].artifact.path)).unwrap();
    assert_eq!(svg.matches("<tspan>–</tspan>").count(), 2);
    assert_eq!(svg.matches("<tspan>0</tspan>").count(), 2);

    let rhythm = compile(
        r"\time 4/4
        \tuplet 3/2 { c'2 d'4 } \times 4/5 { f'8 g' a' b' c'' } |
        \grace { d'16 f' } g'4 \appoggiatura a'16 b'4
        \acciaccatura g'16 f'4 \afterGrace d'4 { c'16 } |",
    );
    assert!(
        rhythm.format_errors.is_empty(),
        "{:?}",
        rhythm.format_errors
    );
    let svg = fs::read_to_string(assets.join(&rhythm.pages[0].artifact.path)).unwrap();
    assert!(svg.contains("<tspan>3</tspan>"));
    assert!(svg.contains("<tspan>5:4</tspan>"));
    assert_eq!(svg.matches("<tspan>–</tspan>").count(), 1);

    for (music, feature) in [
        ("c'1 | \\key f \\major f'1", "a key change"),
        ("cih'1", "a microtonal"),
        ("\\change Staff = \"other\" c'1", "unresolved staff change"),
        ("c'8*2 d'2.", "scaled"),
        (
            "\\tuplet 3/2 { c'8 \\tuplet 3/2 { d'16 e' f' } g'8 } a'2.",
            "nested tuplets",
        ),
        ("R1", "multi-measure rests"),
    ] {
        let score = compile(music);
        assert!(score.pages.is_empty(), "{feature}");
        let error = &score.format_errors[0].error;
        assert_eq!(
            error.code,
            ErrorCode::UnsupportedSource,
            "{feature}: {error:?}"
        );
        assert!(error.message.contains(feature), "{error:?}");
        let detail = error.detail.as_ref().unwrap();
        assert!(
            detail["position"]["line"] == 3
                || detail["diagnostics"].as_array().is_some_and(|items| {
                    items
                        .iter()
                        .any(|diagnostic| diagnostic["position"]["line"] == 3)
                }),
            "{error:?}"
        );
    }
}
