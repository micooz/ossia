use super::geometry::first_note_x;
use super::groups::ElementPosition;
use super::timing::measure_times;
use super::MeasureTime;
use crate::ScoreLayout;
use ossia_types::{PlaybackNoteGroup, PlaybackSystemSpan};
use std::collections::HashMap;

pub(super) fn system_spans(
    layout: &ScoreLayout,
    midi: &crate::MidiFile,
    ppqn: u64,
    last_tick: u64,
    pickup: Option<u64>,
    jianpu: bool,
) -> Vec<PlaybackSystemSpan> {
    let Some(mut times) = measure_times(midi, ppqn, last_tick, pickup) else {
        return Vec::new();
    };
    // Jianpu systems are already padded around their engraved ink.
    let padding = if jianpu { 0. } else { 1.2 };
    if layout
        .columns
        .iter()
        .any(|column| column.measure_start.is_none() || column.measure_end.is_none())
    {
        if let Some(last) = times.last_mut() {
            last.end_seconds = last
                .end_seconds
                .min(super::timing::seconds_at(last_tick, ppqn, midi));
        }
        return fallback_spans(layout, &times, padding);
    }
    let by_measure: HashMap<_, _> = times.iter().map(|time| (time.measure, *time)).collect();
    let mut spans = Vec::new();
    for page in &layout.pages {
        for column in layout
            .columns
            .iter()
            .filter(|column| column.page == page.page)
        {
            let (Some(start), Some(end)) = (column.measure_start, column.measure_end) else {
                continue;
            };
            let Some(system) = page.systems.get(column.system_index) else {
                continue;
            };
            let count = end.saturating_sub(start).saturating_add(1);
            if count == 0 {
                continue;
            }
            let start_x = first_note_x(layout, column);
            let width = column.x1 - start_x;
            for measure in start..=end {
                let Some(time) = by_measure.get(&measure) else {
                    continue;
                };
                let offset = measure.saturating_sub(start);
                let x0 = start_x + width * f64::from(offset) / f64::from(count);
                let x1 = start_x + width * f64::from(offset + 1) / f64::from(count);
                spans.push(PlaybackSystemSpan {
                    page: page.page,
                    system_index: column.system_index,
                    measure_start: measure,
                    measure_end: measure,
                    start_seconds: time.start_seconds,
                    end_seconds: time.end_seconds,
                    x0,
                    x1,
                    y0: system.top - page.staff_space * padding,
                    y1: system.bot + page.staff_space * padding,
                });
            }
        }
    }
    spans.sort_by(|a, b| {
        a.start_seconds
            .total_cmp(&b.start_seconds)
            .then_with(|| a.page.cmp(&b.page))
            .then_with(|| a.system_index.cmp(&b.system_index))
    });
    spans
}

/// Keep the playhead available when source measure indexing is incomplete.
///
/// Engraved columns remain the visual coordinate system. Their order maps onto
/// the performance measure timeline, so tempo and meter changes still determine
/// wall-clock movement instead of assuming columns have equal seconds.
fn fallback_spans(
    layout: &ScoreLayout,
    times: &[MeasureTime],
    padding: f64,
) -> Vec<PlaybackSystemSpan> {
    let columns = layout
        .columns
        .iter()
        .filter(|column| column.x1 > column.x0)
        .collect::<Vec<_>>();
    if columns.is_empty() || times.is_empty() {
        return Vec::new();
    }
    let count = columns.len() as f64;
    let measures = times.len() as f64;
    let mut spans = Vec::with_capacity(columns.len());
    for (index, column) in columns.iter().enumerate() {
        let Some(page) = layout.pages.iter().find(|page| page.page == column.page) else {
            continue;
        };
        let Some(system) = page.systems.get(column.system_index) else {
            continue;
        };
        let start = seconds_at_measure_position(times, measures * index as f64 / count);
        let end = seconds_at_measure_position(times, measures * (index + 1) as f64 / count);
        let x0 = first_note_x(layout, column);
        spans.push(PlaybackSystemSpan {
            page: page.page,
            system_index: column.system_index,
            measure_start: u32::try_from(index + 1).unwrap_or(u32::MAX),
            measure_end: u32::try_from(index + 1).unwrap_or(u32::MAX),
            start_seconds: start,
            end_seconds: end,
            x0,
            x1: column.x1,
            y0: system.top - page.staff_space * padding,
            y1: system.bot + page.staff_space * padding,
        });
    }
    spans
}

