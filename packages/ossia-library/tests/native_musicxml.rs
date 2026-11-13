use ossia_library::*;
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with verified native 2.26.0 resources"]
fn native_musicxml_rehearsal_import_preserves_markup_anchors_and_tempo() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let library = Library::open(&work.path().join("library")).unwrap();
    let input = include_bytes!("../../../tests/fixtures/inputs/xml-rehearsal.musicxml");
    let imported = library
        .import(
            &engine,
            ImportRequest {
                title: "Rehearsal mark",
                input,
                format: InputFormat::MusicXml,
                confirmation: None,
                author: "native-test",
            },
            &RunContext::default(),
        )
        .unwrap();
    let document = imported.document.unwrap();
    let source = fs::read_to_string(&document.source_path).unwrap();
    assert!(source.contains("#{"));
    assert!(source.contains("\\pointAndClickOff"));
    assert!(source.contains("\n  \\midi { \\tempo 4 = 96 }"));
    let render = imported.render.unwrap();
    assert!(render.format_errors.is_empty(), "{render:?}");
    assert_eq!(render.pages.len(), 1);
    let svg = fs::read_to_string(
        library
            .artifact_root()
            .unwrap()
            .join(&render.pages[0].artifact.path),
    )
    .unwrap();
    assert!(svg.contains("textedit://score.ly:"));
    let midi = fs::read(
        library
            .artifact_root()
            .unwrap()
            .join(&render.midi.unwrap().path),
    )
    .unwrap();
    // 96 quarter notes per minute = 625,000 microseconds per quarter note.
    assert!(midi
        .windows(6)
        .any(|bytes| bytes == [0xff, 0x51, 3, 0x09, 0x89, 0x68]));
    let midi = ossia_score::parse_midi(&midi).unwrap();
    let notes: Vec<_> = midi.tracks.iter().flat_map(|track| &track.notes).collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].note, 60);
}
