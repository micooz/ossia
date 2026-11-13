use ossia_engine::*;
use serde_json::json;
use std::{fs, path::PathBuf, time::Duration};

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
            ImportStatus::Refused,
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
