use crate::{resolve_locator, BBox, PageLayout, ScoreIndex, ScoreLayout, StaffGeometry, SvgObject};
use ossia_types::{
    AppError, ContentHash, GeometryBounds, GeometryElement, GeometryPage, GeometryRegion,
    LocationCapability, Locator, SelectionRegion, SelectionTarget, SourceSpan,
};
use std::collections::HashMap;

pub(crate) mod engraved;

/// Browser hit areas and the source evidence needed to resolve engraved measures.
#[derive(Debug)]
pub struct SelectionGeometry {
    pub pages: Vec<GeometryPage>,
    source_hash: ContentHash,
    engraved: engraved::EngravedMeasures,
}

impl std::ops::Deref for SelectionGeometry {
    type Target = [GeometryPage];

    fn deref(&self) -> &Self::Target {
        &self.pages
    }
}

impl SelectionGeometry {
    /// Whether measure selection is grounded in this exact render.
    pub fn has_engraved_measures(&self) -> bool {
        !self.engraved.is_empty()
    }

    /// Resolve a selection against the same source and measure grid used for hit testing.
    pub fn resolve_measure(
        &self,
        index: &ScoreIndex,
        locator: &Locator,
    ) -> Result<SelectionRegion, AppError> {
        if self.source_hash != index.source_hash {
            return Err(AppError::new(
                ossia_types::ErrorCode::StaleSelection,
                "geometry belongs to another source",
            ));
        }
        if self.engraved.is_empty() {
            return measure_selection(index, locator);
        }
        if locator.measure_start == 0 || locator.measure_end < locator.measure_start {
            return Err(AppError::invalid("invalid measure range"));
        }
        if locator.voice.is_some() {
            return Err(AppError::unsupported(
                "engraved measures require a staff target",
            ));
        }
        let range = (locator.staff_index, locator.measure_start)
            ..=(locator.staff_index, locator.measure_end);
        let mut spans = Vec::new();
        let mut count = 0;
        for (_, sources) in self.engraved.range(range) {
            spans.extend_from_slice(sources);
            count += 1;
        }
        if count != locator.measure_end - locator.measure_start + 1 {
            return Err(AppError::unsupported(
                "selection requires a reliable measure location",
            ));
        }
        spans.sort_unstable_by_key(|span| (span.byte_start, span.byte_end));
        spans.dedup();
        Ok(measure_region(locator, spans))
    }
}

fn bounds(b: BBox) -> GeometryBounds {
    GeometryBounds {
        x0: b.x0,
        y0: b.y0,
        x1: b.x1,
        y1: b.y1,
    }
}

fn element_system(
    index: &ScoreIndex,
    layout: &ScoreLayout,
    page: &PageLayout,
    object: &SvgObject,
    region: &SelectionRegion,
) -> Option<usize> {
    let (x, _) = object.bbox.center();
    let source_measure = object
        .span
        .and_then(|span| index.measure_at(span.byte_start));
    if let Some(measure) = source_measure {
        let measures = if index.rendered_measure_sequence().is_empty() {
            vec![measure.measure_start]
        } else {
            index
                .rendered_measure_sequence()
                .iter()
                .enumerate()
                .filter_map(|(i, rendered)| {
                    rendered
                        .measures
                        .iter()
                        .any(|item| {
                            Some(item.voice.as_str()) == region.voice.as_deref()
                                && item.measure == measure.measure_start
                        })
                        .then(|| u32::try_from(i + 1).ok())
                        .flatten()
                })
                .collect::<Vec<_>>()
        };
        // Source staff indices can include contexts absent from the visual staves.
        let mut systems = layout
            .columns
            .iter()
            .filter(|column| {
                column.page == page.page
                    && x >= column.x0
                    && x <= column.x1
                    && column
                        .measure_start
                        .zip(column.measure_end)
                        .is_some_and(|(a, b)| {
                            measures
                                .iter()
                                .any(|measure| *measure >= a && *measure <= b)
                        })
            })
            .map(|column| column.system_index);
        if let Some(first) = systems.next() {
            return systems.all(|system| system == first).then_some(first);
        }
    }
    let (x, y) = object.bbox.center();
    if page
        .systems
        .iter()
        .filter(|system| {
            system
                .staves
                .iter()
                .any(|staff| x >= staff.x0 && x <= staff.x1 && y >= staff.top && y <= staff.bot)
        })
        .count()
        > 1
    {
        return None;
    }
    let mut containing = page.systems.iter().enumerate().filter(|(_, system)| {
        system
            .staves
            .iter()
            .any(|staff| x >= staff.x0 && x <= staff.x1)
            && y >= system.top
            && y <= system.bot
    });
    if let Some((system_index, _)) = containing.next() {
        if containing.next().is_none() {
            return Some(system_index);
        }
    }
    // A note inside its uniquely identified source staff needs no measure estimate.
    if let Some(voice) = region.voice.as_deref() {
        let mut staves = index.staves.iter().enumerate().filter(|(_, staff)| {
            staff.voices.iter().any(|name| name == voice)
                && match object.kind.as_str() {
                    "NoteHead" => staff.kind == crate::StaffKind::Notation,
                    "TabNoteHead" => staff.kind == crate::StaffKind::Tablature,
                    _ => true,
                }
        });
        if let Some((staff_index, _)) = staves.next() {
            if staves.next().is_none() {
                let (x, y) = object.bbox.center();
                // Ledger-line notes can lie beyond the staff's ink bounds. Compare
                // only the same source staff in each system, retaining ambiguous ties.
                let mut systems = page
                    .systems
                    .iter()
                    .enumerate()
                    .filter_map(|(i, system)| {
                        let staff = system.staves.get(staff_index)?;
                        if x < staff.x0 || x > staff.x1 {
                            return None;
                        }
                        Some((i, (staff.top - y).max(y - staff.bot).max(0.)))
                    })
                    .collect::<Vec<_>>();
                systems.sort_by(|left, right| left.1.total_cmp(&right.1));
                if let Some((system_index, distance)) = systems.first() {
                    if systems
                        .get(1)
                        .is_none_or(|(_, other)| *other > distance + 1e-6)
                    {
                        return Some(*system_index);
                    }
                    return None;
                }
            }
        }
    }
    let [_system] = page.systems.as_slice() else {
        return None;
    };
    Some(0)
}

