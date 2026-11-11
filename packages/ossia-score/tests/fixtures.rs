use ossia_score::*;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}
fn text(file: &str) -> String {
    fs::read_to_string(root().join(file)).unwrap()
}
fn input(id: &str) -> String {
    text(&format!("inputs/{id}.ly"))
}
fn locator(staff_index: usize, start: u32, end: u32) -> Locator {
    Locator {
        staff_index,
        measure_start: start,
        measure_end: end,
        voice: None,
    }
}

#[test]
fn tokenizer_matches_normalized_ts_observations_and_preserves_all_byte_spans() {
    #[derive(serde::Deserialize)]
    struct Observed {
        samples: Vec<Sample>,
    }
    #[derive(serde::Deserialize)]
    struct Sample {
        id: String,
        tokens: Vec<Value>,
    }
    let observed: Observed = serde_json::from_str(&text("analysis/tokens.json")).unwrap();
    for sample in observed.samples {
        let id = sample.id.as_str();
        let expected = &sample.tokens;
        let source = input(id);
        let map = SourceMap::new(&source);
        let tokens = tokenize(&source);
        assert_eq!(serialize(&tokens), source, "{id}");
        let actual:Vec<_>=tokens.iter().map(|t| json!({"kind":t.kind,"mode":t.mode,"raw":t.raw,"span":map.span(t.byte_start,t.byte_end).unwrap()})).collect();
        assert_eq!(&actual, expected, "token parity: {id}");
        for token in tokens {
            assert_eq!(
                map.slice(map.span(token.byte_start, token.byte_end).unwrap())
                    .unwrap(),
                token.raw
            );
        }
    }
}

#[test]
fn named_inline_absolute_and_same_line_music_have_exact_measure_ranges() {
    for (id, n) in [
        ("named-variable", 2),
        ("inline-staff", 2),
        ("absolute", 1),
        ("same-line", 2),
        ("pitch-state", 2),
        ("scheme-markup", 1),
    ] {
        let source = input(id);
        let index = build_index(&source);
        assert_eq!(
            index.capability,
            LocationCapability::Measure,
            "{id}: {index:?}"
        );
        assert_eq!(index.max_measures, n, "{id}");
        assert_eq!(index.staves.len(), 1, "{id}");
        let region = resolve_locator(&index, &locator(0, 1, 1)).unwrap();
        assert_eq!(region.capability, LocationCapability::Measure, "{id}");
        assert_eq!(region.spans.len(), 1, "{id}");
        assert!(SourceMap::new(&source)
            .slice(region.spans[0])
            .unwrap()
            .contains('|'));
        if id == "same-line" {
            let other = resolve_locator(&index, &locator(0, 2, 2)).unwrap();
            assert_eq!(region.spans[0].end.line, other.spans[0].start.line);
            assert!(region.spans[0].byte_end <= other.spans[0].byte_start);
        }
    }
}

#[test]
fn compressed_rest_score_tail_context_ids_and_staff_reuse_are_fixed() {
    let source = input("compressed-rest");
    let index = build_index(&source);
    assert_eq!(index.max_measures, 5);
    assert_eq!(
        index.voices[0]
            .measures
            .iter()
            .map(|m| (m.measure_start, m.measure_end))
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 5)]
    );
    assert!(!SourceMap::new(&source)
        .slice(index.voices[0].span)
        .unwrap()
        .contains("\\score"));
    assert_eq!(
        resolve_locator(&index, &locator(999, 1, 1))
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let mut missing = locator(0, 1, 1);
    missing.voice = Some("missing".into());
    assert_eq!(
        resolve_locator(&index, &missing).unwrap_err().code,
        ErrorCode::NotFound
    );
    assert_eq!(
        resolve_locator(&index, &locator(0, 6, 6))
            .unwrap()
            .capability,
        LocationCapability::None
    );
    let poly = build_index(&input("polyphonic-pages"));
    assert_eq!(
        poly.staves
            .iter()
            .map(|s| s.voices.clone())
            .collect::<Vec<_>>(),
        vec![vec!["Upper", "Second"], vec!["Lower"]]
    );
    let mut wrong = locator(0, 1, 1);
    wrong.voice = Some("Lower".into());
    assert_eq!(
        resolve_locator(&poly, &wrong).unwrap_err().code,
        ErrorCode::NotFound
    );
    let midi = build_index(&input("midi-context-reuse"));
    assert_eq!(midi.staves.len(), 1);
    assert_eq!(
        midi.staves[0].voices,
        vec!["trackBchannelA", "trackBchannelB"]
    );
}

#[test]
fn unsupported_music_never_claims_precise_ranges_or_partial_pitches() {
    for id in ["chordmode", "repeat", "transpose"] {
        let source = input(id);
        let index = build_index(&source);
        assert_eq!(
            index.capability,
            LocationCapability::AnchorOnly,
            "{id}: {index:?}"
        );
        assert!(index.voices.iter().all(|v| v.measures.is_empty()));
        let span = index.voices[0].music_span;
        let result = resolve_pitches(&source, span).unwrap();
        assert!(result.unsupported.is_some(), "{id}");
        assert!(result.events.is_empty(), "{id}");
    }
    for source in [
        "Music = { c'1 d'1 } \\score { \\new Staff \\Music }",
        "Music = { c1 | #(make-music 'NoteEvent) } \\score { \\new Staff \\Music }",
    ] {
        assert_eq!(
            build_index(source).capability,
            LocationCapability::AnchorOnly
        );
    }
}

