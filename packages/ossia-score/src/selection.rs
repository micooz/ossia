use crate::{resolve_locator, BBox, PageLayout, ScoreIndex, ScoreLayout, StaffGeometry, SvgObject};
use ossia_types::{
    AppError, ContentHash, GeometryBounds, GeometryElement, GeometryPage, GeometryRegion,
    LocationCapability, Locator, SelectionRegion, SelectionTarget,
};

fn bounds(b: BBox) -> GeometryBounds {
    GeometryBounds {
        x0: b.x0,
        y0: b.y0,
        x1: b.x1,
        y1: b.y1,
    }
}

fn barline_extent(
    page: &PageLayout,
    staff: &StaffGeometry,
    staff_index: usize,
    x: f64,
) -> Option<(f64, f64)> {
    let at_boundary = |bbox: BBox| {
        (bbox.x0 - x).abs() <= page.staff_space * 0.3
            && bbox.y0 <= staff.top + 0.1
            && bbox.y1 >= staff.bot - 0.1
    };
    let merge = |extent: Option<(f64, f64)>, bbox: BBox| {
        Some(extent.map_or((bbox.x0, bbox.x1), |(left, right)| {
            (left.min(bbox.x0), right.max(bbox.x1))
        }))
    };
    let grouped = page
        .objects
        .iter()
        .filter(|o| {
            o.kind == "BarLine"
                && o.staff_index.is_none_or(|i| i == staff_index)
                && at_boundary(o.bbox)
        })
        .fold(None, |extent, o| merge(extent, o.bbox));
    if grouped.is_some() {
        return grouped;
    }
    page.elements
        .iter()
        .filter(|e| e.kind == "rect" && e.bbox.x1 - e.bbox.x0 <= 0.45 && at_boundary(e.bbox))
        .fold(None, |extent, e| merge(extent, e.bbox))
}

/// Give a source object one identity across all of its rendered occurrences.
pub fn element_target(hash: &ContentHash, region: &SelectionRegion) -> SelectionTarget {
    let key = if region.spans.is_empty() {
        format!(
            "{hash}:{:?}:{:?}",
            region.element_kind, region.insertion_anchor
        )
    } else {
        format!("{hash}:{:?}:{:?}", region.element_kind, region.spans)
    };
    SelectionTarget::Element {
        element_id: ContentHash::of(key.as_bytes()).to_string(),
    }
}

/// Resolve a measure target without involving render geometry.
pub fn measure_selection(
    index: &ScoreIndex,
    locator: &Locator,
) -> Result<SelectionRegion, AppError> {
    let located = resolve_locator(index, locator)?;
    if located.capability != LocationCapability::Measure {
        return Err(AppError::unsupported(
            "selection requires a reliable measure location",
        ));
    }
    Ok(SelectionRegion {
        target: SelectionTarget::Measure {
            locator: locator.clone(),
        },
        label: format!(
            "Staff {}, measures {}–{}",
            locator.staff_index + 1,
            locator.measure_start,
            locator.measure_end
        ),
        element_kind: None,
        spans: located.spans,
        insertion_anchor: None,
        staff_index: Some(locator.staff_index),
        voice: locator.voice.clone(),
        reason: None,
        occurrences: Vec::new(),
    })
}

