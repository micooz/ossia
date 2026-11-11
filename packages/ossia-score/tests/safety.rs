use ossia_score::*;

#[test]
fn arbitrary_utf8_and_deep_structures_round_trip_without_panics() {
    let alphabet = [
        '\0', '\u{feff}', '\r', '\n', '\t', '😀', '中', 'é', '\u{301}', 'c', '4', 'a', ' ', '{',
        '}', '<', '>', '#', '$', '%', '(', ')', '"', '\\', '\'', '|', '=', '*',
    ];
    let mut seed = 73u64;
    for len in 0..300 {
        let mut source = String::new();
        for _ in 0..len {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push(alphabet[(seed >> 32) as usize % alphabet.len()]);
        }
        let tokens = tokenize(&source);
        assert_eq!(serialize(&tokens), source);
        let map = SourceMap::new(&source);
        for token in &tokens {
            let span = map.span(token.byte_start, token.byte_end).unwrap();
            assert_eq!(map.slice(span).unwrap(), token.raw);
        }
        let _ = build_index(&source);
        let _ = resolve_pitches(&source, map.span(0, source.len()).unwrap());
        let _ = parse_midi(source.as_bytes());
        let _ = build_layout(
            &build_index(""),
            &[SvgPage {
                page: 1,
                svg: &source,
            }],
        );
    }
    for source in [
        format!("{}c4{}", "{".repeat(4000), "}".repeat(4000)),
        format!("#({}😀{})", "(".repeat(4000), ")".repeat(4000)),
        format!("%{{{}%}}", "%{".repeat(2000)),
    ] {
        assert_eq!(serialize(&tokenize(&source)), source);
        let _ = build_index(&source);
    }
}

#[test]
fn every_truncation_and_single_byte_mutation_of_a_valid_midi_is_checked() {
    let valid = include_bytes!("../../../tests/fixtures/inputs/midi-running-zero.mid");
    assert!(parse_midi(valid).is_ok());
    for end in 0..valid.len() {
        assert!(parse_midi(&valid[..end]).is_err(), "truncation {end}");
    }
    for i in 0..valid.len() {
        for b in [0, 0x7f, 0x80, 0xff] {
            let mut bytes = valid.to_vec();
            bytes[i] = b;
            let _ = parse_midi(&bytes);
        }
    }
}

#[test]
fn midi_diff_tracks_counts_pitches_times_and_time_bases_are_distinct() {
    let a = parse_midi(include_bytes!(
        "../../../tests/fixtures/inputs/midi-valid.mid"
    ))
    .unwrap();
    let mut b = a.clone();
    b.tracks[0].notes[0].note += 1;
    b.tracks[0].notes[0].time += 1;
    let diff = diff_midi(&a, &b).unwrap();
    assert!(diff.tracks[0].notes[0].pitch_changed);
    assert!(diff.tracks[0].notes[0].start_changed);
    b.tracks[0].notes.clear();
    assert_eq!(diff_midi(&a, &b).unwrap().tracks[0].after_count, 0);
    let mut a = a;
    a.tracks[0].notes[0].time = 96;
    a.division = MidiDivision::Ppqn {
        ticks_per_quarter: 96,
    };
    let mut b = a.clone();
    b.tracks[0].notes[0].time = 192;
    b.division = MidiDivision::Ppqn {
        ticks_per_quarter: 192,
    };
    assert!(diff_midi(&a, &b).unwrap().tracks.is_empty());
    b.division = MidiDivision::Smpte {
        frames_per_second: -24,
        ticks_per_frame: 10,
    };
    assert_eq!(
        diff_midi(&a, &b).unwrap_err().code,
        ErrorCode::UnsupportedSource
    );
    b = a.clone();
    b.format = 2;
    assert!(diff_midi(&a, &b).is_err());
    b = a.clone();
    b.tracks.push(MidiTrack {
        track: 1,
        notes: Vec::new(),
    });
    assert!(diff_midi(&a, &b).unwrap().tracks[0].track_added);
}

#[test]
fn invalid_midi_headers_statuses_vlqs_and_end_markers_are_errors() {
    fn smf(track: &[u8]) -> Vec<u8> {
        let mut bytes = b"MThd\0\0\0\x06\0\0\0\x01\0\x60MTrk".to_vec();
        bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
        bytes.extend_from_slice(track);
        bytes
    }
    for track in [
        &[0, 60, 64][..],
        &[0, 0x90, 60, 0x80],
        &[0x81, 0x81, 0x81, 0x81, 0, 0xff, 0x2f, 0],
        &[0, 0x90, 60, 64],
        &[0, 0xff, 0x2f, 1, 0],
        &[0, 0xff, 0x2f, 0, 0],
        &[0, 0xf0, 5, 0],
        &[0, 0xff, 0x80, 0],
    ] {
        assert_eq!(
            parse_midi(&smf(track)).unwrap_err().code,
            ErrorCode::InvalidInput,
            "{track:?}"
        );
    }
    let mut file = smf(&[0, 0xff, 0x2f, 0]);
    file[12] = 0;
    file[13] = 0;
    assert!(parse_midi(&file).is_err());
}
