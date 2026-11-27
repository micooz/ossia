use ossia_engine::{render_wav, RunContext};
use ossia_types::{ContentHash, ErrorCode, PlaybackNote, SampleInstrument, ScorePlayback};
use std::{io::Cursor, path::PathBuf, time::Duration};

fn audio_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/public/audio")
}

fn playback(notes: Vec<PlaybackNote>) -> ScorePlayback {
    let hash = ContentHash::of(b"audio-test");
    ScorePlayback {
        render_key: hash.clone(),
        source_hash: hash,
        duration_seconds: notes
            .iter()
            .map(|note| note.start_seconds + note.duration_seconds)
            .fold(0.0, f64::max),
        notes,
        parts: Vec::new(),
        groups: Vec::new(),
        systems: Vec::new(),
    }
}

#[test]
fn renders_stereo_pcm_wav_with_audible_piano_samples() {
    let timeline = playback(vec![PlaybackNote {
        ornament_anchor: None,
        part_id: "part".into(),
        instrument: SampleInstrument::Piano,
        pitch: 69,
        start_seconds: 0.0,
        duration_seconds: 0.25,
    }]);
    let context = RunContext::default();
    let bytes = render_wav(&timeline, &audio_root(), &context).unwrap();
    let progress = context.progress().unwrap();
    assert_eq!(progress.phase, "audio");
    assert_eq!((progress.completed, progress.total), (Some(100), Some(100)));
    assert!(bytes.starts_with(b"RIFF"));
    assert_eq!(&bytes[8..12], b"WAVE");

    let mut reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(
        reader.spec(),
        hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        }
    );
    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let frames = samples.len() / 2;
    assert!(frames >= 27_000);
    assert!(samples.iter().any(|sample| sample.unsigned_abs() > 0));
}

#[test]
fn renders_each_bundled_instrument_with_its_own_samples() {
    let mut results = Vec::new();
    for instrument in [
        SampleInstrument::Piano,
        SampleInstrument::Guitar,
        SampleInstrument::Violin,
        SampleInstrument::Ukulele,
    ] {
        let timeline = playback(vec![PlaybackNote {
            ornament_anchor: None,
            part_id: "part".into(),
            instrument,
            pitch: 69,
            start_seconds: 0.0,
            duration_seconds: 0.2,
        }]);
        let bytes = render_wav(&timeline, &audio_root(), &RunContext::default()).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(&bytes)).unwrap();
        assert!(reader.samples::<i16>().any(|sample| sample.unwrap() != 0));
        assert!(
            !results.contains(&bytes),
            "{instrument:?} rendered like another bank"
        );
        results.push(bytes);
    }
}

#[test]
fn catalog_gain_scales_rendered_samples() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        audio_root().join("salamander/A4.ogg"),
        dir.path().join("sample.ogg"),
    )
    .unwrap();
    std::fs::write(dir.path().join("manifest.json"), r#"{"instruments":{"piano":{"samples":[{"pitch":69,"ogg":"sample.ogg"}]},"violin":{"gain":2.0,"samples":[{"pitch":69,"ogg":"sample.ogg"}]}}}"#).unwrap();
    let render = |instrument| {
        let bytes = render_wav(
            &playback(vec![PlaybackNote {
                ornament_anchor: None,
                part_id: "part".into(),
                instrument,
                pitch: 69,
                start_seconds: 0.0,
                duration_seconds: 0.2,
            }]),
            dir.path(),
            &RunContext::default(),
        )
        .unwrap();
        hound::WavReader::new(Cursor::new(bytes))
            .unwrap()
            .samples::<i16>()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    };
    let piano = render(SampleInstrument::Piano);
    let violin = render(SampleInstrument::Violin);
    let index = piano
        .iter()
        .position(|sample| sample.unsigned_abs() > 1000)
        .unwrap();
    assert!((i32::from(violin[index]) - 2 * i32::from(piano[index])).abs() <= 1);
}

#[test]
fn rejects_silent_timelines() {
    let error =
        render_wav(&playback(Vec::new()), &audio_root(), &RunContext::default()).unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedSource);
}

#[test]
fn honours_cancellation_while_mixing() {
    let context = RunContext::new(Duration::from_secs(30));
    context.cancellation.cancel();
    let error = render_wav(
        &playback(vec![PlaybackNote {
            ornament_anchor: None,
            part_id: "part".into(),
            instrument: SampleInstrument::Piano,
            pitch: 60,
            start_seconds: 0.0,
            duration_seconds: 1.0,
        }]),
        &audio_root(),
        &context,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
}

#[test]
fn reports_missing_catalog_and_corrupt_sample_paths() {
    let dir = tempfile::tempdir().unwrap();
    let timeline = playback(vec![PlaybackNote {
        ornament_anchor: None,
        part_id: "part".into(),
        instrument: SampleInstrument::Piano,
        pitch: 69,
        start_seconds: 0.0,
        duration_seconds: 0.25,
    }]);
    let error = render_wav(&timeline, dir.path(), &RunContext::default()).unwrap_err();
    assert!(error.message.contains("manifest.json"));

    std::fs::write(
        dir.path().join("manifest.json"),
        r#"{"instruments":{"piano":{"samples":[{"pitch":69,"ogg":"bad.ogg"}]}}}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("bad.ogg"), b"not audio").unwrap();
    let error = render_wav(&timeline, dir.path(), &RunContext::default()).unwrap_err();
    assert!(error.message.contains("bad.ogg"));
}
