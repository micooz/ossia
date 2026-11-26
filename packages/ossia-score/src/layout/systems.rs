use super::svg::{BBox, SvgElement, SvgObject};
use crate::{AppError, StaffKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffGeometry {
    pub top: f64,
    pub bot: f64,
    pub x0: f64,
    pub x1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemGeometry {
    pub top: f64,
    pub bot: f64,
    pub staves: Vec<StaffGeometry>,
    pub bar_xs: Vec<f64>,
}

fn rounded(n: f64, d: f64) -> f64 {
    (n * d).round() / d
}

fn sorted_unique(mut n: Vec<f64>) -> Vec<f64> {
    n.sort_by(f64::total_cmp);
    n.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    n
}

fn ruled_staves(
    elements: &[SvgElement],
    staff_kinds: &[StaffKind],
) -> Result<(f64, Vec<StaffGeometry>), AppError> {
    let lines: Vec<_> = elements
        .iter()
        .filter(|e| {
            e.kind == "line" && (e.bbox.y1 - e.bbox.y0).abs() < 1e-6 && e.bbox.x1 - e.bbox.x0 > 5.
        })
        .collect();
    let ys = sorted_unique(lines.iter().map(|e| rounded(e.bbox.y0, 1000.)).collect());
    if ys.len() < 5 {
        return Err(AppError::unsupported("no five-line staff geometry"));
    }
    let gaps: Vec<_> = ys
        .windows(2)
        .map(|w| rounded(w[1] - w[0], 1000.))
        .filter(|n| *n > 0.)
        .collect();
    let unique = sorted_unique(gaps.clone());
    let staff_space = *unique
        .iter()
        .max_by(|a, b| {
            let ac = gaps.iter().filter(|g| (**g - **a).abs() < 1e-6).count();
            let bc = gaps.iter().filter(|g| (**g - **b).abs() < 1e-6).count();
            ac.cmp(&bc).then_with(|| b.total_cmp(a))
        })
        .ok_or_else(|| AppError::unsupported("staff spacing unavailable"))?;
    let mut runs: Vec<Vec<f64>> = vec![vec![ys[0]]];
    for &y in &ys[1..] {
        if y - runs.last().unwrap().last().unwrap() >= staff_space * 1.6 {
            runs.push(Vec::new());
        }
        runs.last_mut().unwrap().push(y);
    }
    let matches = |kind: StaffKind, lines: usize| match kind {
        StaffKind::Notation => lines == 5,
        StaffKind::Tablature => (3..=8).contains(&lines),
    };
    let mut staves = Vec::new();
    for run in runs {
        if !staff_kinds.iter().any(|kind| matches(*kind, run.len())) {
            continue;
        }
        let line_count = run.len();
        let top = run[0];
        let bot = run[line_count - 1];
        let ls: Vec<_> = lines
            .iter()
            .filter(|l| (l.bbox.y0 - top).abs() < 1e-3 || (l.bbox.y0 - bot).abs() < 1e-3)
            .collect();
        staves.push((
            line_count,
            StaffGeometry {
                top,
                bot,
                x0: ls.iter().map(|l| l.bbox.x0).fold(f64::INFINITY, f64::min),
                x1: ls
                    .iter()
                    .map(|l| l.bbox.x1)
                    .fold(f64::NEG_INFINITY, f64::max),
            },
        ));
    }
    if !staves.chunks(staff_kinds.len().max(1)).all(|group| {
        group.len() == staff_kinds.len()
            && group
                .iter()
                .zip(staff_kinds)
                .all(|((lines, _), kind)| matches(*kind, *lines))
    }) {
        return Err(AppError::unsupported(
            "source staff mapping does not match SVG",
        ));
    }
    Ok((
        staff_space,
        staves.into_iter().map(|(_, staff)| staff).collect(),
    ))
}

pub(super) fn normalize_jianpu_staff_indices(objects: &mut [SvgObject], staff_count: usize) {
    // ChordNames and other non-pitched contexts can take an interaction staff ID.
    // Only the pitched staves contribute numbers, rests or measure boundaries.
    let mut pitched: Vec<_> = objects
        .iter()
        .filter(|object| {
            !object.hit_only && matches!(object.kind.as_str(), "NoteHead" | "Rest" | "BarLine")
        })
        .filter_map(|object| object.staff_index)
        .collect();
    pitched.sort_unstable();
    pitched.dedup();
    if pitched.len() == staff_count {
        for object in objects {
            object.staff_index = object
                .staff_index
                .and_then(|staff| pitched.binary_search(&staff).ok());
        }
    }
}

pub(super) fn systems(
    elements: &[SvgElement],
    objects: &[SvgObject],
    staff_kinds: &[StaffKind],
    jianpu: bool,
) -> Result<(f64, Vec<SystemGeometry>), AppError> {
    let (staff_space, staves) = if jianpu {
        let mut staves: Vec<_> = objects
            .iter()
            .filter(|object| object.kind == "JianpuStaff" && !object.hit_only)
            .map(|object| StaffGeometry {
                top: object.bbox.y0,
                bot: object.bbox.y1,
                x0: object.bbox.x0,
                x1: object.bbox.x1,
            })
            .collect();
        staves.sort_by(|a, b| a.top.total_cmp(&b.top));
        // LilyPond emits coordinates in staff spaces.
        (1., staves)
    } else {
        ruled_staves(elements, staff_kinds)?
    };
    if staff_kinds.is_empty() || staves.is_empty() || staves.len() % staff_kinds.len() != 0 {
        return Err(AppError::unsupported(
            "source staff mapping does not match SVG",
        ));
    }
    let mut result = Vec::new();
    let has_rhythmic_objects = objects.iter().any(|object| {
        !object.hit_only
            && matches!(
                object.kind.as_str(),
                "NoteHead" | "TabNoteHead" | "Rest" | "MultiMeasureRest"
            )
    });
    let rhythmic_systems = if has_rhythmic_objects {
        let mut marked = HashSet::new();
        for object in objects.iter().filter(|object| {
            !object.hit_only
                && matches!(
                    object.kind.as_str(),
                    "NoteHead" | "TabNoteHead" | "Rest" | "MultiMeasureRest"
                )
        }) {
            let y = object.bbox.center().1;
            let mut closest = None;
            for (system_index, group) in staves.chunks(staff_kinds.len()).enumerate() {
                // Engraving staff ids are not guaranteed to be local zero-based indices.
                let distance = match object.staff_index.and_then(|index| group.get(index)) {
                    Some(staff) => Some(vertical_distance(staff, y)),
                    None => group
                        .iter()
                        .map(|staff| vertical_distance(staff, y))
                        .min_by(f64::total_cmp),
                };
                if let Some(distance) = distance {
                    if closest.is_none_or(|(_, closest_distance)| distance < closest_distance) {
                        closest = Some((system_index, distance));
                    }
                }
            }
            if let Some((system_index, _)) = closest {
                marked.insert(system_index);
            }
        }
        marked
    } else {
        HashSet::new()
    };
    // Source contexts determine staves per system, removing the ambiguous one-gap/two-staff case.
    for (system_index, group) in staves.chunks(staff_kinds.len()).enumerate() {
        let mut system = SystemGeometry {
            top: group[0].top,
            bot: group.last().unwrap().bot,
            staves: group.to_vec(),
            bar_xs: Vec::new(),
        };
        let bars: Vec<_> = objects
            .iter()
            .filter(|o| o.kind == "BarLine" && o.bbox.y1 >= system.top && o.bbox.y0 <= system.bot)
            .map(|o| o.bbox.x0)
            .collect();
        if has_rhythmic_objects && !rhythmic_systems.contains(&system_index) {
            continue;
        }
        system.bar_xs = if bars.is_empty() {
            barlines(elements, &system)
        } else {
            sorted_unique(bars)
        };
        let left = system
            .staves
            .iter()
            .map(|s| s.x0)
            .fold(f64::INFINITY, f64::min);
        if let Some(first) = system.bar_xs.first().copied() {
            if first - left > staff_space
                && (elements.iter().any(|e| {
                    let (x, y) = e.bbox.center();
                    e.anchor.as_ref().is_some_and(|a| a.byte.is_some())
                        && x >= left
                        && x < first
                        && y >= system.top - staff_space * 2.
                        && y <= system.bot + staff_space * 2.
                }) || (jianpu
                    && objects.iter().any(|object| {
                        let (x, y) = object.bbox.center();
                        object.note.is_some()
                            && object.span.is_some()
                            && x >= left
                            && x < first
                            && y >= system.top
                            && y <= system.bot
                    })))
            {
                system.bar_xs.insert(0, left);
            }
        }
        if let Some(last) = system.bar_xs.last().copied() {
            let right = system
                .staves
                .iter()
                .map(|s| s.x1)
                .fold(f64::NEG_INFINITY, f64::max);
            // A final partial measure can end with the staff rather than a barline.
            if right - last > staff_space
                && objects.iter().any(|object| {
                    let (x, y) = object.bbox.center();
                    object.span.is_some()
                        && matches!(
                            object.kind.as_str(),
                            "NoteHead" | "TabNoteHead" | "Rest" | "MultiMeasureRest"
                        )
                        && x > last + staff_space * 0.5
                        && x < right
                        && system
                            .staves
                            .iter()
                            .any(|staff| vertical_distance(staff, y) <= staff_space * 2.)
                })
            {
                system.bar_xs.push(right);
            }
        }
        result.push(system);
    }
    if jianpu {
        fit_jianpu_staves(&mut result, objects, staff_space);
    }
    Ok((staff_space, result))
}

fn fit_jianpu_staves(systems: &mut [SystemGeometry], objects: &[SvgObject], space: f64) {
    // Staff markers identify each row, but their height covers the largest chord
    // anywhere in the score. Use the engraved bounds of this system instead.
    #[derive(Clone, Default)]
    struct StaffInk {
        music: Option<(f64, f64)>,
        bars: Option<(f64, f64)>,
    }
    let mut ink = systems
        .iter()
        .map(|system| vec![StaffInk::default(); system.staves.len()])
        .collect::<Vec<_>>();
    for object in objects.iter().filter(|object| {
        !object.hit_only
            && matches!(
                object.kind.as_str(),
                "BarLine"
                    | "NoteHead"
                    | "Rest"
                    | "Slur"
                    | "PhrasingSlur"
                    | "Tie"
                    | "Script"
                    | "Arpeggio"
            )
    }) {
        let Some(staff_index) = object.staff_index else {
            continue;
        };
        let (x, y) = object.bbox.center();
        let system = systems
            .iter()
            .enumerate()
            .filter_map(|(i, system)| {
                let staff = system.staves.get(staff_index)?;
                (x >= staff.x0 && x <= staff.x1)
                    .then_some((i, (y - (staff.top + staff.bot) / 2.).abs()))
            })
            .min_by(|(_, a), (_, b)| a.total_cmp(b));
        if let Some((i, _)) = system {
            let staff = &mut ink[i][staff_index];
            let bounds = if object.kind == "BarLine" {
                &mut staff.bars
            } else {
                &mut staff.music
            };
            let (top, bot) = bounds.unwrap_or((object.bbox.y0, object.bbox.y1));
            *bounds = Some((top.min(object.bbox.y0), bot.max(object.bbox.y1)));
        }
    }
    for (system, ink) in systems.iter_mut().zip(ink) {
        // Bar heights can reflect voices used only on other systems.
        let ink: Vec<_> = ink
            .into_iter()
            .map(|staff| staff.music.or(staff.bars))
            .collect();
        let padding = space * 0.5;
        for (i, staff) in system.staves.iter_mut().enumerate() {
            if let Some((top, bot)) = ink[i] {
                let top_padding = i
                    .checked_sub(1)
                    .and_then(|previous| ink[previous])
                    .map_or(padding, |(_, above)| {
                        padding.min((top - above).max(0.) / 2.)
                    });
                let bot_padding = ink
                    .get(i + 1)
                    .and_then(|next| *next)
                    .map_or(padding, |(below, _)| {
                        padding.min((below - bot).max(0.) / 2.)
                    });
                staff.top = top - top_padding;
                staff.bot = bot + bot_padding;
            }
        }
        system.top = system
            .staves
            .iter()
            .map(|s| s.top)
            .fold(f64::INFINITY, f64::min);
        system.bot = system
            .staves
            .iter()
            .map(|s| s.bot)
            .fold(f64::NEG_INFINITY, f64::max);
    }
}

fn barlines(elements: &[SvgElement], system: &SystemGeometry) -> Vec<f64> {
    let left = system
        .staves
        .iter()
        .map(|s| s.x0)
        .fold(f64::INFINITY, f64::min);
    let right = system
        .staves
        .iter()
        .map(|s| s.x1)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut rects: Vec<_> = elements
        .iter()
        .filter(|e| {
            e.kind == "rect"
                && e.bbox.x1 - e.bbox.x0 <= 0.45
                && e.bbox.x0 >= left - 1.
                && e.bbox.x0 <= right + 1.
        })
        .collect();
    rects.sort_by(|a, b| a.bbox.x0.total_cmp(&b.bbox.x0));
    let mut clusters: Vec<(f64, Vec<BBox>)> = Vec::new();
    for rect in rects {
        if let Some((x, boxes)) = clusters
            .iter_mut()
            .find(|(x, _)| (*x - rect.bbox.x0).abs() <= 0.25)
        {
            boxes.push(rect.bbox);
            *x = (*x * (boxes.len() - 1) as f64 + rect.bbox.x0) / boxes.len() as f64;
        } else {
            clusters.push((rect.bbox.x0, vec![rect.bbox]));
        }
    }
    clusters
        .into_iter()
        .filter(|(_, boxes)| {
            system.staves.iter().all(|s| {
                let mut intervals: Vec<_> = boxes
                    .iter()
                    .map(|b| (b.y0.max(s.top - 0.5), b.y1.min(s.bot + 0.5)))
                    .filter(|(a, b)| b > a)
                    .collect();
                intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut merged: Vec<(f64, f64)> = Vec::new();
                for (a, b) in intervals {
                    if let Some(last) = merged.last_mut().filter(|last| a <= last.1 + 0.5) {
                        last.1 = last.1.max(b);
                    } else {
                        merged.push((a, b));
                    }
                }
                merged.iter().map(|(a, b)| b - a).sum::<f64>() >= (s.bot - s.top) * 0.85
            })
        })
        .map(|(x, _)| rounded(x, 100.))
        .collect()
}

fn vertical_distance(staff: &StaffGeometry, y: f64) -> f64 {
    if y < staff.top {
        staff.top - y
    } else if y > staff.bot {
        y - staff.bot
    } else {
        0.
    }
}

#[cfg(test)]
#[path = "../../tests/unit/layout_systems.rs"]
mod tests;
