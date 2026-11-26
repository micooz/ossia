use super::events::{ticks, Event, Meter, Repeat, Tick};
use super::notes::{Note, Voice};
use super::{invalid_events, unsupported, Result};
use crate::RunContext;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::Excluded;

const MAX_MEASURES: usize = 100_000;

#[derive(Default)]
pub(super) struct Barline {
    pub repeat: Option<u32>,
    pub ending: Option<u32>,
}

pub(super) struct Measure {
    pub from: Tick,
    pub to: Tick,
    pub number: usize,
    pub fragment: usize,
    pub time: Option<(u32, u32)>,
    pub implicit: bool,
    pub left: Barline,
    pub right: Barline,
}

pub(super) fn measure_grid(
    events: &[Event],
    end: Tick,
    scale: i64,
    source: &str,
    run: &RunContext,
) -> Result<Vec<Measure>> {
    let mut changes = BTreeMap::new();
    let mut pickup = 0;
    for event in events.iter().filter(|event| event.kind == "grid") {
        let Meter(position, length, beats, beat_type) = event.fields()?;
        let position = ticks(position, scale)?;
        let length = ticks(length, scale)?;
        if length <= 0 || beats == 0 || beat_type == 0 {
            return Err(event.fail(source, "an invalid meter"));
        }
        if position < 0 {
            if event.time != 0 {
                return Err(event.fail(source, "a pickup inside the score"));
            }
            pickup = -position;
        }
        let from = event
            .time
            .checked_sub(position)
            .ok_or_else(invalid_events)?;
        if from < 0
            || (position > 0
                && changes
                    .get(&from)
                    .is_some_and(|prev| *prev != (length, beats, beat_type)))
        {
            return Err(event.fail(source, "a meter change inside a measure"));
        }
        changes.insert(from, (length, beats, beat_type));
    }

    let mut result = Vec::new();
    let mut from = 0;
    let mut previous_time = None;
    let mut number = usize::from(pickup == 0);
    while from < end {
        if result.len() % 1024 == 0 {
            run.check()?;
        }
        if result.len() >= MAX_MEASURES {
            return Err(unsupported(source, &[], "more than 100,000 measures"));
        }
        let info_at = from.max(pickup);
        let (_, &(length, beats, beat_type)) = changes
            .range(..=info_at)
            .next_back()
            .ok_or_else(|| unsupported(source, &[], "a score without a stable measure grid"))?;
        let nominal_end = if from < pickup {
            pickup
        } else {
            from.checked_add(length).ok_or_else(invalid_events)?
        };
        if changes.range((from + 1)..nominal_end).next().is_some() {
            return Err(unsupported(
                source,
                &[],
                "inconsistent or independent staff measure boundaries",
            ));
        }
        let to = nominal_end.min(end);
        let current_time = (beats, beat_type);
        let time = (previous_time != Some(current_time)).then_some(current_time);
        result.push(Measure {
            from,
            to,
            number,
            fragment: 0,
            time,
            implicit: from < pickup || to < nominal_end,
            left: Barline::default(),
            right: Barline::default(),
        });
        previous_time = Some(current_time);
        from = to;
        number += 1;
    }
    Ok(result)
}

pub(super) fn split_rests(
    measures: &[Measure],
    voice: &mut Voice,
    source: &str,
    events: &[&Event],
) -> Result<()> {
    let mut notes = BTreeMap::new();
    for (from, note) in std::mem::take(&mut voice.notes) {
        let fail = |feature| {
            events
                .iter()
                .find(|event| {
                    event.time == from
                        && matches!(event.kind.as_str(), "note" | "rest" | "measure-rest")
                })
                .map_or_else(
                    || unsupported(source, &[], feature),
                    |event| event.fail(source, feature),
                )
        };
        let end = from.checked_add(note.duration).ok_or_else(invalid_events)?;
        let measure_start = measures.partition_point(|m| m.to <= from);
        let measure = measures
            .get(measure_start)
            .ok_or_else(|| fail("a note outside the measure grid"))?;
        if !note.measure_rest {
            if end > measure.to {
                return Err(fail("a note or rest crossing an unnotated barline"));
            }
            notes.insert(from, note);
            continue;
        }
        if from != measure.from {
            return Err(fail("a measure rest starting inside a measure"));
        }
        for measure in &measures[measure_start..] {
            if measure.to > end {
                return Err(fail("a measure rest ending inside a measure"));
            }
            notes.insert(measure.from, Note::measure_rest(measure.to - measure.from));
            if measure.to == end {
                break;
            }
        }
        if end > measures.last().ok_or_else(invalid_events)?.to {
            return Err(fail("a measure rest outside the measure grid"));
        }
    }
    voice.notes = notes;
    Ok(())
}