fn barline_extent(
    page: &PageLayout,
    staff: &StaffGeometry,
    staff_index: usize,
    x: f64,
    jianpu: bool,
) -> Option<(f64, f64)> {
    let at_boundary = |bbox: BBox| {
        (bbox.x0 - x).abs() <= page.staff_space * 0.3
            && if jianpu {
                bbox.y0 < staff.bot && bbox.y1 > staff.top
            } else {
                bbox.y0 <= staff.top + 0.1 && bbox.y1 >= staff.bot - 0.1
            }
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
    if jianpu || grouped.is_some() {
        return grouped;
    }
    page.elements
        .iter()
        .filter(|e| e.kind == "rect" && e.bbox.x1 - e.bbox.x0 <= 0.45 && at_boundary(e.bbox))
        .fold(None, |extent, e| merge(extent, e.bbox))
}

/// Give a source object one identity across all of its rendered occurrences.
pub fn element_target(hash: &ContentHash, region: &SelectionRegion) -> SelectionTarget {
    let staff = region
        .staff_index
        .map_or_else(|| "none".into(), |index| index.to_string());
    let key = if region.spans.is_empty() {
        format!(
            "{hash}:{staff}:{:?}:{:?}",
            region.element_kind, region.insertion_anchor
        )
    } else {
        format!(
            "{hash}:{staff}:{:?}:{:?}",
            region.element_kind, region.spans
        )
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
    Ok(measure_region(locator, located.spans))
}

fn measure_region(locator: &Locator, spans: Vec<SourceSpan>) -> SelectionRegion {
    SelectionRegion {
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
        spans,
        insertion_anchor: None,
        staff_index: Some(locator.staff_index),
        voice: locator.voice.clone(),
        reason: None,
        occurrences: Vec::new(),
    }
}

fn object_region(
    index: &ScoreIndex,
    page: &PageLayout,
    object: &SvgObject,
    instrument_spans: &HashMap<String, Option<SourceSpan>>,
) -> Option<SelectionRegion> {
    if matches!(
        object.kind.as_str(),
        "JianpuStaff"
            | "BarLine"
            | "Stem"
            | "Beam"
            | "Flag"
            | "Dots"
            | "Accidental"
            | "AccidentalCautionary"
            | "StemTremolo"
    ) || object.hit_only
    {
        return None;
    }
    let geometric_staff = if object.kind.starts_with("header-") {
        None
    } else {
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
    };
    let staff_index = if matches!(
        object.kind.as_str(),
        "Clef" | "KeySignature" | "TimeSignature"
    ) {
        geometric_staff.or(object.staff_index)
    } else {
        object.staff_index.or(geometric_staff)
    };
    let source_span = object
        .span
        .or_else(|| instrument_spans.get(&object.kind).copied().flatten());
    let voice = source_span
        .and_then(|span| {
            index.voices.iter().find(|v| {
                span.byte_start >= v.music_span.byte_start && span.byte_end <= v.music_span.byte_end
            })
        })
        .map(|v| v.name.clone());
    let insertion_anchor = if source_span.is_none()
        && matches!(
            object.kind.as_str(),
            "Clef" | "TimeSignature" | "KeySignature" | "InstrumentName" | "ShortInstrumentName"
        ) {
        staff_index.and_then(|i| crate::property_insertion_anchor(index, i))
    } else {
        None
    };
    let spans = source_span.into_iter().collect::<Vec<_>>();
    let label = if let Some(span) = source_span {
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
        reason: source_span.is_none().then(|| {
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
) -> Result<SelectionGeometry, AppError> {
    if index.source_hash != layout.source_hash {
        return Err(AppError::new(
            ossia_types::ErrorCode::StaleSelection,
            "geometry belongs to another source",
        ));
    }
    // LilyPond can omit group instrument-name origins; fill them only from a uniquely defined set command.
    let mut instrument_spans = HashMap::<String, Option<SourceSpan>>::new();
    for element in crate::source_elements(index.source()) {
        if matches!(
            element.kind.as_str(),
            "InstrumentName" | "ShortInstrumentName"
        ) {
            instrument_spans
                .entry(element.kind)
                .and_modify(|span| *span = None)
                .or_insert(Some(element.span));
        }
    }
    let engraved = engraved::measures(index, layout).unwrap_or_default();
    let jianpu = crate::is_jianpu_source(index.source());
    let mut pages = Vec::new();
    for page in &layout.pages {
        let elements = page
            .objects
            .iter()
            .filter_map(|object| {
                let region = object_region(index, page, object, &instrument_spans)?;
                let spanning = matches!(
                    object.kind.as_str(),
                    "Slur" | "PhrasingSlur" | "Hairpin" | "TextSpanner" | "Tie"
                );
                Some(GeometryElement {
                    bounds: bounds(object.bbox),
                    system_index: element_system(index, layout, page, object, &region),
                    graphic_ids: object.graphic_id.iter().cloned().collect(),
                    linked_targets: vec![region.target.clone()],
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
                    region,
                })
            })
            .collect::<Vec<_>>();
        let mut regions = Vec::new();
        for (ordinal, column) in layout
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.page == page.page)
        {
            let Some(system) = page.systems.get(column.system_index) else {
                continue;
            };
            let range = if engraved.is_empty() {
                column.measure_start.zip(column.measure_end)
            } else {
                u32::try_from(ordinal + 1)
                    .ok()
                    .map(|number| (number, number))
            };
            let Some((start, end)) = range else {
                continue;
            };
            for (staff_index, staff) in system.staves.iter().enumerate() {
                let locator = Locator {
                    staff_index,
                    measure_start: start,
                    measure_end: end,
                    voice: None,
                };
                if engraved.is_empty()
                    && resolve_locator(index, &locator)?.capability != LocationCapability::Measure
                {
                    continue;
                }
                let mut x0 = column.x0.max(staff.x0);
                let mut x1 = column.x1.min(staff.x1);
                if let Some((_, right)) =
                    barline_extent(page, staff, staff_index, column.x0, jianpu)
                {
                    x0 = x0.max(right);
                }
                if let Some((left, _)) = barline_extent(page, staff, staff_index, column.x1, jianpu)
                {
                    x1 = x1.min(left);
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
                            matches!(
                                n.kind.as_str(),
                                "NoteHead" | "TabNoteHead" | "Rest" | "MultiMeasureRest"
                            ) && n.bbox.x0 >= x0
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
                            matches!(
                                o.kind.as_str(),
                                "NoteHead" | "TabNoteHead" | "Rest" | "MultiMeasureRest"
                            ) && o.bbox.x0 >= prefix
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
            capability: if engraved.is_empty() {
                page.capability
            } else {
                LocationCapability::Measure
            },
            reason: engraved.is_empty().then(|| page.reason.clone()).flatten(),
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
    graphics::associate(index, layout, &instrument_spans, &mut pages);
    Ok(SelectionGeometry {
        pages,
        source_hash: index.source_hash.clone(),
        engraved,
    })
}

/// Pick local symbols before spanning marks and measure whitespace.
pub fn hit_selection(
    index: &ScoreIndex,
    geometry: &SelectionGeometry,
    page: u32,
    x: f64,
    y: f64,
) -> Result<Option<SelectionRegion>, AppError> {
    if !x.is_finite() || !y.is_finite() {
        return Err(AppError::invalid("point must be finite"));
    }
    let Some(page) = geometry.iter().find(|p| p.page == page) else {
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
        .map(|r| geometry.resolve_measure(index, &r.locator))
        .transpose()
}

mod graphics;
pub use graphics::expand_selection;
