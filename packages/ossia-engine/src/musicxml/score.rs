use super::events::{Clef, Event, Fraction, Id, Key, Metadata, Notation, Pitch, Tick};
use super::harmony::{build_harmonies, Harmonies};
use super::measures::{apply_repeats, measure_grid, split_rests, Measure};
use super::notes::{attach_lyrics, build_voice, Voice};
use super::{invalid_events, unsupported, Result};
use crate::RunContext;
use std::collections::BTreeMap;

pub(super) struct Score {
    pub metadata: Metadata,
    pub divisions: Tick,
    pub measures: Vec<Measure>,
    pub parts: Vec<Part>,
    pub tempo: BTreeMap<Tick, Tempo>,
}

pub(super) struct Tempo {
    pub text: String,
    pub metronome: Option<Metronome>,
}

#[derive(serde::Deserialize)]
pub(super) struct Metronome(pub i8, pub u8, pub Fraction, pub Fraction);

pub(super) struct Part {
    pub name: String,
    pub staves: Vec<Staff>,
    pub harmonies: Harmonies,
}

#[derive(Default)]
pub(super) struct Staff {
    pub tablature: BTreeMap<Tick, Vec<Pitch>>,
    pub clefs: BTreeMap<Tick, Clef>,
    pub keys: BTreeMap<Tick, Key>,
    pub transpositions: BTreeMap<Tick, (i64, i64)>,
    pub voices: Vec<Voice>,
    pub dynamics: Vec<(Tick, usize, usize, Direction)>,
}

