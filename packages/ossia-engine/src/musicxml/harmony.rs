use super::events::{Context, Event, Harmony, Id, Pitch, Tick};
use super::tablature::{attach_frames, Frame};
use super::{unsupported, Result};
use crate::RunContext;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Harmonies = BTreeMap<Tick, Vec<(usize, Chord)>>;

pub(super) struct Chord {
    pub root: Option<Pitch>,
    pub kind: &'static str,
    pub bass: Option<Pitch>,
    pub frame: Option<Frame>,
    pub visible: bool,
}

struct Track {
    parent: Id,
    staff: usize,
    chords: BTreeMap<Tick, Chord>,
}

pub(super) fn build_harmonies(
    contexts: &BTreeMap<Id, Context>,
    streams: &BTreeMap<Id, Vec<&Event>>,
    groups: &BTreeMap<Id, Vec<Id>>,
    end: Tick,
    source: &str,
    run: &RunContext,
) -> Result<BTreeMap<Id, Harmonies>> {
    let mut tracks = BTreeMap::<Id, Vec<Track>>::new();
    let mut fretboards = None;
    for context in contexts.values() {
        run.check()?;
        match context.kind.as_str() {
            "ChordNames" => {}
            "FretBoards" if fretboards.is_none() => {
                fretboards = Some(context);
                continue;
            }
            "FretBoards" => return Err(unsupported(source, &[], "multiple FretBoards tracks")),
            _ => continue,
        }

        let (part, staff) = groups
            .iter()
            .find_map(|(&part, staves)| {
                staves.iter().enumerate().find_map(|(index, &staff)| {
                    // A group-level track belongs to the first staff under that group.
                    let mut id = staff;
                    while id != 0 {
                        if id == context.parent {
                            return Some((part, index + 1));
                        }
                        id = contexts[&id].parent;
                    }
                    None
                })
            })
            .ok_or_else(|| unsupported(source, &[], "a harmony track without a target staff"))?;

        let mut chords = BTreeMap::new();
        for event in streams.get(&context.id).into_iter().flatten() {
            let chord = match event.kind.as_str() {
                "harmony" => {
                    let (pitches, bass, visible): Harmony = event.fields()?;
                    let mut chord = classify_chord(&pitches, &bass)
                        .ok_or_else(|| event.fail(source, "this chord symbol"))?;
                    chord.visible = visible;
                    chord
                }
                "no-chord" => {
                    let (visible,): (bool,) = event.fields()?;
                    Chord {
                        root: None,
                        kind: "none",
                        bass: None,
                        frame: None,
                        visible,
                    }
                }
                _ => continue,
            };
            if event.time >= end || chords.insert(event.time, chord).is_some() {
                return Err(event.fail(source, "a harmony without a unique score position"));
            }
        }
        tracks.entry(part).or_default().push(Track {
            parent: context.parent,
            staff,
            chords,
        });
    }

    if let Some(frames) = fretboards {
        let mut matching = tracks
            .values_mut()
            .flatten()
            .filter(|track| track.parent == frames.parent)
            .map(|track| &mut track.chords)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return Err(unsupported(
                source,
                &[],
                "fretboard diagrams without a matching chord track",
            ));
        }
        let events = streams.get(&frames.id).map(Vec::as_slice).unwrap_or(&[]);
        attach_frames(events, &mut matching, source)?;
    }

    Ok(tracks
        .into_iter()
        .map(|(part, tracks)| {
            let mut harmonies = Harmonies::new();
            for track in tracks {
                for (time, chord) in track.chords {
                    harmonies
                        .entry(time)
                        .or_default()
                        .push((track.staff, chord));
                }
            }
            (part, harmonies)
        })
        .collect())
}

pub(super) fn classify_chord(pitches: &[Pitch], bass: &[Pitch]) -> Option<Chord> {
    let root = *pitches.first()?;
    let chromatic = root.chromatic()?;
    let intervals = pitches
        .iter()
        .map(|p| {
            let scale_step =
                (i32::from(p.1) - i32::from(root.1)) * 7 + i32::from(p.0) - i32::from(root.0);
            Some((scale_step, p.chromatic()? - chromatic))
        })
        .collect::<Option<BTreeSet<_>>>()?;
    let intervals = intervals.into_iter().collect::<Vec<_>>();
    let kind = match intervals.as_slice() {
        [(0, 0), (2, 4), (4, 7)] => "major",
        [(0, 0), (2, 3), (4, 7)] => "minor",
        [(0, 0), (2, 4), (4, 8)] => "augmented",
        [(0, 0), (2, 3), (4, 6)] => "diminished",
        [(0, 0), (1, 2), (4, 7)] => "suspended-second",
        [(0, 0), (3, 5), (4, 7)] => "suspended-fourth",
        [(0, 0), (2, 4), (4, 7), (6, 10)] => "dominant",
        [(0, 0), (2, 4), (4, 7), (6, 11)] => "major-seventh",
        [(0, 0), (2, 3), (4, 7), (6, 10)] => "minor-seventh",
        [(0, 0), (2, 3), (4, 7), (6, 11)] => "major-minor",
        [(0, 0), (2, 3), (4, 6), (6, 9)] => "diminished-seventh",
        [(0, 0), (2, 3), (4, 6), (6, 10)] => "half-diminished",
        [(0, 0), (2, 4), (4, 8), (6, 10)] => "augmented-seventh",
        [(0, 0), (2, 4), (4, 7), (5, 9)] => "major-sixth",
        [(0, 0), (2, 3), (4, 7), (5, 9)] => "minor-sixth",
        [(0, 0), (2, 4), (4, 7), (6, 11), (8, 14)] => "major-ninth",
        [(0, 0), (2, 3), (4, 7), (6, 10), (8, 14)] => "minor-ninth",
        [(0, 0), (2, 4), (4, 7), (6, 10), (8, 14), (10, 17)] => "dominant-11th",
        [(0, 0), (4, 7)] => "power",
        _ => return None,
    };
    let bass = match bass {
        [] => None,
        [p] if p.chromatic().is_some() => Some(*p),
        _ => return None,
    };
    Some(Chord {
        root: Some(root),
        kind,
        bass,
        frame: None,
        visible: true,
    })
}
