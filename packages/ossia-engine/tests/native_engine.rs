use ossia_engine::*;
use serde_json::json;
use std::{fs, path::PathBuf, time::Duration};

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn native_convert_ly_migrates_an_independent_candidate() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::default();
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    let source = b"\\version \"2.14.2\"\n\\score { { \\times 2/3 { c'4 d' e' } } \\layout {} }\n";
    let report = engine.migrate_lilypond(source, &context).unwrap();
    assert_eq!(report.status, ImportStatus::Clean, "{:?}", report.blocking);
    assert_eq!(report.input_hash, ContentHash::of(source));
    let candidate = report.source.unwrap();
    assert!(candidate.contains("\\version \"2.26.0\""));
    assert!(candidate.contains("\\tuplet 3/2"));
    assert!(engine
        .migrate_lilypond(b"\\version \"2.27.0\"", &context)
        .is_err());
    assert!(engine.migrate_lilypond(b"{ c'1 }", &context).is_err());
    let snapshot = SourceSnapshot::new(DocumentId::new(), candidate.into_bytes()).unwrap();
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                point_and_click: true,
            },
            &work.path().join("assets"),
            &context,
        )
        .unwrap();
    assert!(
        render.format_errors.is_empty(),
        "{:?}",
        render.format_errors
    );
    assert!(render.midi.is_some());
}

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn native_starter_scores_identify_solo_instruments() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::new(Duration::from_secs(180));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    for (source, expected) in [
        (
            include_str!("../../ossia-backend/resources/starter/bach-bwv1002-01-allemanda.ly"),
            SampleInstrument::Violin,
        ),
        (
            include_str!("../../ossia-backend/resources/starter/ukulele-minuet-bwv-anh114.ly"),
            SampleInstrument::Ukulele,
        ),
    ] {
        let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
        let assets = work.path().join(format!("assets-{expected:?}"));
        let render = engine
            .compile(
                &snapshot,
                &CompileOptions {
                    formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                    point_and_click: true,
                },
                &assets,
                &context,
            )
            .unwrap();
        assert!(
            render.format_errors.is_empty(),
            "{:?}",
            render.format_errors
        );
        let midi =
            ossia_score::parse_midi(&fs::read(assets.join(render.midi.unwrap().path)).unwrap())
                .unwrap();
        let pages = render
            .pages
            .iter()
            .map(|page| {
                (
                    page.page,
                    fs::read_to_string(assets.join(&page.artifact.path)).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let index = ossia_score::build_index(source);
        let layout = ossia_score::build_layout(
            &index,
            &pages
                .iter()
                .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let playback =
            ossia_score::build_playback(&index, &layout, &midi, ContentHash::of(source.as_bytes()))
                .unwrap();
        assert!(!playback.notes.is_empty());
        assert!(
            playback
                .notes
                .iter()
                .all(|note| note.instrument == expected),
            "expected {expected:?}, parts: {:?}",
            playback.parts
        );
    }
}

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn native_playback_identifies_instruments_and_keeps_unison_parts() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::new(Duration::from_secs(180));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    let source = r#"\version "2.26.0"
    \score { <<
      \new Staff = "partA" \with { instrumentName = "Ukulele" midiInstrument = "acoustic guitar (nylon)" } { c'4 d'4 }
      \new Staff = "partB" \with { instrumentName = "Violin" midiInstrument = "violin" } { c'4 e'4 }
    >> \layout {} \midi {} }"#;
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let assets = work.path().join("assets");
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                point_and_click: true,
            },
            &assets,
            &context,
        )
        .unwrap();
    assert!(
        render.format_errors.is_empty(),
        "{:?}",
        render.format_errors
    );
    let midi = ossia_score::parse_midi(&fs::read(assets.join(render.midi.unwrap().path)).unwrap())
        .unwrap();
    let pages = render
        .pages
        .iter()
        .map(|page| {
            (
                page.page,
                fs::read_to_string(assets.join(&page.artifact.path)).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let index = ossia_score::build_index(source);
    let layout = ossia_score::build_layout(
        &index,
        &pages
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let playback =
        ossia_score::build_playback(&index, &layout, &midi, ContentHash::of(source.as_bytes()))
            .unwrap();
    let opening = playback
        .notes
        .iter()
        .filter(|note| note.pitch == 60 && note.start_seconds < 0.01)
        .collect::<Vec<_>>();
    assert_eq!(
        opening.len(),
        2,
        "parts: {:?}, notes: {:?}, MIDI: {:?}",
        playback.parts,
        playback.notes,
        midi.tracks
    );
    assert_ne!(opening[0].part_id, opening[1].part_id);
    assert!(
        opening
            .iter()
            .any(|note| note.instrument == SampleInstrument::Ukulele),
        "{:?}",
        playback.parts
    );
    assert!(
        opening
            .iter()
            .any(|note| note.instrument == SampleInstrument::Violin),
        "{:?}",
        playback.parts
    );
}

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn native_notation_and_tab_do_not_double_the_same_performance() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::new(Duration::from_secs(180));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    let source = r#"\version "2.26.0"
      music = { c'4 d'4 e'4 f'4 }
      \score { \new StaffGroup << \new Staff { \music } \new TabStaff { \music } >> \layout {} \midi {} }"#;
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let assets = work.path().join("assets");
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                point_and_click: true,
            },
            &assets,
            &context,
        )
        .unwrap();
    assert!(
        render.format_errors.is_empty(),
        "{:?}",
        render.format_errors
    );
    let midi = ossia_score::parse_midi(&fs::read(assets.join(render.midi.unwrap().path)).unwrap())
        .unwrap();
    assert_eq!(
        midi.tracks
            .iter()
            .map(|track| track.notes.len())
            .sum::<usize>(),
        8
    );
    let pages = render
        .pages
        .iter()
        .map(|page| {
            (
                page.page,
                fs::read_to_string(assets.join(&page.artifact.path)).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let index = ossia_score::build_index(source);
    let layout = ossia_score::build_layout(
        &index,
        &pages
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let playback =
        ossia_score::build_playback(&index, &layout, &midi, ContentHash::of(source.as_bytes()))
            .unwrap();
    assert_eq!(playback.notes.len(), 4, "{:?}", playback.notes);
    assert_eq!(playback.parts.len(), 1, "{:?}", playback.parts);
    assert!(playback
        .notes
        .iter()
        .all(|note| note.instrument == SampleInstrument::Guitar));
    assert!(playback
        .groups
        .iter()
        .all(|group| group.element_ids.len() == 2));
}

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn native_midi_preserves_scaled_skip_timing() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::new(Duration::from_secs(180));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    let source = "\\version \"2.26.0\" \\score { { c'4 \\skip 1*2 d'4 \\skip 4*3 e'4 } \\midi {} }";
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let assets = work.path().join("assets");
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Midi],
                point_and_click: true,
            },
            &assets,
            &context,
        )
        .unwrap();
    assert!(
        render.format_errors.is_empty(),
        "{:?}",
        render.format_errors
    );
    let parsed =
        ossia_score::parse_midi(&fs::read(assets.join(render.midi.unwrap().path)).unwrap())
            .unwrap();
    let ossia_score::MidiDivision::Ppqn { ticks_per_quarter } = parsed.division else {
        panic!("expected PPQN");
    };
    let notes = parsed
        .tracks
        .iter()
        .flat_map(|track| &track.notes)
        .collect::<Vec<_>>();
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[0].time, 0);
    assert_eq!(notes[1].time, u64::from(ticks_per_quarter) * 9);
    assert_eq!(notes[2].time, u64::from(ticks_per_quarter) * 13);
}

