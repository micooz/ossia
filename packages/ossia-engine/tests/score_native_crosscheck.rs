use ossia_score::*;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "ossia-p01-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT pointing to native LilyPond 2.26.0"]
fn native_226_cross_checks_pitch_states_and_compressed_svg() {
    let toolchain = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let binary = toolchain.join(if cfg!(windows) {
        "bin/lilypond.exe"
    } else {
        "bin/lilypond"
    });
    let version = Command::new(&binary).arg("--version").output().unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).unwrap();
    assert!(version.lines().next().unwrap().contains("2.26.0"));
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/inputs/pitch-layout");
    let mut evidence = Vec::new();
    for id in [
        "absolute",
        "nested-modes",
        "markup-chords",
        "compressed-render",
    ] {
        let source = fs::read_to_string(fixtures.join(format!("{id}.ly"))).unwrap();
        let scratch = Scratch::new();
        fs::write(scratch.0.join("score.ly"), &source).unwrap();
        let log = fs::File::create(scratch.0.join("engine.log")).unwrap();
        let mut child = Command::new(&binary)
            .current_dir(&scratch.0)
            .args([
                "-dno-strip-output-dir",
                "-dpoint-and-click=#t",
                "--svg",
                "-o",
                "score",
                "score.ly",
            ])
            .env("HOME", &scratch.0)
            .env("XDG_CACHE_HOME", scratch.0.join("cache"))
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("native engine timed out: {id}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "{id}: {}",
            fs::read_to_string(scratch.0.join("engine.log")).unwrap()
        );
        assert_eq!(
            fs::read(scratch.0.join("score.ly")).unwrap(),
            source.as_bytes()
        );
        let midi_bytes = fs::read(scratch.0.join("score.midi")).unwrap();
        let midi = parse_midi(&midi_bytes).unwrap();
        let index = build_index(&source);
        let pitch = resolve_pitches(&source, index.voices[0].music_span).unwrap();
        assert_eq!(pitch.unsupported, None, "{id}: {pitch:?}");
        let predicted: Vec<_> = pitch
            .events
            .iter()
            .flat_map(|e| e.pitches.iter().copied())
            .collect();
        let actual: Vec<_> = midi
            .tracks
            .iter()
            .flat_map(|t| t.notes.iter().map(|n| i32::from(n.note)))
            .collect();
        // Chord member order in SMF is not part of the pitch contract.
        if id == "nested-modes" {
            assert_eq!(actual, predicted, "nested modes preserve event order");
        }
        let mut a = actual.clone();
        let mut p = predicted.clone();
        a.sort();
        p.sort();
        assert_eq!(a, p, "{id}");
        let svg = fs::read_to_string(scratch.0.join("score.svg")).unwrap();
        let layout = build_layout(&index, &[SvgPage { page: 1, svg: &svg }]).unwrap();
        assert!(
            layout.pages[0].reason.is_none(),
            "{id}: {:?}",
            layout.pages[0].reason
        );
        if id == "compressed-render" {
            assert_eq!(
                layout
                    .columns
                    .iter()
                    .map(|c| (c.measure_start, c.measure_end))
                    .collect::<Vec<_>>(),
                vec![(Some(1), Some(1)), (Some(2), Some(5))]
            );
        } else {
            assert_eq!(
                layout
                    .columns
                    .iter()
                    .map(|c| (c.measure_start, c.measure_end))
                    .collect::<Vec<_>>(),
                vec![(Some(1), Some(1)), (Some(2), Some(2))],
                "{id}"
            );
        }
        evidence.push(json!({"id":id,"sourceHash":index.source_hash,"midiHash":ContentHash::of(&midi_bytes),"predictedPitches":predicted,"nativePitches":actual,"measureColumns":layout.columns,"sourceUnchanged":true}));
    }
    if let Some(file) = std::env::var_os("OSSIA_NATIVE_EVIDENCE") {
        let report = json!({"engineVersion":version.lines().next(),"target":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),"cases":evidence});
        fs::write(
            file,
            format!("{}\n", serde_json::to_string_pretty(&report).unwrap()),
        )
        .unwrap();
    }
}