fn object_region(
    index: &ScoreIndex,
    page: &PageLayout,
    object: &SvgObject,
) -> Option<SelectionRegion> {
    if object.kind == "BarLine" || object.hit_only {
        return None;
    }
    let staff_index = object.staff_index.or_else(|| {
        if object.kind.starts_with("header-") {
            return None;
        }
        let (_, y) = object.bbox.center();
        page.systems
            .iter()
            .flat_map(|s| s.staves.iter().enumerate())
            .min_by(|(_, a), (_, b)| {
                ((a.top + a.bot) / 2. - y)
                    .abs()
                    .total_cmp(&((b.top + b.bot) / 2. - y).abs())
            })
            .map(|(i, _)| i)
    });
    let voice = object
        .span
        .and_then(|span| {
            index.voices.iter().find(|v| {
                span.byte_start >= v.music_span.byte_start && span.byte_end <= v.music_span.byte_end
            })
        })
        .map(|v| v.name.clone());
    let insertion_anchor = if object.span.is_none()
        && matches!(
            object.kind.as_str(),
            "Clef" | "TimeSignature" | "KeySignature"
        ) {
        staff_index.and_then(|i| crate::property_insertion_anchor(index, i))
    } else {
        None
    };
    let spans = object.span.into_iter().collect::<Vec<_>>();
    let label = if let Some(span) = object.span {
        let text = &index.source()[span.byte_start..span.byte_end];
        let excerpt: String = text.chars().take(80).collect();
        format!(
            "{}: {}{}",
            object.kind,
            excerpt,
            if excerpt.len() < text.len() {
                "…"
            } else {
                ""
            }
        )
    } else {
        format!("{} (generated)", object.kind)
    };
    let mut region = SelectionRegion {
        target: SelectionTarget::Element {
            element_id: String::new(),
        },
        label,
        element_kind: Some(object.kind.clone()),
        spans,
        insertion_anchor,
        staff_index,
        voice,
        occurrences: Vec::new(),
        reason: object.span.is_none().then(|| {
            if insertion_anchor.is_some() {
                "Generated context property; insert an explicit command at insertionAnchor".into()
            } else {
                "No reliable source mapping or insertion anchor for this element".into()
            }
        }),
    };
    region.target = element_target(&index.source_hash, &region);
    Some(region)
}

