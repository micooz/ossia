use super::svg::{BBox, SvgElement, SvgObject};
use crate::AppError;
use serde::{Deserialize, Serialize};

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

pub(super) fn systems(
    elements: &[SvgElement],
    objects: &[SvgObject],
    staff_count: usize,
) -> Result<(f64, Vec<SystemGeometry>), AppError> {
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
    let mut staves = Vec::new();
    for run in runs {
        if run.len() != 5 {
            continue;
        }
        let top = run[0];
        let bot = run[4];
        let ls: Vec<_> = lines
            .iter()
            .filter(|l| (l.bbox.y0 - top).abs() < 1e-3 || (l.bbox.y0 - bot).abs() < 1e-3)
            .collect();
        staves.push(StaffGeometry {
            top,
            bot,
            x0: ls.iter().map(|l| l.bbox.x0).fold(f64::INFINITY, f64::min),
            x1: ls
                .iter()
                .map(|l| l.bbox.x1)
                .fold(f64::NEG_INFINITY, f64::max),
        });
    }
    if staff_count == 0 || staves.is_empty() || staves.len() % staff_count != 0 {
        return Err(AppError::unsupported(
            "source staff mapping does not match SVG",
        ));
    }
    let mut result = Vec::new();
    // Source contexts determine staves per system, removing the ambiguous one-gap/two-staff case.
    for group in staves.chunks(staff_count) {
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
        system.bar_xs = if bars.is_empty() {
            barlines(elements, &system)
        } else {
            sorted_unique(bars)
        };
        let left = group.iter().map(|s| s.x0).fold(f64::INFINITY, f64::min);
        if let Some(first) = system.bar_xs.first().copied() {
            if first - left > staff_space
                && elements.iter().any(|e| {
                    let (x, y) = e.bbox.center();
                    e.anchor.as_ref().is_some_and(|a| a.byte.is_some())
                        && x >= left
                        && x < first
                        && y >= system.top - staff_space * 2.
                        && y <= system.bot + staff_space * 2.
                })
            {
                system.bar_xs.insert(0, left);
            }
        }
        result.push(system);
    }
    Ok((staff_space, result))
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
