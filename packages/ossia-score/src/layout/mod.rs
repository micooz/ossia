use crate::{resolve_locator, ScoreIndex, SourceMap};
use ossia_types::{AppError, ContentHash, ErrorCode, LocatedRegion, LocationCapability, Locator};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod svg;
mod systems;

use svg::read_svg;
pub use svg::{path_bbox, BBox, SourceAnchor, SvgElement, SvgObject};
use systems::systems;
pub use systems::{StaffGeometry, SystemGeometry};

pub struct SvgPage<'a> {
    pub page: u32,
    pub svg: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLayout {
    pub page: u32,
    pub view_box: Option<BBox>,
    pub staff_space: f64,
    pub systems: Vec<SystemGeometry>,
    pub elements: Vec<SvgElement>,
    pub objects: Vec<SvgObject>,
    pub capability: LocationCapability,
    pub reason: Option<String>,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureColumn {
    pub page: u32,
    pub system_index: usize,
    pub column_index: usize,
    pub x0: f64,
    pub x1: f64,
    pub measure_start: Option<u32>,
    pub measure_end: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreLayout {
    pub source_hash: ContentHash,
    pub pages: Vec<PageLayout>,
    pub columns: Vec<MeasureColumn>,
    pub capability: LocationCapability,
    pub reason: Option<String>,
}

/// Join all pages to one source index. A failed page never silently renumbers subsequent pages.
pub fn build_layout(index: &ScoreIndex, pages: &[SvgPage<'_>]) -> Result<ScoreLayout, AppError> {
    let mut seen = HashSet::new();
    if pages.iter().any(|p| p.page == 0 || !seen.insert(p.page)) {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            "page numbers must be positive and unique",
        ));
    }
    let mut input: Vec<_> = pages.iter().collect();
    input.sort_by_key(|p| p.page);
    let map = SourceMap::new(index.source());
    let mut result = Vec::new();
    let mut columns = Vec::new();
    let mut evidence = Vec::new();
    let mut occurrences = HashMap::<(u32, u32), usize>::new();
    for p in input {
        let mut page = PageLayout {
            page: p.page,
            view_box: None,
            staff_space: 0.,
            systems: Vec::new(),
            elements: Vec::new(),
            objects: Vec::new(),
            capability: LocationCapability::None,
            reason: None,
            error: None,
        };
        match read_svg(p.svg, &map) {
            Err(error) => {
                page.reason = Some(error.to_string());
                page.error = Some(error);
            }
            Ok((view_box, elements, objects)) => {
                page.view_box = Some(view_box);
                page.elements = elements;
                page.objects = objects;
                match systems(&page.elements, &page.objects, index.staves.len()) {
                    Err(error) => {
                        page.capability = LocationCapability::AnchorOnly;
                        page.reason = Some(error.to_string());
                        page.error = Some(error);
                    }
                    Ok((space, systems)) => {
                        page.staff_space = space;
                        page.systems = systems;
                        page.capability = LocationCapability::Measure;
                        for (system_index, sys) in page.systems.iter().enumerate() {
                            if sys.bar_xs.len() < 2 {
                                page.capability = LocationCapability::AnchorOnly;
                                page.reason = Some("no reliable measure grid".into());
                            }
                            for (column_index, w) in sys.bar_xs.windows(2).enumerate() {
                                // Only rhythmic origins establish a measure; a broken slur
                                // keeps its opening origin even on a later system.
                                let rhythmic = page.objects.iter().any(|o| {
                                    !o.hit_only
                                        && matches!(
                                            o.kind.as_str(),
                                            "NoteHead" | "Rest" | "MultiMeasureRest"
                                        )
                                });
                                let anchors: Vec<_> = if rhythmic {
                                    page.objects
                                        .iter()
                                        .filter(|o| {
                                            !o.hit_only
                                                && matches!(
                                                    o.kind.as_str(),
                                                    "NoteHead" | "Rest" | "MultiMeasureRest"
                                                )
                                        })
                                        .map(|o| (o.bbox, o.span.map(|span| span.byte_start)))
                                        .collect()
                                } else {
                                    page.elements
                                        .iter()
                                        .map(|e| (e.bbox, e.anchor.as_ref().and_then(|a| a.byte)))
                                        .collect()
                                };
                                let candidates: Vec<_> = anchors
                                    .iter()
                                    .filter(|(bbox, _)| {
                                        let (x, y) = bbox.center();
                                        x >= w[0]
                                            && x < w[1]
                                            && y >= sys.top - space * 2.
                                            && y <= sys.bot + space * 2.
                                    })
                                    .filter_map(|(_, byte)| byte.and_then(|b| index.measure_at(b)))
                                    .collect();
                                let candidate = if candidates.is_empty() {
                                    None
                                } else {
                                    let start =
                                        candidates.iter().map(|m| m.measure_start).max().unwrap();
                                    let end =
                                        candidates.iter().map(|m| m.measure_end).min().unwrap();
                                    (start <= end).then_some((start, end))
                                };
                                if let Some(range) = candidate {
                                    *occurrences.entry(range).or_default() += 1;
                                }
                                evidence.push((candidate, !candidates.is_empty()));
                                columns.push(MeasureColumn {
                                    page: p.page,
                                    system_index,
                                    column_index,
                                    x0: w[0],
                                    x1: w[1],
                                    measure_start: None,
                                    measure_end: None,
                                });
                            }
                        }
                    }
                }
            }
        }
        result.push(page);
    }
    let mut used = HashMap::<(u32, u32), u32>::new();
    let mut next = Some(1u32);
    let mut previous_page = 0;
    let mut offset = 0;
    let mut partial = false;
    for page in &mut result {
        if page.page != previous_page + 1 {
            next = None;
            partial = true;
        }
        previous_page = page.page;
        if page.capability != LocationCapability::Measure {
            next = None;
            partial = true;
        }
        let mut previous_system = None;
        for column in columns.iter_mut().filter(|c| c.page == page.page) {
            if previous_system.map_or(column.system_index > 0, |s| column.system_index > s + 1) {
                next = None;
                partial = true;
            }
            previous_system = Some(column.system_index);
            let (candidate, conflicting) = evidence[offset];
            offset += 1;
            let range = match candidate {
                Some((start, end)) if start == end => Some((start, end)),
                Some(range @ (start, end)) => {
                    let count = occurrences[&range];
                    if count == 1 {
                        Some(range)
                    } else if count as u64 == u64::from(end) - u64::from(start) + 1 {
                        let n = used.entry(range).or_default();
                        let measure = start + *n;
                        *n += 1;
                        Some((measure, measure))
                    } else {
                        None
                    }
                }
                None if !conflicting => next.filter(|n| *n <= index.max_measures).map(|n| (n, n)),
                _ => None,
            };
            if let Some((start, end)) = range {
                column.measure_start = Some(start);
                column.measure_end = Some(end);
                next = end.checked_add(1);
            } else {
                next = None;
                page.capability = LocationCapability::AnchorOnly;
                page.reason
                    .get_or_insert("incomplete or ambiguous measure grid".into());
                partial = true;
            }
        }
    }
    // A stem can cover a whole single staff. Identical singleton source measures on both
    // sides prove that the apparent bar is inside a measure, not a new measure boundary.
    let mut merged: Vec<MeasureColumn> = Vec::new();
    for column in columns {
        if let Some(previous) = merged.last_mut().filter(|previous| {
            previous.page == column.page
                && previous.system_index == column.system_index
                && previous.measure_start.is_some()
                && previous.measure_start == previous.measure_end
                && previous.measure_start == column.measure_start
                && previous.measure_end == column.measure_end
        }) {
            previous.x1 = column.x1;
        } else {
            merged.push(column);
        }
    }
    let mut columns = merged;
    let mut group = None;
    let mut local = 0;
    for column in &mut columns {
        let current = (column.page, column.system_index);
        if group != Some(current) {
            group = Some(current);
            local = 0;
        }
        column.column_index = local;
        local += 1;
    }
    let mut expected = Some(1);
    for column in &columns {
        if expected != column.measure_start {
            partial = true;
        }
        expected = column.measure_end.and_then(|n| n.checked_add(1));
    }
    let capability = if columns.iter().any(|c| c.measure_start.is_some()) {
        LocationCapability::Measure
    } else if result.iter().any(|p| !p.elements.is_empty()) {
        LocationCapability::AnchorOnly
    } else {
        LocationCapability::None
    };
    Ok(ScoreLayout {
        source_hash: index.source_hash.clone(),
        pages: result,
        columns,
        capability,
        reason: partial
            .then(|| "partial layout capability; inspect individual pages and columns".into()),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum LocateQuery {
    Point { page: u32, x: f64, y: f64 },
    Region { locator: Locator },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationResult {
    pub capability: LocationCapability,
    pub reason: Option<String>,
    pub region: Option<LocatedRegion>,
    pub page: Option<u32>,
    pub system_index: Option<usize>,
    pub anchors: Vec<SourceAnchor>,
}

fn no_hit(reason: &str) -> LocationResult {
    LocationResult {
        capability: LocationCapability::None,
        reason: Some(reason.into()),
        region: None,
        page: None,
        system_index: None,
        anchors: Vec::new(),
    }
}

/// Point coordinates are SVG user units. No nearest-staff or first-page fallback is performed.
pub fn locate(
    index: &ScoreIndex,
    layout: &ScoreLayout,
    query: &LocateQuery,
) -> Result<LocationResult, AppError> {
    if index.source_hash != layout.source_hash {
        return Err(AppError::new(
            ErrorCode::StaleSelection,
            "layout belongs to another source snapshot",
        ));
    }
    if let LocateQuery::Region { locator } = query {
        let region = resolve_locator(index, locator)?;
        return Ok(LocationResult {
            capability: region.capability,
            reason: region.reason.clone(),
            region: Some(region),
            page: None,
            system_index: None,
            anchors: Vec::new(),
        });
    }
    let LocateQuery::Point { page, x, y } = *query else {
        unreachable!()
    };
    if !x.is_finite() || !y.is_finite() {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            "point must be finite",
        ));
    }
    let Some(p) = layout.pages.iter().find(|p| p.page == page) else {
        return Ok(no_hit("no-hit"));
    };
    if !p.view_box.is_some_and(|b| b.contains(x, y)) {
        return Ok(no_hit(p.reason.as_deref().unwrap_or("no-hit")));
    }
    if p.systems.is_empty() {
        let anchors: Vec<_> = p
            .elements
            .iter()
            .filter(|e| e.bbox.contains(x, y))
            .filter_map(|e| e.anchor.clone())
            .collect();
        if !anchors.is_empty() {
            return Ok(LocationResult {
                capability: LocationCapability::AnchorOnly,
                reason: p.reason.clone(),
                region: None,
                page: Some(page),
                system_index: None,
                anchors,
            });
        }
    }
    for (system_index, sys) in p.systems.iter().enumerate() {
        let Some((staff_index, staff)) = sys.staves.iter().enumerate().find(|(_, s)| {
            y >= s.top - p.staff_space * 1.2
                && y <= s.bot + p.staff_space * 1.2
                && x >= s.x0
                && x <= s.x1
        }) else {
            continue;
        };
        let anchors: Vec<_> = p
            .elements
            .iter()
            .filter(|e| e.bbox.contains(x, y))
            .filter_map(|e| e.anchor.clone())
            .collect();
        if let Some(col) = layout
            .columns
            .iter()
            .find(|c| c.page == page && c.system_index == system_index && x >= c.x0 && x < c.x1)
        {
            if let (Some(start), Some(end)) = (col.measure_start, col.measure_end) {
                let region = resolve_locator(
                    index,
                    &Locator {
                        staff_index,
                        measure_start: start,
                        measure_end: end,
                        voice: None,
                    },
                )?;
                if region.capability == LocationCapability::Measure {
                    return Ok(LocationResult {
                        capability: region.capability,
                        reason: region.reason.clone(),
                        region: Some(region),
                        page: Some(page),
                        system_index: Some(system_index),
                        anchors,
                    });
                }
            }
        }
        let _ = staff;
        if !anchors.is_empty() {
            return Ok(LocationResult {
                capability: LocationCapability::AnchorOnly,
                reason: Some("no reliable measure grid".into()),
                region: None,
                page: Some(page),
                system_index: Some(system_index),
                anchors,
            });
        }
    }
    Ok(no_hit("no-hit"))
}
