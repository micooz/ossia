use super::RunContext;
use ossia_types::{AppError, ErrorCode, ScorePlayback};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Component, Path},
};

const OUTPUT_RATE: u32 = 44_100;
const OUTPUT_CHANNELS: u16 = 2;
const MASTER_GAIN: f32 = 0.82;
const RELEASE_SECONDS: f32 = 0.35;
const STOP_TAIL_SECONDS: f32 = 0.02;
const MIN_NOTE_SECONDS: f32 = 0.05;
const CHUNK_FRAMES: usize = 4_096;

#[derive(Deserialize)]
struct Catalog {
    instruments: HashMap<String, Instrument>,
}
#[derive(Deserialize)]
struct Instrument {
    #[serde(default = "default_gain")]
    gain: f32,
    samples: Vec<Sample>,
}
#[derive(Deserialize)]
struct Sample {
    pitch: u8,
    ogg: String,
}

struct DecodedSample {
    rate: u32,
    channels: usize,
    samples: Vec<i16>,
}

struct NotePlan {
    gain: f32,
    sample: usize,
    start_frame: usize,
    hold_frames: usize,
    stop_frames: usize,
    pitch_step: f64,
}

fn default_gain() -> f32 {
    1.0
}

fn audio_error(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::CompileFailed, message)
}

fn pitch_step(pitch: u8, sample_pitch: u8) -> f64 {
    2f64.powf((f64::from(pitch) - f64::from(sample_pitch)) / 12.0)
}

fn decode_sample(name: &str, bytes: &[u8]) -> Result<DecodedSample, AppError> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes))
        .map_err(|error| audio_error(format!("cannot decode sample {name}: {error}")))?;
    let rate = reader.ident_hdr.audio_sample_rate;
    let channels = usize::from(reader.ident_hdr.audio_channels);
    if rate == 0 || channels == 0 {
        return Err(audio_error(format!("invalid sample {name}")));
    }
    let mut samples = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|error| audio_error(format!("cannot decode sample {name}: {error}")))?
    {
        samples.try_reserve(packet.len()).map_err(|_| {
            AppError::new(
                ErrorCode::StorageError,
                format!("not enough memory to decode sample {name}"),
            )
        })?;
        samples.extend_from_slice(&packet);
    }
    if samples.len() < channels {
        return Err(audio_error(format!("sample {name} is empty")));
    }
    Ok(DecodedSample {
        rate,
        channels,
        samples,
    })
}

fn seconds_to_frames(seconds: f64) -> Result<usize, AppError> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(AppError::invalid(
            "audio duration must be finite and non-negative",
        ));
    }
    let frames = (seconds * f64::from(OUTPUT_RATE)).ceil();
    if frames > usize::MAX as f64 {
        return Err(AppError::new(
            ErrorCode::StorageError,
            "WAV output is too large",
        ));
    }
    Ok(frames as usize)
}

fn release_gain(elapsed: f32, hold_seconds: f32) -> f32 {
    if elapsed <= hold_seconds {
        return MASTER_GAIN;
    }
    let release = ((elapsed - hold_seconds) / RELEASE_SECONDS).min(1.0);
    MASTER_GAIN * (0.001 / MASTER_GAIN).powf(release)
}

fn pcm16(value: f32) -> i16 {
    let value = value.clamp(-1.0, 1.0);
    (value * if value < 0.0 { 32_768.0 } else { 32_767.0 }).round() as i16
}

