use crate::{BBox, ScoreLayout};
use ossia_types::{AppError, ErrorCode, MeasureRange};

/// A reliable page crop request in rendered SVG coordinates.
#[derive(Debug, Clone)]
pub struct CropQuery {
    pub page: Option<u32>,
    pub system_index: Option<usize>,
    pub measure_range: Option<MeasureRange>,
    pub staff: Option<usize>,
    pub padding_staff_spaces: f64,
}

/// One ink-inclusive crop region and the staff space used to derive it.
#[derive(Debug, Clone)]
pub struct CropRegion {
    pub page: u32,
    pub bounds: BBox,
    pub staff_space: f64,
}

/// Resolve measure or system requests to ink-inclusive page bounds.
pub fn crop_regions(layout: &ScoreLayout, query: &CropQuery) -> Result<Vec<CropRegion>, AppError> {
    if !query.padding_staff_spaces.is_finite()
        || !(0.0..=16.0).contains(&query.padding_staff_spaces)
    {
        return Err(AppError::invalid(
            "paddingStaffSpaces must be finite and between 0 and 16",
        ));
    }
    if query.measure_range.is_some() == query.system_index.is_some() {
        return Err(AppError::invalid(
            "crop requires exactly one of measureRange or systemIndex",
        ));
    }
    let mut regions = Vec::new();
    for page in &layout.pages {
        if query.page.is_some_and(|requested| requested != page.page) {
            continue;
        }
        let matching = page
            .systems
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                query
                    .system_index
                    .is_none_or(|requested| *index == requested)
            })
            .filter(|(index, _)| {
                query.measure_range.as_ref().is_none_or(|range| {
                    layout.columns.iter().any(|column| {
                        column.page == page.page
                            && column.system_index == *index
                            && column.measure_start.zip(column.measure_end).is_some_and(
                                |(start, end)| start <= range.end && end >= range.start,
                            )
                    })
                })
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        let staff_space = page.staff_space;
        if !staff_space.is_finite() || staff_space <= 0.0 {
            return Err(AppError::unsupported(
                "crop requires reliable staff-space geometry",
            ));
        }
        let mut bounds = None::<BBox>;
        for (system_index, system) in matching {
            let selected_staff = query.staff.map(|index| {
                system.staves.get(index).ok_or_else(|| {
                    AppError::invalid(format!(
                        "staff index {index} is outside page {} system {system_index}",
                        page.page
                    ))
                })
            });
            let (x0, x1, y0, y1) = if let Some(staff) = selected_staff {
                let staff = staff?;
                (staff.x0, staff.x1, staff.top, staff.bot)
            } else {
                let first = system
                    .staves
                    .first()
                    .ok_or_else(|| AppError::unsupported("crop system has no staves"))?;
                let last = system
                    .staves
                    .last()
                    .ok_or_else(|| AppError::unsupported("crop system has no staves"))?;
                let x0 = system
                    .staves
                    .iter()
                    .map(|staff| staff.x0)
                    .fold(f64::INFINITY, f64::min);
                let x1 = system
                    .staves
                    .iter()
                    .map(|staff| staff.x1)
                    .fold(f64::NEG_INFINITY, f64::max);
                (x0, x1, first.top, last.bot)
            };
            let (x0, x1) = query.measure_range.as_ref().map_or((x0, x1), |range| {
                let mut columns = layout.columns.iter().filter(|column| {
                    column.page == page.page
                        && column.system_index == system_index
                        && column
                            .measure_start
                            .zip(column.measure_end)
                            .is_some_and(|(start, end)| start <= range.end && end >= range.start)
                });
                let first = columns.next();
                first.map_or((x0, x1), |first| {
                    columns.fold((first.x0, first.x1), |(left, right), column| {
                        (left.min(column.x0), right.max(column.x1))
                    })
                })
            });
            let candidate = BBox {
                x0,
                y0: y0 - staff_space * 6.0,
                x1,
                y1: y1 + staff_space * 3.0,
            };
            let mut ink = candidate;
            for element in &page.elements {
                let center = (element.bbox.x0 + element.bbox.x1) / 2.0;
                if center >= x0
                    && center <= x1
                    && element.bbox.y1 >= candidate.y0
                    && element.bbox.y0 <= candidate.y1
                {
                    ink.x0 = ink.x0.min(element.bbox.x0);
                    ink.x1 = ink.x1.max(element.bbox.x1);
                    ink.y0 = ink.y0.min(element.bbox.y0);
                    ink.y1 = ink.y1.max(element.bbox.y1);
                }
            }
            bounds = Some(match bounds {
                Some(current) => BBox {
                    x0: current.x0.min(ink.x0),
                    y0: current.y0.min(ink.y0),
                    x1: current.x1.max(ink.x1),
                    y1: current.y1.max(ink.y1),
                },
                None => ink,
            });
        }
        let mut bounds =
            bounds.ok_or_else(|| AppError::unsupported("crop has no render bounds"))?;
        let padding = query.padding_staff_spaces * staff_space;
        bounds.x0 -= padding;
        bounds.x1 += padding;
        bounds.y0 -= padding;
        bounds.y1 += padding;
        if let Some(view_box) = page.view_box {
            bounds.x0 = bounds.x0.max(view_box.x0);
            bounds.y0 = bounds.y0.max(view_box.y0);
            bounds.x1 = bounds.x1.min(view_box.x1);
            bounds.y1 = bounds.y1.min(view_box.y1);
        }
        if bounds.x1 <= bounds.x0 || bounds.y1 <= bounds.y0 {
            return Err(AppError::new(
                ErrorCode::InvalidRequest,
                "crop bounds are empty after clamping to the page",
            ));
        }
        regions.push(CropRegion {
            page: page.page,
            bounds,
            staff_space,
        });
    }
    if regions.is_empty() {
        return Err(AppError::unsupported(
            "crop location is not present in the rendered score",
        ));
    }
    regions.sort_by_key(|region| region.page);
    Ok(regions)
}