#[test]
#[ignore = "requires OSSIA_TEST_LILYPOND_ROOT and OSSIA_TEST_LY_INPUTS"]
fn native_midi_export_from_external_sources() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let paths = std::env::split_paths(&std::env::var_os("OSSIA_TEST_LY_INPUTS").unwrap())
        .collect::<Vec<_>>();
    assert!(!paths.is_empty());
    let work = tempfile::tempdir().unwrap();
    let context = RunContext::new(Duration::from_secs(180));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context).unwrap();
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
        let assets = work.path().join("assets");
        let render = engine
            .compile(
                &snapshot,
                &CompileOptions {
                    formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                    point_and_click: true,
                },
                &assets,
                &context,
            )
            .unwrap();
        assert!(
            render.format_errors.is_empty(),
            "{}: {:?}",
            path.display(),
            render.format_errors
        );
        assert!(!render.pages.is_empty(), "{}", path.display());
        let midi = render.midi.expect("MIDI artifact");
        let parsed = ossia_score::parse_midi(&fs::read(assets.join(midi.path)).unwrap()).unwrap();
        assert!(
            parsed.tracks.iter().any(|track| !track.notes.is_empty()),
            "{}",
            path.display()
        );
        let pages = render
            .pages
            .iter()
            .map(|page| {
                (
                    page.page,
                    fs::read_to_string(assets.join(&page.artifact.path)).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let index = ossia_score::build_index(&source);
        let layout = ossia_score::build_layout(
            &index,
            &pages
                .iter()
                .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let playback = ossia_score::build_playback(
            &index,
            &layout,
            &parsed,
            ContentHash::of(source.as_bytes()),
        )
        .unwrap();
        assert!(!playback.notes.is_empty(), "{}", path.display());
        let wav = render_wav(
            &playback,
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/public/audio"),
            &context,
        )
        .unwrap();
        assert!(wav.starts_with(b"RIFF"));
        assert!(wav[44..].iter().any(|byte| *byte != 0));
        let progress = context.progress().unwrap();
        assert_eq!(progress.phase, "audio");
        assert_eq!((progress.completed, progress.total), (Some(100), Some(100)));
        eprintln!(
            "{}: MIDI notes {}, playback notes {}, {:.1}s, WAV {} bytes",
            path.display(),
            parsed
                .tracks
                .iter()
                .map(|track| track.notes.len())
                .sum::<usize>(),
            playback.notes.len(),
            playback.duration_seconds,
            wav.len()
        );
    }
}

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with prepared native 2.26.0 resources"]
fn native_conversion_compilation_cache_and_original_mapping() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
        fs::create_dir(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &to.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }
    let relocated = work.path().join("中文 engine with spaces");
    copy_tree(&root, &relocated);
    let root = relocated;
    let scratch = work.path().join("中文 scratch");
    let context = RunContext::new(Duration::from_secs(180));
    let engine = Engine::verify(&root, &scratch, &context).unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let document = "10000000-0000-4000-8000-000000000001".parse().unwrap();
    let options = CompileOptions {
        formats: vec![RenderFormat::Svg, RenderFormat::Pdf, RenderFormat::Midi],
        point_and_click: true,
    };
    let assets = work.path().join("产物 assets");
    let mut evidence = Vec::new();
    for (file, format, expected) in [
        ("polyphonic-pages.ly", InputFormat::Ly, ImportStatus::Clean),
        ("unicode-crlf-bom.ly", InputFormat::Ly, ImportStatus::Clean),
        ("standard-include.ly", InputFormat::Ly, ImportStatus::Clean),
        (
            "xml-clean.musicxml",
            InputFormat::MusicXml,
            ImportStatus::Clean,
        ),
        ("xml-clean.mxl", InputFormat::Mxl, ImportStatus::Clean),
        (
            "xml-rehearsal.musicxml",
            InputFormat::MusicXml,
            ImportStatus::Clean,
        ),
        (
            "xml-loss.musicxml",
            InputFormat::MusicXml,
            ImportStatus::NeedsConfirmation,
        ),
        (
            "xml-negative-skip.musicxml",
            InputFormat::MusicXml,
            ImportStatus::NeedsConfirmation,
        ),
        ("midi-valid.mid", InputFormat::Midi, ImportStatus::Clean),
    ] {
        let bytes = fs::read(fixtures.join("inputs").join(file)).unwrap();
        let before = ContentHash::of(&bytes);
        let report = engine.convert(&bytes, format, None, &context).unwrap();
        assert_eq!(report.status, expected, "{file}: {report:?}");
        let again = engine.convert(&bytes, format, None, &context).unwrap();
        assert_eq!(
            report.source, again.source,
            "conversion source must not depend on scratch path"
        );
        assert_eq!(report.confirmation, again.confirmation);
        if expected == ImportStatus::Refused {
            assert!(report.source.is_none());
            assert_eq!(
                engine
                    .convert(&bytes, format, Some(&before), &context)
                    .unwrap_err()
                    .code,
                ErrorCode::ImportRefused
            );
            evidence.push(json!({"input":file,"report":report}));
            continue;
        }
        if let Some(token) = &report.confirmation {
            let confirmed = engine
                .convert(&bytes, format, Some(token), &context)
                .unwrap();
            assert!(confirmed.confirmed);
            assert_eq!(
                engine
                    .convert(&bytes, format, Some(&before), &context)
                    .unwrap_err()
                    .code,
                ErrorCode::Conflict
            );
        }
        let source = report.source.as_ref().unwrap();
        let snapshot = SourceSnapshot::new(document, source.as_bytes().to_vec()).unwrap();
        let manifest = engine
            .compile(&snapshot, &options, &assets, &context)
            .unwrap();
        assert!(!manifest.pages.is_empty(), "{file}: {manifest:?}");
        assert!(manifest.pdf.is_some(), "{file}: {manifest:?}");
        assert!(manifest.midi.is_some(), "{file}: {manifest:?}");
        assert!(manifest.format_errors.is_empty(), "{file}: {manifest:?}");
        if file == "polyphonic-pages.ly" {
            assert_eq!(manifest.pages.len(), 2);
        }
        let index = ossia_score::build_index(source);
        let svg: Vec<_> = manifest
            .pages
            .iter()
            .map(|p| fs::read_to_string(assets.join(&p.artifact.path)).unwrap())
            .collect();
        let pages: Vec<_> = svg
            .iter()
            .enumerate()
            .map(|(i, s)| ossia_score::SvgPage {
                page: i as u32 + 1,
                svg: s,
            })
            .collect();
        let layout = ossia_score::build_layout(&index, &pages).unwrap();
        assert!(
            layout
                .pages
                .iter()
                .any(|p| p.elements.iter().any(|e| e.anchor.is_some())),
            "{file}: missing anchors"
        );
        for page in &layout.pages {
            for anchor in page.elements.iter().filter_map(|e| e.anchor.as_ref()) {
                assert!(
                    ossia_score::SourceMap::new(source)
                        .anchor_byte(anchor.line, anchor.char_offset)
                        .is_some(),
                    "{file}: {anchor:?}"
                );
            }
        }
        for text in &svg {
            assert!(!text.contains(&work.path().to_string_lossy().to_string()));
        }
        let fresh = Engine::verify(&root, &scratch, &context).unwrap();
        assert_eq!(
            fresh
                .compile(&snapshot, &options, &assets, &context)
                .unwrap(),
            manifest
        );
        let first = assets.join(&manifest.pages[0].artifact.path);
        fs::write(&first, "damaged").unwrap();
        assert_eq!(
            engine
                .compile(&snapshot, &CompileOptions::default(), &assets, &context)
                .unwrap()
                .pages,
            manifest.pages
        );
        fs::remove_file(&first).unwrap();
        assert_eq!(
            engine
                .compile(&snapshot, &CompileOptions::default(), &assets, &context)
                .unwrap()
                .pages,
            manifest.pages
        );
        assert_eq!(snapshot.bytes(), source.as_bytes());
        assert_eq!(
            ContentHash::of(&fs::read(fixtures.join("inputs").join(file)).unwrap()),
            before
        );
        evidence.push(json!({"input":file,"report":report,"render":manifest,"sourceUnchanged":true,"cacheAfterRestart":true,"damagedAndMissingRebuilt":true}));
    }
    // A source error after the derivation point must still refer to original source bytes.
    let invalid = "\\version \"2.26.0\"\r\n\\pointAndClickOff\r\n\\score { { \\mark \\markup { \"中文😀\" }\t\\unknownProbe c'1 } \\layout {} }\r\n";
    let snapshot = SourceSnapshot::new(document, invalid.as_bytes().to_vec()).unwrap();
    let result = engine
        .compile(&snapshot, &options, &assets, &context)
        .unwrap();
    assert!(!result.format_errors.is_empty());
    assert!(
        result.diagnostics.iter().any(|d| d
            .position
            .is_some_and(|p| invalid[p.byte..].starts_with("\\unknownProbe"))),
        "{result:?}"
    );
    assert!(fs::read_dir(&scratch).unwrap().next().is_none());
    if let Some(output) = std::env::var_os("OSSIA_NATIVE_EVIDENCE") {
        fs::write(output, format!("{}\n", serde_json::to_string_pretty(&json!({"target":host_target().unwrap(),"engineBuildId":engine.build_id(),"cases":evidence,"diagnosticSourceMapping":true,"relocatedResourcesWithUnicodeAndSpaces":true})).unwrap())).unwrap();
    }
}

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with prepared native 2.26.0 resources"]
fn chord_names_do_not_contribute_to_midi() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let source = r#"\version "2.26.0"
harmonies = \chordmode { d1 }
\score { <<
  \new ChordNames { \chordmode { c1 } }
  \new ChordNames \harmonies
  \new Staff { c'1 }
>> \layout {} \midi {} }
"#;
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let artifacts = work.path().join("artifacts");
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Midi],
                point_and_click: false,
            },
            &artifacts,
            &RunContext::default(),
        )
        .unwrap();
    assert!(
        render.format_errors.is_empty(),
        "{:?}",
        render.format_errors
    );
    let midi = ossia_score::parse_midi(
        &fs::read(artifacts.join(&render.midi.as_ref().unwrap().path)).unwrap(),
    )
    .unwrap();
    let notes: Vec<_> = midi.tracks.iter().flat_map(|track| &track.notes).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].note, 60);
}

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with prepared native 2.26.0 resources"]
fn pdf_renders_with_bundled_ghostscript() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let source = r#"\version "2.26.0"
\score { \new Staff { c'4 d' e' f' | } \layout {} }
"#;
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    let artifacts = work.path().join("artifacts");
    let render = engine
        .compile(
            &snapshot,
            &CompileOptions {
                formats: vec![RenderFormat::Pdf],
                point_and_click: false,
            },
            &artifacts,
            &RunContext::default(),
        )
        .unwrap();

    assert!(
        render.format_errors.is_empty(),
        "{:?}",
        render.format_errors
    );
    let pdf = render.pdf.expect("PDF should be produced");
    assert!(fs::read(artifacts.join(pdf.path))
        .unwrap()
        .starts_with(b"%PDF-"));
}

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with prepared native 2.26.0 resources"]
fn omr_conversion_can_drop_musicxml_page_layout() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let source = br#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <defaults>
    <scaling><millimeters>6.4347</millimeters><tenths>40</tenths></scaling>
    <page-layout>
      <page-height>2006</page-height>
      <page-width>1465</page-width>
    </page-layout>
  </defaults>
  <part-list><score-part id="P1"><part-name>Piano</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>1</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><type>whole</type></note>
    </measure>
  </part>
</score-partwise>
"#;
    let layout = engine
        .convert(source, InputFormat::MusicXml, None, &RunContext::default())
        .unwrap();
    assert!(layout.source.unwrap().contains("paper-width"));
    let compact = engine
        .convert_with_options(
            source,
            InputFormat::MusicXml,
            None,
            &RunContext::default(),
            ConvertOptions {
                ignore_page_layout: true,
            },
        )
        .unwrap();
    let compact = compact.source.unwrap();
    assert!(!compact.contains("paper-width"));
    assert!(!compact.contains("paper-height"));
}