/// Render the browser playback timeline to a 44.1 kHz stereo 16-bit PCM WAV.
pub fn render_wav(
    playback: &ScorePlayback,
    audio_root: &Path,
    context: &RunContext,
) -> Result<Vec<u8>, AppError> {
    if playback.notes.is_empty() {
        return Err(AppError::unsupported(
            "audio output requires at least one sounding note",
        ));
    }

    context.check()?;
    let manifest_path = audio_root.join("manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|e| {
        audio_error(format!(
            "cannot read audio catalog {}: {e}",
            manifest_path.display()
        ))
    })?;
    let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|e| {
        audio_error(format!(
            "invalid audio catalog {}: {e}",
            manifest_path.display()
        ))
    })?;
    let mut sample_names = Vec::<&str>::new();
    let mut plans = Vec::with_capacity(playback.notes.len());
    for note in &playback.notes {
        let instrument = note.instrument.catalog_key();
        let bank = catalog
            .instruments
            .get(instrument)
            .filter(|i| !i.samples.is_empty())
            .ok_or_else(|| {
                audio_error(format!(
                    "{instrument} samples missing in {}",
                    manifest_path.display()
                ))
            })?;
        if !bank.gain.is_finite() || bank.gain <= 0.0 {
            return Err(audio_error(format!("invalid {instrument} sample gain")));
        }
        let entry = bank
            .samples
            .iter()
            .min_by_key(|s| (i16::from(note.pitch) - i16::from(s.pitch)).unsigned_abs())
            .unwrap();
        let name = entry.ogg.as_str();
        let sample_pitch = entry.pitch;
        let sample =
            if let Some(index) = sample_names.iter().position(|candidate| *candidate == name) {
                index
            } else {
                sample_names.push(name);
                sample_names.len() - 1
            };
        let start_frame = seconds_to_frames(note.start_seconds.max(0.0))?;
        let hold_seconds = if note.ornament_anchor.is_some() {
            note.duration_seconds
        } else {
            note.duration_seconds.max(f64::from(MIN_NOTE_SECONDS))
        } as f32;
        let hold_frames = seconds_to_frames(f64::from(hold_seconds))?;
        let stop_frames = seconds_to_frames(f64::from(
            hold_seconds + RELEASE_SECONDS + STOP_TAIL_SECONDS,
        ))?;
        plans.push(NotePlan {
            gain: bank.gain,
            sample,
            start_frame,
            hold_frames,
            stop_frames,
            pitch_step: pitch_step(note.pitch, sample_pitch),
        });
    }

    let mut samples = Vec::with_capacity(sample_names.len());
    for name in sample_names {
        context.check()?;
        let relative = Path::new(name);
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(audio_error(format!("invalid sample path {name}")));
        }
        let path = audio_root.join(relative);
        let bytes = fs::read(&path)
            .map_err(|e| audio_error(format!("cannot read sample {}: {e}", path.display())))?;
        samples.push(decode_sample(&path.display().to_string(), &bytes)?);
    }
    let note_end_seconds = playback
        .notes
        .iter()
        .map(|note| note.start_seconds.max(0.0) + note.duration_seconds.max(0.0))
        .fold(0.0f64, f64::max);
    let duration = playback.duration_seconds.max(note_end_seconds).max(0.0)
        + f64::from(RELEASE_SECONDS + STOP_TAIL_SECONDS);
    let frames = seconds_to_frames(duration)?;
    let data_bytes = frames
        .checked_mul(usize::from(OUTPUT_CHANNELS))
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| AppError::new(ErrorCode::StorageError, "WAV output is too large"))?;
    let capacity = 44usize
        .checked_add(data_bytes)
        .ok_or_else(|| AppError::new(ErrorCode::StorageError, "WAV output is too large"))?;
    let mut output = Vec::new();
    output.try_reserve(capacity).map_err(|_| {
        AppError::new(
            ErrorCode::StorageError,
            "not enough memory to allocate WAV output",
        )
    })?;

    let mut wav = Cursor::new(output);
    let spec = hound::WavSpec {
        channels: OUTPUT_CHANNELS,
        sample_rate: OUTPUT_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::new(&mut wav, spec).map_err(|error| audio_error(error.to_string()))?;
    context.report_progress("audio", Some((0, 100)));
    let mut mixed = vec![0.0f32; CHUNK_FRAMES * usize::from(OUTPUT_CHANNELS)];
    let mut start = 0usize;
    while start < frames {
        context.check()?;
        let count = CHUNK_FRAMES.min(frames - start);
        mixed[..count * 2].fill(0.0);
        for plan in &plans {
            let overlap_start = plan.start_frame.max(start);
            let overlap_end = (plan.start_frame + plan.stop_frames).min(start + count);
            if overlap_start >= overlap_end {
                continue;
            }
            let sample = &samples[plan.sample];
            let sample_frames = sample.samples.len() / sample.channels;
            let step = plan.pitch_step * f64::from(sample.rate) / f64::from(OUTPUT_RATE);
            for frame in overlap_start..overlap_end {
                let local = frame - plan.start_frame;
                let source_position = local as f64 * step;
                let source = source_position as usize;
                if source + 1 >= sample_frames {
                    break;
                }
                let fraction = source_position.fract() as f32;
                let left_index = source * sample.channels;
                let right_index = if sample.channels > 1 {
                    left_index + 1
                } else {
                    left_index
                };
                let next_index = (source + 1) * sample.channels;
                let next_right = if sample.channels > 1 {
                    next_index + 1
                } else {
                    next_index
                };
                let left = (f32::from(sample.samples[left_index]) * (1.0 - fraction)
                    + f32::from(sample.samples[next_index]) * fraction)
                    / 32_768.0;
                let right = (f32::from(sample.samples[right_index]) * (1.0 - fraction)
                    + f32::from(sample.samples[next_right]) * fraction)
                    / 32_768.0;
                let gain = release_gain(
                    local as f32 / OUTPUT_RATE as f32,
                    plan.hold_frames as f32 / OUTPUT_RATE as f32,
                );
                let gain = gain * plan.gain;
                let offset = (frame - start) * 2;
                mixed[offset] += left * gain;
                mixed[offset + 1] += right * gain;
            }
        }
        for sample in &mixed[..count * 2] {
            writer
                .write_sample(pcm16(*sample))
                .map_err(|error| audio_error(error.to_string()))?;
        }
        start += count;
        let percent = ((start as u128 * 100) / frames as u128) as u32;
        context.report_progress("audio", Some((percent, 100)));
    }
    writer
        .finalize()
        .map_err(|error| audio_error(error.to_string()))?;
    Ok(wav.into_inner())
}

#[cfg(test)]
mod tests {
    use super::pitch_step;

    #[test]
    fn pitch_step_matches_equal_temperament() {
        assert_eq!(pitch_step(69, 69), 1.0);
        assert!((pitch_step(70, 69) - 2f64.powf(1.0 / 12.0)).abs() < 1e-12);
        assert!((pitch_step(81, 69) - 2.0).abs() < 1e-12);
        assert!((pitch_step(57, 69) - 0.5).abs() < 1e-12);
    }
}
