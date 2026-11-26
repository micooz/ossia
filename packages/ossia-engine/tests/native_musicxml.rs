use ossia_engine::{DocumentId, Engine, ErrorCode, InputFormat, RunContext, SourceSnapshot};
use roxmltree::{Document, Node};
use std::{path::PathBuf, process::Command, time::Duration};

const LEAD_SHEET: &str = include_str!("../../../tests/fixtures/musicxml/chinese-lead-sheet.ly");

fn export(engine: &Engine, source: &str, run: &RunContext) -> Vec<u8> {
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec()).unwrap();
    engine
        .export_musicxml(&snapshot, run)
        .unwrap_or_else(|error| panic!("{error:?}"))
}

fn named<'a, 'input>(node: Node<'a, 'input>, name: &'static str) -> Vec<Node<'a, 'input>> {
    node.children()
        .filter(|child| child.has_tag_name(name))
        .collect()
}

fn value<'a>(node: Node<'a, '_>, name: &str) -> &'a str {
    node.children()
        .find(|child| child.has_tag_name(name))
        .and_then(|child| child.text())
        .unwrap()
}

fn validate_schema(xml: &[u8], base: &std::path::Path, label: &str) {
    let Some(schema) = std::env::var_os("OSSIA_TEST_MUSICXML_XSD") else {
        return;
    };
    let path = base.join(format!("{label}.musicxml"));
    std::fs::write(&path, xml).unwrap();
    let output = Command::new("xmllint")
        .args(["--nonet", "--noout", "--schema"])
        .arg(schema)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn validate_measure_timing(root: Node<'_, '_>, label: &str) {
    for part in named(root, "part") {
        let mut divisions = 0;
        let mut meter = (0, 0);
        let measures = named(part, "measure");
        for (index, &measure) in measures.iter().enumerate() {
            let number = measure.attribute("number").unwrap();
            let mut cursor = 0;
            let mut furthest = 0;
            for item in measure.children().filter(Node::is_element) {
                let duration = || {
                    let duration = value(item, "duration").parse::<i64>().unwrap();
                    assert!(duration > 0, "{label}, measure {number}");
                    duration
                };
                match item.tag_name().name() {
                    "attributes" => {
                        if let Some(node) = named(item, "divisions").first() {
                            divisions = node.text().unwrap().parse::<i64>().unwrap();
                        }
                        if let Some(time) = named(item, "time").first() {
                            meter = (
                                value(*time, "beats").parse::<i64>().unwrap(),
                                value(*time, "beat-type").parse::<i64>().unwrap(),
                            );
                        }
                    }
                    "backup" => cursor -= duration(),
                    "forward" => cursor += duration(),
                    "note" if !named(item, "grace").is_empty() => {
                        assert!(
                            named(item, "duration").is_empty(),
                            "{label}, measure {number}"
                        );
                    }
                    "note" if named(item, "chord").is_empty() => cursor += duration(),
                    _ => {}
                }
                assert!(cursor >= 0, "{label}, measure {number}");
                furthest = furthest.max(cursor);
            }
            assert!(
                divisions > 0 && meter.0 > 0 && meter.1 > 0,
                "{label}, measure {number}"
            );
            let full = 4 * divisions * meter.0 / meter.1;
            assert_eq!(cursor, furthest, "{label}, measure {number}");
            let begins_split_measure = measures
                .get(index + 1)
                .and_then(|next| next.attribute("number"))
                .and_then(|next| next.split_once('.'))
                .is_some_and(|(base, _)| base == number);
            if measure.attribute("implicit") == Some("yes") || begins_split_measure {
                assert!((1..=full).contains(&furthest), "{label}, measure {number}");
            } else {
                assert_eq!(furthest, full, "{label}, measure {number}");
            }
        }
    }
}

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn native_musicxml_exports_editable_notation() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let run = RunContext::new(Duration::from_secs(120));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &run).unwrap();
    for (name, source) in [
        ("lead-sheet", LEAD_SHEET),
        (
            "ensemble",
            r#"\version "2.26.0"
\score { \new StaffGroup <<
  \new Staff \with { instrumentName = "Clarinet" } {
    \transposition bes \time 6/8 \tempo 4. = 56 fis'8\p\< g' a' b' c'' cis''\f |
    \time 2/4 \tempo "Andante" d''4( e'') |
  }
  \new Staff \with { instrumentName = "Bass" } {
    \clef bass r8 d8 g a r4 | d2 |
  }
>> }"#,
        ),
        (
            "piano",
            include_str!("../../../tests/fixtures/playback/piano-prelude.ly"),
        ),
        (
            "cross-staff",
            r#"\version "2.26.0"
\score { \new PianoStaff <<
  \new Staff = "upper" \new Voice {
    \time 2/4
    c'16[ \change Staff = "lower" d'32\p e'32 f'16
          \change Staff = "upper" g'16] a'4 |
    a'4~ \change Staff = "lower" a'4 |
  }
  \new Staff = "lower" { \clef bass c2 | g2 | }
>> }"#,
        ),
        (
            "same-staff-voices",
            include_str!("../../../tests/fixtures/playback/bach-sheep-voices.ly"),
        ),
        (
            "polyphonic-pickup",
            include_str!("../../../tests/fixtures/playback/violin-pickup.ly"),
        ),
        (
            "piano-harmony",
            include_str!("../../../tests/fixtures/playback/majo-chord-names.ly"),
        ),
        (
            "repeat",
            include_str!("../../../tests/fixtures/inputs/repeat.ly"),
        ),
        (
            "mid-measure-repeat",
            r#"\version "2.26.0"
{ \time 4/4
  \repeat volta 2 { \partial 4 c'4 | d'1 | e'2. }
  \repeat volta 2 { f'4 | g'1 | a'2. }
}"#,
        ),
        (
            "sor-original",
            include_str!("../../../tests/fixtures/playback/sor-original.ly"),
        ),
        (
            "selective-chord-tie",
            r#"\version "2.26.0"
\new Staff { << { e'2~ e'2 } { a'2 a'2 } >> }"#,
        ),
        (
            "absolute",
            include_str!("../../../tests/fixtures/inputs/absolute.ly"),
        ),
        (
            "guitar-tab",
            r#"\version "2.26.0"
\include "predefined-guitar-fretboards.ly"
\storePredefinedDiagram #default-fret-table \chordmode { f/a } #guitar-tuning "x;o;3;2;1;1;"
harmony = \chordmode { f1/a b1:m }
\score { <<
  \new ChordNames \harmony
  \new FretBoards \harmony
  \new TabStaff <<
    \new TabVoice { \voiceOne <a' c''>2\arpeggio e'4\2 e'4\1 | r1 }
    \new TabVoice { \voiceTwo a,1\5 | r1 }
  >>
>> }"#,
        ),
        (
            "retuned-tab-and-staff",
            r#"\version "2.26.0"
\new GrandStaff <<
  \new Staff { e,1 | d,1 }
  \new TabStaff {
    e,1\6 |
    \set TabStaff.stringTunings = #guitar-drop-d-tuning
    d,1\6
  }
>>"#,
        ),
        (
            "ukulele-tab",
            r#"\version "2.26.0"
\new TabStaff \with { stringTunings = #ukulele-tuning } { g'1\4 }"#,
        ),
        (
            "grace-notes",
            r#"\version "2.26.0"
{ \time 2/4
  \grace { <c' e'>16[ d'16] } e'4 \acciaccatura fis'16 g'4 |
  \appoggiatura a'8 b'2 |
}"#,
        ),
        (
            "goldberg-aria",
            include_str!("../../../tests/fixtures/playback/bach-goldberg-aria.ly"),
        ),
        (
            "grace-pickup-cross-staff",
            r#"\version "2.26.0"
\new PianoStaff <<
  \new Staff = "upper" \new Voice {
    \partial 4
    \grace { c'16[ \change Staff = "lower" d'] \change Staff = "upper" }
    e'4 | f'1 |
  }
  \new Staff = "lower" { \clef bass \acciaccatura d16 c4 | c1 | }
>>"#,
        ),
        (
            "grace-with-tie-and-tuplet",
            r#"\version "2.26.0"
{
  \tuplet 3/2 { c'8 d' e' } \grace { f'16~ } f'4
  \tuplet 3/2 { \grace { e'16 } d'8 e' f' } g'4 |
}"#,
        ),
        (
            "compressed-rest",
            include_str!("../../../tests/fixtures/inputs/compressed-rest.ly"),
        ),
        (
            "split-score",
            "\\version \"2.26.0\"\n\\score { { c'1 } \\layout {} } \\score { { d'1 } \\midi {} }",
        ),
    ] {
        let bytes = export(&engine, source, &run);
        validate_schema(&bytes, work.path(), name);
        let doc = Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let root = doc.root_element();
        assert!(root.has_tag_name("score-partwise"));
        assert_eq!(root.attribute("version"), Some("4.0"));
        validate_measure_timing(root, name);
        let mut parts = named(root, "part");
        assert!(!parts.is_empty(), "{name}");
        let measures = named(parts.remove(0), "measure");
        assert!(!measures.is_empty(), "{name}");
        match name {
            "mid-measure-repeat" => {
                let measure_lengths = measures
                    .iter()
                    .map(|&m| {
                        named(m, "note")
                            .iter()
                            .map(|&n| value(n, "duration").parse::<i64>().unwrap())
                            .sum::<i64>()
                    })
                    .collect::<Vec<_>>();
                let divisions = value(named(measures[0], "attributes")[0], "divisions")
                    .parse::<i64>()
                    .unwrap();
                assert_eq!(
                    measure_lengths
                        .iter()
                        .map(|l| l / divisions)
                        .collect::<Vec<_>>(),
                    [1, 4, 3, 1, 4, 3]
                );
                assert_eq!(measures[2].attribute("number"), Some("2"));
                assert_eq!(measures[3].attribute("number"), Some("2.1"));
                assert_eq!(measures[3].attribute("implicit"), Some("yes"));
                assert_eq!(measures[4].attribute("number"), Some("3"));
                for (at, direction) in [
                    (0, "forward"),
                    (2, "backward"),
                    (3, "forward"),
                    (5, "backward"),
                ] {
                    assert_eq!(
                        named(named(measures[at], "barline")[0], "repeat")[0]
                            .attribute("direction"),
                        Some(direction)
                    );
                }
            }
            "sor-original" => {
                assert_eq!(measures.len(), 26);
                assert_eq!(parts.len(), 1);
                for staff in [measures, named(parts[0], "measure")] {
                    assert_eq!(staff.len(), 26);
                    assert_eq!(staff[9].attribute("implicit"), Some("yes"));
                    let repeats = staff
                        .iter()
                        .flat_map(|&measure| {
                            named(measure, "barline")
                                .into_iter()
                                .flat_map(|barline| named(barline, "repeat"))
                                .map(move |r| {
                                    (
                                        measure.attribute("number"),
                                        r.attribute("direction"),
                                        r.attribute("times"),
                                    )
                                })
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        repeats,
                        [
                            (Some("0"), Some("forward"), None),
                            (Some("8"), Some("backward"), Some("2")),
                            (Some("8.1"), Some("forward"), None),
                            (Some("24"), Some("backward"), Some("2")),
                        ]
                    );
                    let tied = named(staff[19], "note")
                        .into_iter()
                        .flat_map(|note| {
                            named(note, "tie").into_iter().map(move |tie| {
                                (
                                    value(named(note, "pitch")[0], "step"),
                                    tie.attribute("type"),
                                )
                            })
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(tied, [("E", Some("start")), ("E", Some("stop"))]);
                }
            }
            "selective-chord-tie" => {
                let notes = named(measures[0], "note");
                assert_eq!(notes.len(), 4);
                for note in notes {
                    let step = value(named(note, "pitch")[0], "step");
                    assert_eq!(named(note, "tie").len(), usize::from(step == "E"));
                }
            }
            "guitar-tab" => {
                let attributes = named(measures[0], "attributes")[0];
                assert_eq!(value(named(attributes, "clef")[0], "sign"), "TAB");
                let details = named(attributes, "staff-details")[0];
                assert_eq!(value(details, "staff-lines"), "6");
                let tuning = named(details, "staff-tuning");
                assert_eq!(value(tuning[0], "tuning-step"), "E");
                assert_eq!(value(tuning[0], "tuning-octave"), "2");
                assert_eq!(value(tuning[5], "tuning-octave"), "4");
                let notes = named(measures[0], "note");
                let frets = notes
                    .iter()
                    .map(|&note| {
                        let technical = named(named(note, "notations")[0], "technical")[0];
                        (value(technical, "string"), value(technical, "fret"))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    frets,
                    [("2", "10"), ("1", "8"), ("2", "5"), ("1", "0"), ("5", "0")]
                );
                let harmony = named(measures[0], "harmony")[0];
                assert_eq!(value(named(harmony, "bass")[0], "bass-step"), "A");
                let frame = named(harmony, "frame")[0];
                assert_eq!(value(frame, "frame-strings"), "6");
                let frame_notes = named(frame, "frame-note");
                assert_eq!(frame_notes.len(), 5);
                assert!(frame_notes
                    .iter()
                    .any(|&n| value(n, "string") == "5" && value(n, "fret") == "0"));
                for note in &notes[..2] {
                    assert_eq!(named(named(*note, "notations")[0], "arpeggiate").len(), 1);
                }
                let barre = named(named(measures[1], "harmony")[0], "frame")[0];
                assert_eq!(
                    barre
                        .descendants()
                        .filter(|n| n.has_tag_name("barre"))
                        .count(),
                    2
                );
            }
            "retuned-tab-and-staff" => {
                assert!(parts.is_empty());
                assert_eq!(value(named(measures[0], "attributes")[0], "staves"), "2");
                for (index, measure) in measures.iter().enumerate() {
                    let details = named(named(*measure, "attributes")[0], "staff-details")[0];
                    assert_eq!(details.attribute("number"), Some("2"));
                    let low_string = named(details, "staff-tuning")[0];
                    assert_eq!(value(low_string, "tuning-step"), ["E", "D"][index]);
                    let notes = named(*measure, "note");
                    assert_eq!(value(notes[0], "staff"), "1");
                    assert_eq!(value(notes[1], "staff"), "2");
                    let technical = named(named(notes[1], "notations")[0], "technical")[0];
                    assert_eq!(
                        (value(technical, "string"), value(technical, "fret")),
                        ("6", "0")
                    );
                }
            }
            "ukulele-tab" => {
                let details = named(named(measures[0], "attributes")[0], "staff-details")[0];
                assert_eq!(value(details, "staff-lines"), "4");
                let tuning = named(details, "staff-tuning");
                assert_eq!(value(tuning[0], "tuning-step"), "G");
                assert_eq!(value(tuning[0], "tuning-octave"), "4");
                let note = named(measures[0], "note")[0];
                let technical = named(named(note, "notations")[0], "technical")[0];
                assert_eq!(
                    (value(technical, "string"), value(technical, "fret")),
                    ("4", "0")
                );
            }
            "grace-notes" => {
                let notes = named(measures[0], "note");
                assert_eq!(notes.len(), 6);
                for index in [0, 1, 2, 4] {
                    let note = notes[index];
                    assert_eq!(value(note, "type"), "16th");
                    assert!(named(note, "duration").is_empty());
                    let slash = if index == 4 { "yes" } else { "no" };
                    assert_eq!(named(note, "grace")[0].attribute("slash"), Some(slash));
                }
                assert_eq!(named(notes[1], "chord").len(), 1);
                assert_eq!(named(notes[0], "beam")[0].text(), Some("begin"));
                assert_eq!(named(notes[2], "beam")[0].text(), Some("end"));
                assert_eq!(value(notes[3], "duration"), value(notes[5], "duration"));
                let slur = |note: Node<'_, '_>| {
                    named(named(note, "notations")[0], "slur")[0]
                        .attribute("type")
                        .unwrap()
                        .to_owned()
                };
                assert_eq!(slur(notes[4]), "start");
                assert_eq!(slur(notes[5]), "stop");
                let appoggiatura = named(measures[1], "note")[0];
                assert_eq!(value(appoggiatura, "type"), "eighth");
                assert_eq!(
                    named(appoggiatura, "grace")[0].attribute("slash"),
                    Some("no")
                );
            }
            "goldberg-aria" => {
                assert_eq!(measures.len(), 32);
                let graces = root.descendants().filter(|node| node.has_tag_name("grace"));
                assert_eq!(graces.clone().count(), 27);
                assert!(graces
                    .into_iter()
                    .all(|g| g.attribute("slash") == Some("no")));
            }
            "grace-with-tie-and-tuplet" => {
                let notes = named(measures[0], "note");
                assert_eq!(named(notes[3], "tie")[0].attribute("type"), Some("start"));
                assert_eq!(named(notes[4], "tie")[0].attribute("type"), Some("stop"));
                assert!(named(notes[5], "time-modification").is_empty());
                assert_eq!(named(notes[6], "time-modification").len(), 1);
            }
            "grace-pickup-cross-staff" => {
                assert_eq!(measures[0].attribute("number"), Some("0"));
                assert_eq!(measures[0].attribute("implicit"), Some("yes"));
                let notes = named(measures[0], "note");
                assert_eq!(notes.len(), 5);
                assert_eq!(
                    notes.iter().map(|n| value(*n, "voice")).collect::<Vec<_>>(),
                    ["1", "1", "1", "2", "2"]
                );
                assert_eq!(
                    notes.iter().map(|n| value(*n, "staff")).collect::<Vec<_>>(),
                    ["1", "2", "1", "2", "2"]
                );
                assert_eq!(named(notes[0], "beam")[0].text(), Some("begin"));
                assert_eq!(named(notes[1], "beam")[0].text(), Some("end"));
                assert_eq!(named(notes[3], "grace")[0].attribute("slash"), Some("yes"));
            }
            "cross-staff" => {
                assert!(parts.is_empty());
                assert_eq!(value(named(measures[0], "attributes")[0], "staves"), "2");
                let notes = named(measures[0], "note");
                assert_eq!(
                    notes.iter().map(|n| value(*n, "voice")).collect::<Vec<_>>(),
                    ["1", "1", "1", "1", "1", "1", "2"]
                );
                assert_eq!(
                    notes.iter().map(|n| value(*n, "staff")).collect::<Vec<_>>(),
                    ["1", "2", "2", "2", "1", "1", "2"]
                );
                let primary_beam = notes
                    .iter()
                    .filter_map(|n| {
                        named(*n, "beam")
                            .into_iter()
                            .find(|beam| beam.attribute("number") == Some("1"))
                            .and_then(|beam| beam.text())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    primary_beam,
                    ["begin", "continue", "continue", "continue", "end"]
                );
                let third_beam = |note| {
                    named(note, "beam")
                        .into_iter()
                        .find(|beam| beam.attribute("number") == Some("3"))
                        .and_then(|beam| beam.text())
                };
                assert_eq!(third_beam(notes[1]), Some("begin"));
                assert_eq!(third_beam(notes[2]), Some("end"));
                let direction = named(measures[0], "direction")[0];
                assert_eq!(value(direction, "voice"), "1");
                assert_eq!(value(direction, "staff"), "2");
                let tied = named(measures[1], "note");
                assert_eq!(value(tied[0], "staff"), "1");
                assert_eq!(value(tied[1], "staff"), "2");
                assert_eq!(named(tied[0], "tie")[0].attribute("type"), Some("start"));
                assert_eq!(named(tied[1], "tie")[0].attribute("type"), Some("stop"));
            }
            "lead-sheet" => {
                assert_eq!(value(named(root, "work")[0], "work-title"), "月亮 & 山");
                assert_eq!(measures[0].attribute("number"), Some("0"));
                assert_eq!(measures[0].attribute("implicit"), Some("yes"));
                let attrs = named(measures[0], "attributes")[0];
                assert_eq!(value(attrs, "divisions"), "3");
                assert_eq!(value(named(attrs, "key")[0], "fifths"), "-1");
                assert_eq!(value(named(attrs, "key")[0], "mode"), "minor");
                let harmony = named(measures[1], "harmony")[0];
                assert_eq!(value(named(harmony, "root")[0], "root-step"), "D");
                assert_eq!(value(named(harmony, "bass")[0], "bass-step"), "F");
                assert_eq!(value(harmony, "kind"), "minor");
                let notes = named(measures[1], "note");
                assert_eq!(value(notes[0], "duration"), "1");
                assert_eq!(
                    value(named(notes[0], "time-modification")[0], "actual-notes"),
                    "3"
                );
                assert_eq!(value(named(notes[0], "lyric")[0], "text"), "好");
                assert_eq!(value(named(notes[0], "lyric")[1], "text"), "看");
                assert_eq!(
                    named(named(notes[2], "lyric")[0], "extend")[0].attribute("type"),
                    Some("stop")
                );
                assert_eq!(named(notes[4], "chord").len(), 1);
                assert_eq!(named(notes[3], "tie")[0].attribute("type"), Some("start"));
                assert_eq!(named(notes[5], "tie")[0].attribute("type"), Some("stop"));
                assert_eq!(
                    named(measures[2], "barline")[0]
                        .descendants()
                        .find(|n| n.has_tag_name("ending"))
                        .unwrap()
                        .attribute("number"),
                    Some("1")
                );
                assert_eq!(
                    named(measures[3], "barline")[0]
                        .descendants()
                        .find(|n| n.has_tag_name("ending"))
                        .unwrap()
                        .attribute("number"),
                    Some("2")
                );
            }
            "piano" | "piano-harmony" => {
                assert_eq!(value(named(measures[0], "attributes")[0], "staves"), "2");
                assert!(!named(measures[0], "backup").is_empty(), "{name}");
                assert!(named(measures[0], "note")
                    .iter()
                    .any(|note| value(*note, "staff") == "2"));
                assert!(parts.is_empty());
            }
            "ensemble" => {
                assert_eq!(parts.len(), 1);
                let attrs = named(measures[0], "attributes")[0];
                assert_eq!(value(named(attrs, "transpose")[0], "chromatic"), "-2");
                assert_eq!(value(named(attrs, "time")[0], "beats"), "6");
                assert_eq!(
                    value(
                        named(named(measures[1], "attributes")[0], "time")[0],
                        "beats"
                    ),
                    "2"
                );
                let notes = named(measures[0], "note");
                assert_eq!(value(named(notes[0], "pitch")[0], "step"), "F");
                assert_eq!(value(named(notes[0], "pitch")[0], "alter"), "1");
                assert_eq!(value(named(notes[4], "pitch")[0], "step"), "C");
                assert_eq!(value(named(notes[5], "pitch")[0], "alter"), "1");
                let markers = measures[0]
                    .descendants()
                    .filter(|node| node.has_tag_name("wedge"))
                    .map(|node| node.attribute("type").unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(markers, ["crescendo", "stop"]);
                let tempo = measures[0]
                    .descendants()
                    .find(|node| node.has_tag_name("sound"))
                    .unwrap();
                assert_eq!(tempo.attribute("tempo"), Some("84"));
                let metronome = measures[0]
                    .descendants()
                    .find(|n| n.has_tag_name("metronome"))
                    .unwrap();
                assert_eq!(value(metronome, "beat-unit"), "quarter");
                assert_eq!(named(metronome, "beat-unit-dot").len(), 1);
                assert_eq!(value(metronome, "per-minute"), "56");
                assert!(measures[1]
                    .descendants()
                    .any(|n| n.has_tag_name("words") && n.text() == Some("Andante")));
                let bass_measures = named(parts[0], "measure");
                assert_eq!(named(named(bass_measures[0], "note")[0], "rest").len(), 1);
            }
            "same-staff-voices" => {
                assert_eq!(named(measures[0], "backup").len(), 1);
                assert!(named(measures[0], "note")
                    .iter()
                    .any(|note| value(*note, "voice") == "2"));
            }
            "split-score" => {
                let note = named(measures[0], "note")[0];
                assert_eq!(value(named(note, "pitch")[0], "step"), "C");
            }
            _ => {}
        }
    }

    let imported = engine
        .convert(
            include_bytes!("../../../tests/fixtures/inputs/xml-clean.musicxml"),
            InputFormat::MusicXml,
            None,
            &run,
        )
        .unwrap();
    let source = imported.source.unwrap();
    assert!(source.contains("c1"));
    let mut source = source.replacen("c1", "es1", 1);
    source.push_str("\n\\header { title = \"Edited XML source\" }\n");
    let edited = export(&engine, &source, &run);
    let text = std::str::from_utf8(&edited).unwrap();
    let doc = Document::parse(text).unwrap();
    assert!(text.contains("Edited XML source"));
    let pitch = doc
        .descendants()
        .find(|node| node.has_tag_name("pitch"))
        .unwrap();
    assert_eq!(value(pitch, "step"), "E");
    assert_eq!(value(pitch, "alter"), "-1");
    validate_schema(&edited, work.path(), "edited-import");
}

#[test]
#[ignore = "requires native LilyPond 2.26.0"]
fn native_musicxml_refuses_unsupported_notation_and_respects_context() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let run = RunContext::default();
    let engine = Engine::verify(&root, &work.path().join("scratch"), &run).unwrap();
    for (source, feature) in [
        ("\\score {{ c'1 }} \\score {{ d'1 }}", "one unambiguous"),
        ("\\new DrumStaff \\drummode { bd1 }", "DrumStaff"),
        (
            "{ \\tuplet 3/2 { c'4 \\tuplet 3/2 { d'8 e' f' } g'4 } }",
            "nested tuplets",
        ),
        (
            r#"{ \repeat volta 2 { \repeat volta 2 { c'1 } } }"#,
            "nested volta",
        ),
        (
            r#"\new Staff << { \repeat volta 2 { c'2 } d'2 } \\ { g'1 } >>"#,
            "a note or rest crossing an unnotated barline",
        ),
        (
            r#"\new StaffGroup <<
          \new Staff = "up" \new Voice { c'1 \change Staff = "down" d'1 }
          \new Staff = "down" { s1 s1 }
        >>"#,
            "cross-staff",
        ),
        ("{ \\repeat segno 2 { c'1 } }", "jump"),
        (
            r#"{ \afterGrace 1/1 c'2 { d'16 } e'2 }"#,
            "after-grace notes",
        ),
        (r#"{ \grace { r16 c' } d'1 }"#, "grace rest"),
        (
            r#"\new TabStaff { c,,,1 }"#,
            "without a playable integer fret",
        ),
        (
            r#"\score { <<
              \new ChordNames \chordmode { c1 }
              \new FretBoards \chordmode { g1 }
              \new TabStaff { c'1 }
            >> }"#,
            "matching chord symbol",
        ),
    ] {
        let source = format!("\\version \"2.26.0\"\n{source}\n");
        let snapshot = SourceSnapshot::new(DocumentId::new(), source.into_bytes()).unwrap();
        let error = engine.export_musicxml(&snapshot, &run).unwrap_err();
        assert_eq!(
            error.code,
            ErrorCode::UnsupportedSource,
            "{source}: {error:?}",
            source = snapshot.source()
        );
        assert!(error.message.contains(feature), "{error:?}");
        if feature == "nested tuplets" || feature == "cross-staff" {
            assert!(
                error
                    .detail
                    .as_ref()
                    .and_then(|detail| detail.get("position"))
                    .is_some(),
                "{error:?}"
            );
        }
    }
    let snapshot = SourceSnapshot::new(DocumentId::new(), LEAD_SHEET.as_bytes().to_vec()).unwrap();
    let cancelled = RunContext::default();
    cancelled.cancellation.cancel();
    assert_eq!(
        engine
            .export_musicxml(&snapshot, &cancelled)
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
    let expired = RunContext::new(Duration::ZERO);
    assert_eq!(
        engine
            .export_musicxml(&snapshot, &expired)
            .unwrap_err()
            .detail
            .unwrap()["reason"],
        "timeout"
    );
}