fn seconds_at_measure_position(times: &[MeasureTime], position: f64) -> f64 {
    let last = times.len() as f64;
    if position <= 0.0 {
        return times[0].start_seconds;
    }
    if position >= last {
        return times[times.len() - 1].end_seconds;
    }
    let index = position.floor() as usize;
    let progress = position - index as f64;
    let time = times[index];
    time.start_seconds + (time.end_seconds - time.start_seconds) * progress
}

pub(super) fn refine_system_spans(
    layout: &ScoreLayout,
    groups: &[PlaybackNoteGroup],
    positions: &HashMap<String, ElementPosition>,
    provisional: Vec<PlaybackSystemSpan>,
) -> Vec<PlaybackSystemSpan> {
    if provisional.is_empty()
        || layout
            .columns
            .iter()
            .all(|column| column.measure_start.is_some() && column.measure_end.is_some())
    {
        return provisional;
    }
    let columns = layout
        .columns
        .iter()
        .filter(|column| column.x1 > column.x0)
        .collect::<Vec<_>>();
    if columns.len() != provisional.len() {
        return provisional;
    }
    let mut starts = vec![None::<f64>; columns.len()];
    let mut ends = vec![None::<f64>; columns.len()];
    for group in groups {
        let Some(start) = group.start_seconds else {
            continue;
        };
        let Some(position) = group
            .element_ids
            .iter()
            .find_map(|element_id| positions.get(element_id))
        else {
            continue;
        };
        let Some(index) = columns
            .iter()
            .position(|column| {
                column.page == position.page
                    && column.system_index == position.system_index
                    && position.x >= column.x0
                    && position.x < column.x1
            })
            .or_else(|| {
                columns
                    .iter()
                    .enumerate()
                    .filter(|(_, column)| {
                        column.page == position.page && column.system_index == position.system_index
                    })
                    .min_by(|(_, left), (_, right)| {
                        interval_distance(position.x, left.x0, left.x1)
                            .total_cmp(&interval_distance(position.x, right.x0, right.x1))
                    })
                    .map(|(index, _)| index)
            })
        else {
            continue;
        };
        starts[index] = Some(starts[index].map_or(start, |value| value.min(start)));
        let end = start + group.duration_seconds.unwrap_or(0.0);
        ends[index] = Some(ends[index].map_or(end, |value| value.max(end)));
    }
    for index in 0..starts.len() {
        let start = starts[index].unwrap_or(provisional[index].start_seconds);
        if index > 0 {
            let previous = starts[index - 1].unwrap_or(provisional[index - 1].start_seconds);
            starts[index] = Some(start.max(previous));
        } else {
            starts[index] = Some(start);
        }
    }
    let mut last_end = provisional
        .last()
        .map(|span| span.end_seconds)
        .unwrap_or(0.0);
    for end in ends.into_iter().flatten() {
        last_end = last_end.max(end);
    }
    let mut refined = provisional;
    for index in 0..refined.len() {
        let start = starts[index].unwrap_or(refined[index].start_seconds);
        let end = starts
            .get(index + 1)
            .and_then(|value| *value)
            .unwrap_or(last_end)
            .max(start + 0.001);
        refined[index].start_seconds = start;
        refined[index].end_seconds = end;
    }
    refined
}

fn interval_distance(value: f64, start: f64, end: f64) -> f64 {
    if value < start {
        start - value
    } else if value > end {
        value - end
    } else {
        0.
    }
}
