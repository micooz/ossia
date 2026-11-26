use crate::{MidiFile, ScoreIndex, ScoreLayout};
use ossia_types::{AppError, ContentHash, ErrorCode, MeasureRange, PlaybackNote, ScorePlayback};
use std::ops::Range;

mod engraved;
mod fallback;
mod fermatas;
mod geometry;
mod graces;
mod groups;
mod matching;
mod ornaments;
mod parts;
mod source;
mod systems;
mod timing;

use fermatas::Fermatas;
use groups::{note_groups, NoteGroupBuild};
use systems::system_spans;
use timing::{ppqn, seconds_at};

const ONSET_EPSILON_SECONDS: f64 = 0.001;

#[derive(Debug, Clone)]
struct SourcePlaybackMember {
    span: ossia_types::SourceSpan,
    pitch: u8,
}

#[derive(Debug, Clone)]
struct SourcePlaybackEvent {
    span: ossia_types::SourceSpan,
    staff_index: usize,
    voice: String,
    kind: crate::PitchEventKind,
    pitches: Vec<u8>,
    members: Vec<SourcePlaybackMember>,
    measure_start: Option<u32>,
    measure_end: Option<u32>,
    muted: bool,
}

#[derive(Debug, Clone)]
struct MidiCluster {
    start_tick: u64,
    duration_tick: u64,
    pitches: Vec<u8>,
    measure: Option<u32>,
    track: usize,
    channel: u8,
}

#[derive(Debug, Clone, Copy)]
struct MeasureTime {
    measure: u32,
    start_seconds: f64,
    end_seconds: f64,
}

fn unsupported(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::UnsupportedSource, message)
}

/// Resolve MIDI measure boundaries in the expressive playback timeline.
pub fn playback_measure_bounds(
    index: &ScoreIndex,
    layout: &ScoreLayout,
    midi: &MidiFile,
    range: MeasureRange,
) -> Result<Range<f64>, AppError> {
    let bounds = crate::midi_measure_bounds(midi, range)?;
    let ppqn = ppqn(midi)?;
    let fermatas = Fermatas::from_layout(index, layout, midi, ppqn);
    let time = |tick: u64| {
        let shifted = if tick == 0 {
            tick
        } else {
            tick.checked_add(fermatas.origin_tick)
                .ok_or_else(|| unsupported("playback measure range overflow"))?
        };
        Ok::<_, AppError>(fermatas.time(seconds_at(shifted, ppqn, midi)))
    };
    Ok(time(bounds.start_tick)?..time(bounds.end_tick)?)
}

/// Build playback data from one immutable score render.
pub fn build_playback(
    index: &ScoreIndex,
    layout: &ScoreLayout,
    midi: &MidiFile,
    render_key: ContentHash,
) -> Result<ScorePlayback, AppError> {
    if index.source_hash != layout.source_hash {
        return Err(AppError::new(
            ErrorCode::StaleSelection,
            "playback layout belongs to another source",
        ));
    }
    let ppqn = ppqn(midi)?;
    let fermatas = Fermatas::from_layout(index, layout, midi, ppqn);
    let programs = crate::instruments::ProgramTimeline::new(midi);
    let mut notes = Vec::new();
    for (track_index, track) in midi.tracks.iter().enumerate() {
        for note in &track.notes {
            let duration = note
                .duration
                .ok_or_else(|| unsupported("playback requires note-off timing"))?;
            notes.push((
                PlaybackNote {
                    pitch: note.note,
                    start_seconds: seconds_at(note.time, ppqn, midi),
                    duration_seconds: seconds_at(note.time + duration, ppqn, midi)
                        - seconds_at(note.time, ppqn, midi),
                    part_id: format!("{track_index}:{}", note.channel),
                    ornament_anchor: None,
                    instrument: crate::select_sample_instrument(
                        track.name.as_deref(),
                        (note.channel != 9)
                            .then(|| programs.at(note.channel, note.time).0)
                            .flatten(),
                    ),
                },
                note.time,
                track_index,
                note.channel,
            ));
        }
    }
    notes.sort_by(|a, b| {
        a.0.start_seconds
            .total_cmp(&b.0.start_seconds)
            .then_with(|| a.0.pitch.cmp(&b.0.pitch))
    });
    let last_tick = midi
        .tracks
        .iter()
        .flat_map(|track| &track.notes)
        .map(|note| note.time + note.duration.unwrap_or(0))
        .chain([fermatas.last_release_tick])
        .max()
        .unwrap_or(0);
    let systems = system_spans(
        layout,
        midi,
        ppqn,
        last_tick,
        timing::pickup_ticks(index.source(), ppqn),
        crate::is_jianpu_source(index.source()),
    );
    let NoteGroupBuild {
        mut groups,
        muted_notes,
        element_positions,
        pages,
    } = note_groups(index, layout, midi, ppqn, &systems)?;
    let mut systems =
        systems::refine_system_spans(layout, groups.as_slice(), &element_positions, systems);
    notes.retain(|(note, tick, track, channel)| {
        !muted_notes.iter().any(|muted| {
            note.pitch == muted.pitch
                && tick == &muted.tick
                && track == &muted.track
                && channel == &muted.channel
        })
    });
    let parts = parts::assign_parts(index, midi, &programs, &mut notes, &mut groups);
    let mut notes = ornaments::expand(index, layout, midi, ppqn, &pages, &mut groups, notes);
    fermatas.apply(&mut notes, &mut groups, &mut systems);
    notes.sort_by(|a, b| {
        a.start_seconds
            .total_cmp(&b.start_seconds)
            .then_with(|| a.pitch.cmp(&b.pitch))
            .then_with(|| a.part_id.cmp(&b.part_id))
            .then_with(|| a.ornament_anchor.cmp(&b.ornament_anchor))
    });
    notes.dedup_by(|left, right| {
        left.part_id == right.part_id
            && left.pitch == right.pitch
            && left.ornament_anchor == right.ornament_anchor
            && (left.start_seconds - right.start_seconds).abs() <= ONSET_EPSILON_SECONDS
            && (left.duration_seconds - right.duration_seconds).abs() <= ONSET_EPSILON_SECONDS
    });
    let duration_seconds =
        notes
            .iter()
            .map(|note| note.start_seconds + note.duration_seconds)
            .chain(systems.iter().map(|span| span.end_seconds))
            .chain(groups.iter().filter_map(|group| {
                Some(group.start_seconds? + group.duration_seconds.unwrap_or(0.))
            }))
            .chain(fermatas.end())
            .fold(0.0, f64::max);
    Ok(ScorePlayback {
        render_key,
        source_hash: index.source_hash.clone(),
        duration_seconds,
        parts,
        notes,
        groups,
        systems,
    })
}