pub(super) enum Direction {
    Dynamic(String),
    Wedge(&'static str),
}

impl Score {
    pub fn build(notation: Notation, source: &str, run: &RunContext) -> Result<Self> {
        let Notation {
            metadata,
            contexts,
            events,
            scale,
        } = notation;
        let at_end = events
            .iter()
            .filter(|e| e.kind == "end")
            .collect::<Vec<_>>();
        let [end] = at_end.as_slice() else {
            return Err(unsupported(
                source,
                &[],
                "a score without one complete layout translation",
            ));
        };
        let score_id = end.id;
        let mut measures = measure_grid(&events, end.time, scale, source, run)?;
        if measures.is_empty() {
            return Err(unsupported(source, &[], "an empty score"));
        }
        apply_repeats(&mut measures, &events, source)?;

        let mut streams = BTreeMap::<Id, Vec<&Event>>::new();
        for event in &events {
            streams.entry(event.id).or_default().push(event);
        }
        let names = |id| -> Result<String> {
            streams
                .get(&id)
                .into_iter()
                .flatten()
                .find(|event| event.kind == "name")
                .map_or(Ok(String::new()), |event| {
                    let (name,): (String,) = event.fields()?;
                    Ok(name)
                })
        };

        let mut voices = BTreeMap::new();
        let mut lyrics = BTreeMap::new();
        for context in contexts.values() {
            run.check()?;
            let context_events = streams.get(&context.id).map(Vec::as_slice).unwrap_or(&[]);
            match context.kind.as_str() {
                "Voice" | "TabVoice" => {
                    voices.insert(
                        context.id,
                        build_voice(context_events, scale, source, context.kind == "TabVoice")?,
                    );
                }
                "Lyrics" => {
                    lyrics.insert(context.id, context_events.to_vec());
                }
                "Score" | "Staff" | "TabStaff" | "PianoStaff" | "GrandStaff" | "StaffGroup"
                | "ChoirStaff" | "Dynamics" | "NullVoice" | "ChordNames" | "FretBoards" => {}
                kind => return Err(unsupported(source, &[], format_args!("the {kind} context"))),
            }
        }
        attach_lyrics(lyrics, &mut voices, source)?;

        let mut groups = BTreeMap::<Id, Vec<Id>>::new();
        for staff in contexts
            .values()
            .filter(|c| matches!(c.kind.as_str(), "Staff" | "TabStaff"))
        {
            let parent = contexts.get(&staff.parent).ok_or_else(invalid_events)?;
            let key = if matches!(parent.kind.as_str(), "PianoStaff" | "GrandStaff") {
                parent.id
            } else {
                staff.id
            };
            groups.entry(key).or_default().push(staff.id);
        }
        if groups.is_empty() {
            return Err(unsupported(source, &[], "a score without a pitched staff"));
        }

        let mut harmonies = build_harmonies(&contexts, &streams, &groups, end.time, source, run)?;

        let mut parts = Vec::new();
        let mut has_notation = false;
        for (group, staff_ids) in groups {
            let mut staves = Vec::new();
            let is_piano = contexts[&group].kind == "PianoStaff";
            let name = names(group)?;
            let name = if name.trim().is_empty() {
                if is_piano {
                    "Piano".into()
                } else {
                    let label = &contexts[&group].name;
                    if label.is_empty() || label.starts_with('\\') {
                        format!("Part {}", parts.len() + 1)
                    } else {
                        label.clone()
                    }
                }
            } else {
                name
            };
            for (staff_index, &staff_id) in staff_ids.iter().enumerate() {
                let mut staff = Staff::default();
                for event in streams.get(&staff_id).into_iter().flatten() {
                    match event.kind.as_str() {
                        "clef" => {
                            staff.clefs.insert(event.time, event.fields()?);
                        }
                        "key" => {
                            let key: Key = event.fields()?;
                            if key_signature(&key).is_none() {
                                return Err(event.fail(source, "a nonstandard key signature"));
                            }
                            staff.keys.insert(event.time, key);
                        }
                        "transpose" => {
                            let (diatonic, chromatic): (i64, Fraction) = event.fields()?;
                            let chromatic = chromatic.integer().ok_or_else(|| {
                                event.fail(source, "a microtonal instrument transposition")
                            })?;
                            staff
                                .transpositions
                                .insert(event.time, (diatonic, chromatic));
                        }
                        "tab-tuning" => {
                            let (tuning,): (Vec<Pitch>,) = event.fields()?;
                            if tuning.is_empty()
                                || tuning.len() > u8::MAX as usize
                                || tuning
                                    .iter()
                                    .any(|p| p.chromatic().is_none() || !(0..=9).contains(&p.1))
                            {
                                return Err(event.fail(source, "this tablature tuning"));
                            }
                            staff.tablature.insert(event.time, tuning);
                        }
                        _ => {}
                    }
                }
                if contexts[&staff_id].kind == "TabStaff" && !staff.tablature.contains_key(&0) {
                    return Err(unsupported(
                        source,
                        &[],
                        "a tablature staff without initial tuning",
                    ));
                }
                for context in contexts.values().filter(|c| c.parent == staff_id) {
                    if let Some(mut voice) = voices.remove(&context.id) {
                        let number = staff.voices.len() + 1;
                        let context_events =
                            streams.get(&context.id).map(Vec::as_slice).unwrap_or(&[]);
                        for event in context_events.iter().filter(|e| e.kind == "voice-staff") {
                            let (target,): (Id,) = event.fields()?;
                            let index = staff_ids
                                .iter()
                                .position(|&staff| staff == target)
                                .ok_or_else(|| {
                                    event.fail(source, "cross-staff voice changes between parts")
                                })?;
                            voice
                                .staff_changes
                                .insert((event.time, event.grace), index + 1);
                        }
                        staff.dynamics.extend(
                            directions(context_events, end.time)?.into_iter().map(
                                |(time, direction)| {
                                    (
                                        time,
                                        number,
                                        voice.staff_at(time, 0, staff_index + 1),
                                        direction,
                                    )
                                },
                            ),
                        );
                        if !voice.notes.is_empty() || !context_events.is_empty() {
                            split_rests(&measures, &mut voice, source, context_events)?;
                            has_notation |= !voice.notes.is_empty();
                            staff.voices.push(voice);
                        }
                    }
                }
                staves.push(staff);
            }

            // A separate Dynamics context belongs to the first staff in its group.
            for ctx in contexts
                .values()
                .filter(|c| c.parent == group && c.kind == "Dynamics")
            {
                staves[0].dynamics.extend(
                    directions(
                        streams.get(&ctx.id).map(Vec::as_slice).unwrap_or(&[]),
                        end.time,
                    )?
                    .into_iter()
                    .map(|(time, direction)| (time, 1, 1, direction)),
                );
            }
            parts.push(Part {
                name,
                staves,
                harmonies: harmonies.remove(&group).unwrap_or_default(),
            });
        }
        if !voices.is_empty() {
            return Err(unsupported(
                source,
                &[],
                "a voice without a supported staff",
            ));
        }
        if !has_notation {
            return Err(unsupported(source, &[], "a score without notes or rests"));
        }

        let tempo = events
            .iter()
            .filter(|e| e.kind == "tempo")
            .map(|event| {
                let (text, beats): (String, Vec<Metronome>) = event.fields()?;
                if event.id != score_id {
                    return Err(invalid_events());
                }
                let mut beats = beats.into_iter();
                let metronome = beats.next();
                if beats.next().is_some()
                    || metronome.as_ref().is_some_and(|beat| {
                        beat.1 > 8
                            || beat.2 .0 <= 0
                            || beat.2 .1 <= 0
                            || beat.3 .0 <= 0
                            || beat.3 .1 <= 0
                    })
                {
                    return Err(event.fail(source, "this metronome marking"));
                }
                Ok((event.time, Tempo { text, metronome }))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            metadata,
            divisions: scale / 4,
            measures,
            parts,
            tempo,
        })
    }
}

fn directions(events: &[&Event], end: Tick) -> Result<Vec<(Tick, Direction)>> {
    let mut result = Vec::new();
    let mut active = false;
    for event in events {
        match event.kind.as_str() {
            "dynamic" => {
                let (value,): (String,) = event.fields()?;
                if active {
                    result.push((event.time, Direction::Wedge("stop")));
                    active = false;
                }
                result.push((event.time, Direction::Dynamic(value)));
            }
            "crescendo-event" | "decrescendo-event" => {
                let (side,): (i8,) = event.fields()?;
                if side == -1 {
                    if active {
                        result.push((event.time, Direction::Wedge("stop")));
                    }
                    let kind = if event.kind == "crescendo-event" {
                        "crescendo"
                    } else {
                        "diminuendo"
                    };
                    result.push((event.time, Direction::Wedge(kind)));
                    active = true;
                } else if active {
                    result.push((event.time, Direction::Wedge("stop")));
                    active = false;
                }
            }
            _ => {}
        }
    }
    if active {
        result.push((end, Direction::Wedge("stop")));
    }
    Ok(result)
}

pub(super) fn key_signature((tonic, entries): &Key) -> Option<(i64, &'static str)> {
    let mut accidentals = [0; 7];
    for &(step, alteration) in entries {
        *accidentals.get_mut(step as usize)? = alteration.integer()?;
    }
    let fifths = accidentals.iter().sum::<i64>();
    let order = if fifths < 0 {
        [6, 2, 5, 1, 4, 0, 3]
    } else {
        [3, 0, 4, 1, 5, 2, 6]
    };
    for (i, degree) in order.into_iter().enumerate() {
        let expected = if (i as u64) < fifths.unsigned_abs() {
            fifths.signum()
        } else {
            0
        };
        if accidentals[degree] != expected {
            return None;
        }
    }
    let root = (fifths * 7).rem_euclid(12);
    let offset = (tonic.chromatic()? - root).rem_euclid(12);
    let mode = match offset {
        0 => "major",
        2 => "dorian",
        4 => "phrygian",
        5 => "lydian",
        7 => "mixolydian",
        9 => "minor",
        11 => "locrian",
        _ => return None,
    };
    Some((fifths, mode))
}