pub(super) fn apply_repeats(
    measures: &mut Vec<Measure>,
    events: &[Event],
    source: &str,
) -> Result<()> {
    // On a shared boundary, close the preceding repeat or ending before opening the next one.
    let mut boundaries = BTreeMap::<(Tick, u8), (u32, &Event)>::new();
    for event in events {
        let (kind, number) = match event.kind.as_str() {
            "volta-repeat-start-event" => (2, event.fields::<Repeat>()?.0),
            "volta-repeat-end-event" => (1, event.fields::<Repeat>()?.0),
            "volta-span-event" => {
                let (_, numbers, side): Repeat = event.fields()?;
                let [number @ (1 | 2)] = numbers.as_slice() else {
                    return Err(event.fail(source, "a volta other than first and second endings"));
                };
                (if side == -1 { 3 } else { 0 }, *number)
            }
            _ => continue,
        };
        if boundaries
            .insert((event.time, kind), (number, event))
            .is_some_and(|(old, _)| old != number)
        {
            return Err(event.fail(source, "conflicting repeats across voices"));
        }
    }

    // MusicXML puts repeat and volta marks on barlines. Keep the metrical grid
    // and represent a repeat inside one bar as consecutive partial measures.
    let cuts = boundaries
        .keys()
        .map(|&(time, _)| time)
        .collect::<BTreeSet<_>>();
    if cuts.iter().any(|&time| {
        let index = measures.partition_point(|m| m.to <= time);
        measures.get(index).is_some_and(|m| m.from < time)
    }) {
        let mut fragments = Vec::new();
        for mut measure in std::mem::take(measures) {
            for &cut in cuts.range((Excluded(measure.from), Excluded(measure.to))) {
                fragments.push(Measure { to: cut, ..measure });
                measure = Measure {
                    from: cut,
                    to: measure.to,
                    number: measure.number,
                    fragment: measure.fragment + 1,
                    time: None,
                    implicit: true,
                    left: Barline::default(),
                    right: Barline::default(),
                };
            }
            fragments.push(measure);
            if fragments.len() > MAX_MEASURES {
                return Err(unsupported(source, &[], "more than 100,000 measures"));
            }
        }
        *measures = fragments;
    }

    let mut repeat = None;
    let mut ending = None;
    for ((time, kind), (number, event)) in boundaries {
        let index = measures.partition_point(|m| m.from < time);
        let fail = || event.fail(source, "nested, overlapping, or unaligned repeats");
        if kind >= 2 {
            let measure = measures
                .get_mut(index)
                .filter(|m| m.from == time)
                .ok_or_else(fail)?;
            match kind {
                2 if repeat.is_none() && number > 0 => {
                    repeat = Some(number);
                    measure.left.repeat = Some(number);
                }
                3 if ending.is_none() => {
                    ending = Some(number);
                    measure.left.ending = Some(number);
                }
                _ => return Err(fail()),
            }
        } else {
            let measure = index
                .checked_sub(1)
                .and_then(|i| measures.get_mut(i))
                .filter(|m| m.to == time)
                .ok_or_else(fail)?;
            match kind {
                0 if ending == Some(number) => {
                    ending = None;
                    measure.right.ending = Some(number);
                }
                1 if repeat == Some(number) => {
                    repeat = None;
                    measure.right.repeat = Some(number);
                }
                _ => return Err(fail()),
            }
        }
    }
    if repeat.is_some() || ending.is_some() {
        return Err(unsupported(
            source,
            &[],
            "an incomplete repeat or alternative ending",
        ));
    }
    Ok(())
}