/// Build the shared browser/server hit geometry without changing the logical measure grid.
pub fn selection_geometry(
    index: &ScoreIndex,
    layout: &ScoreLayout,
) -> Result<Vec<GeometryPage>, AppError> {
    if index.source_hash != layout.source_hash {
        return Err(AppError::new(
            ossia_types::ErrorCode::StaleSelection,
            "geometry belongs to another source",
        ));
    }
    let mut pages = Vec::new();
    for page in &layout.pages {
        let elements = page
            .objects
            .iter()
            .filter_map(|object| {
                let region = object_region(index, page, object)?;
                let spanning = matches!(
                    object.kind.as_str(),
                    "Slur" | "PhrasingSlur" | "Hairpin" | "TextSpanner" | "Tie"
                );
                Some(GeometryElement {
                    region,
                    bounds: bounds(object.bbox),
                    priority: u8::from(matches!(
                        object.kind.as_str(),
                        "Slur" | "PhrasingSlur" | "Hairpin" | "TextSpanner" | "Tie"
                    )),
                    hit_bounds: if spanning {
                        page.objects
                            .iter()
                            .filter(|hit| {
                                hit.hit_only
                                    && hit.kind == object.kind
                                    && hit.span == object.span
                                    && hit.staff_index == object.staff_index
                                    && hit.bbox.x1 >= object.bbox.x0
                                    && hit.bbox.x0 <= object.bbox.x1
                                    && hit.bbox.y1 >= object.bbox.y0
                                    && hit.bbox.y0 <= object.bbox.y1
                            })
                            .map(|hit| bounds(hit.bbox))
                            .collect()
                    } else {
                        Vec::new()
                    },
                })
            })
            .collect::<Vec<_>>();
        let mut regions = Vec::new();
        for column in layout.columns.iter().filter(|c| c.page == page.page) {
            let Some(system) = page.systems.get(column.system_index) else {
                continue;
            };
            let (Some(start), Some(end)) = (column.measure_start, column.measure_end) else {
                continue;
            };
            for (staff_index, staff) in system.staves.iter().enumerate() {
                let locator = Locator {
                    staff_index,
                    measure_start: start,
                    measure_end: end,
                    voice: None,
                };
                if resolve_locator(index, &locator)?.capability != LocationCapability::Measure {
                    continue;
                }
                let mut x0 = column.x0.max(staff.x0);
                let mut x1 = column.x1.min(staff.x1);
                // Leave room for the centered outline stroke inside the barline ink.
                let inset = page.staff_space * 0.2;
                if let Some((_, right)) = barline_extent(page, staff, staff_index, column.x0) {
                    x0 = x0.max(right + inset);
                }
                if let Some((left, _)) = barline_extent(page, staff, staff_index, column.x1) {
                    x1 = x1.min(left - inset);
                }
                let on_staff = |object: &&SvgObject| {
                    object.staff_index.is_none_or(|i| i == staff_index)
                        && page
                            .systems
                            .iter()
                            .enumerate()
                            .filter_map(|(i, s)| s.staves.get(staff_index).map(|s| (i, s)))
                            .min_by(|(_, a), (_, b)| {
                                let y = object.bbox.center().1;
                                ((a.top + a.bot) / 2. - y)
                                    .abs()
                                    .total_cmp(&((b.top + b.bot) / 2. - y).abs())
                            })
                            .is_some_and(|(i, _)| i == column.system_index)
                };
                let prefix_end = page
                    .objects
                    .iter()
                    .filter(on_staff)
                    .filter(|o| {
                        matches!(o.kind.as_str(), "Clef" | "KeySignature" | "TimeSignature")
                            && o.bbox.x0 >= x0 - page.staff_space
                            && o.bbox.x1 < x1
                    })
                    .filter(|o| {
                        !page.objects.iter().filter(on_staff).any(|n| {
                            matches!(n.kind.as_str(), "NoteHead" | "Rest" | "MultiMeasureRest")
                                && n.bbox.x0 >= x0
                                && n.bbox.x0 < o.bbox.x0
                        })
                    })
                    .map(|o| o.bbox.x1)
                    .max_by(f64::total_cmp);
                if let Some(prefix) = prefix_end {
                    let first_note = page
                        .objects
                        .iter()
                        .filter(on_staff)
                        .filter(|o| {
                            matches!(o.kind.as_str(), "NoteHead" | "Rest" | "MultiMeasureRest")
                                && o.bbox.x0 >= prefix
                                && o.bbox.x0 < x1
                        })
                        .map(|o| o.bbox.x0)
                        .min_by(f64::total_cmp);
                    x0 = first_note.map_or(prefix, |note| (prefix + note) / 2.);
                }
                regions.push(GeometryRegion {
                    locator,
                    system_index: column.system_index,
                    bounds: GeometryBounds {
                        x0,
                        x1,
                        y0: staff.top,
                        y1: staff.bot,
                    },
                });
            }
        }
        pages.push(GeometryPage {
            page: page.page,
            view_box: page.view_box.map(bounds),
            capability: page.capability,
            reason: page.reason.clone(),
            regions,
            elements,
        });
    }
    let mut occurrences =
        std::collections::HashMap::<String, Vec<ossia_types::SelectionOccurrence>>::new();
    for page in &pages {
        for e in &page.elements {
            if let SelectionTarget::Element { element_id } = &e.region.target {
                occurrences.entry(element_id.clone()).or_default().push(
                    ossia_types::SelectionOccurrence {
                        page: page.page,
                        staff_index: e.region.staff_index,
                        bounds: e.bounds.clone(),
                    },
                );
            }
        }
    }
    for page in &mut pages {
        for element in &mut page.elements {
            if let SelectionTarget::Element { element_id } = &element.region.target {
                element.region.occurrences =
                    occurrences.get(element_id).cloned().unwrap_or_default();
            }
        }
    }
    Ok(pages)
}

/// Pick local symbols before spanning marks and measure whitespace.
pub fn hit_selection(
    index: &ScoreIndex,
    pages: &[GeometryPage],
    page: u32,
    x: f64,
    y: f64,
) -> Result<Option<SelectionRegion>, AppError> {
    if !x.is_finite() || !y.is_finite() {
        return Err(AppError::invalid("point must be finite"));
    }
    let Some(page) = pages.iter().find(|p| p.page == page) else {
        return Ok(None);
    };
    let contains = |b: &GeometryBounds| x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1;
    let area = |b: &GeometryBounds| (b.x1 - b.x0) * (b.y1 - b.y0);
    if let Some(element) = page
        .elements
        .iter()
        .filter(|e| {
            if e.hit_bounds.is_empty() {
                contains(&e.bounds)
            } else {
                e.hit_bounds.iter().any(contains)
            }
        })
        .min_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| area(&a.bounds).total_cmp(&area(&b.bounds)))
        })
    {
        return Ok(Some(element.region.clone()));
    }
    page.regions
        .iter()
        .find(|r| contains(&r.bounds))
        .map(|r| measure_selection(index, &r.locator))
        .transpose()
}