#[test]
fn pitch_events_match_hand_counted_facts_and_native_p00_midi() {
    for (id, expected) in [
        ("absolute", vec![60, 64, 67]),
        ("named-variable", vec![60, 62]),
        ("pitch-state", vec![60, 64, 67, 65, 67, 60]),
        ("scheme-markup", vec![60]),
    ] {
        let source = input(id);
        let index = build_index(&source);
        let map = SourceMap::new(&source);
        let result = resolve_pitches(&source, index.voices[0].music_span).unwrap();
        assert_eq!(result.unsupported, None, "{id}: {result:?}");
        assert_eq!(
            result
                .events
                .iter()
                .flat_map(|e| e.pitches.clone())
                .collect::<Vec<_>>(),
            expected,
            "{id}"
        );
        for event in &result.events {
            assert_eq!(map.slice(event.span).unwrap(), event.raw, "{id}");
        }
        let midi = parse_midi(&fs::read(root().join(format!("rendered/{id}/score.midi"))).unwrap())
            .unwrap();
        let mut actual: Vec<_> = midi
            .tracks
            .iter()
            .flat_map(|t| t.notes.iter().map(|n| i32::from(n.note)))
            .collect();
        actual.sort();
        let mut predicted = expected;
        predicted.sort();
        assert_eq!(actual, predicted, "{id}");
    }
}

#[test]
fn source_positions_use_scalars_and_probe_verified_lilypond_fields() {
    let source = "\u{feff}\t中文😀\r\n\tc4";
    let map = SourceMap::new(source);
    for (byte, _) in source
        .char_indices()
        .chain(std::iter::once((source.len(), '\0')))
    {
        let p = map.position(byte).unwrap();
        assert_eq!(map.anchor_byte(p.line, p.column - 1), Some(byte));
    }
    assert!(map.position(1).is_err());
    assert!(map.position(source.len() + 1).is_err());
    assert_eq!(map.display_byte(2, 9), Some(source.find("c4").unwrap()));
    assert_eq!(map.display_byte(2, 3), None);
    for (id, char_offset, column) in [
        ("columns-plain", 48, 49),
        ("columns-lf", 48, 50),
        ("columns-bom-crlf", 49, 58),
    ] {
        let source = text(&format!("rendered/{id}/score.ly"));
        let map = SourceMap::new(&source);
        let b = map.anchor_byte(2, char_offset).unwrap();
        assert!(source[b..].starts_with("c4"), "{id}");
        assert_eq!(map.display_byte(2, column), Some(b), "{id}");
    }
}

#[test]
fn midi_fixtures_match_expected_note_ons_and_reject_truncation() {
    let manifest: Value = serde_json::from_str(&text("inputs/manifest.json")).unwrap();
    for f in manifest["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == "midi")
    {
        let parsed = parse_midi(&fs::read(root().join(f["file"].as_str().unwrap())).unwrap());
        if f["expected"]["parse"] == "error" {
            assert_eq!(
                parsed.unwrap_err().code,
                ErrorCode::InvalidInput,
                "{}",
                f["id"]
            );
            continue;
        }
        let midi = parsed.unwrap();
        let actual: Vec<_> = midi
            .tracks
            .iter()
            .flat_map(|t| t.notes.iter().map(|n| json!({"note":n.note,"time":n.time})))
            .collect();
        assert_eq!(json!(actual), f["expected"]["notes"], "{}", f["id"]);
        assert!(diff_midi(&midi, &midi).unwrap().tracks.is_empty());
    }
}

#[test]
fn nested_context_order_is_engraving_order_and_composed_music_is_conservative() {
    let source="Upper = { c'1 | } Lower = { c1 | } \\score { << { \\new Staff \\Upper } { \\new Staff \\Lower } >> }";
    let index = build_index(source);
    assert_eq!(
        index
            .staves
            .iter()
            .map(|s| s.voices.clone())
            .collect::<Vec<_>>(),
        vec![vec!["Upper"], vec!["Lower"]]
    );
    let source = "First = { c'1 | } Music = { \\First d'1 | } \\score { \\new Staff \\Music }";
    let index = build_index(source);
    let v = index.voice("Music").unwrap();
    assert_eq!(v.capability, LocationCapability::AnchorOnly);
    assert!(v.measures.is_empty());
    for source in [
        "Music = { R1 * 4 | % ordinary comment\n } \\score { \\new Staff \\Music }",
        "Music = { c'1 | % 4294967296\n } \\score { \\new Staff \\Music }",
        "Music = { c'1 \\customMusic | } \\score { \\new Staff \\Music }",
    ] {
        assert_eq!(
            build_index(source).capability,
            LocationCapability::AnchorOnly,
            "{source}"
        );
    }
}

#[test]
fn document_note_language_cannot_silently_change_pitch_interpretation() {
    for prefix in ["\\language \"deutsch\"", "\\include \"deutsch.ly\""] {
        let source = format!("{prefix}\nMusic = {{ b'1 | }} \\score {{ \\new Staff \\Music }}");
        let index = build_index(&source);
        let pitch = resolve_pitches(&source, index.voices[0].music_span).unwrap();
        assert!(pitch.unsupported.is_some());
        assert!(pitch.events.is_empty());
    }
}
