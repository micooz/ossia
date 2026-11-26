use crate::{ScoreIndex, ScoreLayout};
use ossia_types::SourceSpan;
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub(crate) type EngravedMeasures = BTreeMap<(usize, u32), Vec<SourceSpan>>;

#[derive(Clone, Copy)]
struct Moment(i64, u64);

impl Moment {
    fn cmp(self, other: Self) -> Ordering {
        (i128::from(self.0) * i128::from(other.1)).cmp(&(i128::from(other.0) * i128::from(self.1)))
    }
}

/// Index a grid when every cell has engraved source evidence and successive
/// columns agree with the native score timeline.
pub(crate) fn measures(index: &ScoreIndex, layout: &ScoreLayout) -> Option<EngravedMeasures> {
    if layout.columns.is_empty() || index.staves.is_empty() || index.measure_count().is_ok() {
        return None;
    }
    let unnumbered = layout
        .columns
        .iter()
        .all(|column| column.measure_start.is_none());
    let numbered_jianpu = crate::is_jianpu_source(index.source())
        && layout.columns.iter().enumerate().all(|(ordinal, column)| {
            let number = u32::try_from(ordinal + 1).ok();
            number.is_some() && column.measure_start == number && column.measure_end == number
        });
    if !unnumbered && !numbered_jianpu {
        return None;
    }
    let mut measures = BTreeMap::<_, Vec<SourceSpan>>::new();
    let mut moments = BTreeMap::<u32, Vec<Moment>>::new();
    let voice_spans = index
        .staves
        .iter()
        .flat_map(|staff| &staff.voices)
        .filter_map(|voice| index.voice(voice))
        .map(|voice| voice.music_span)
        .collect::<Vec<_>>();
    for (page_index, page) in layout.pages.iter().enumerate() {
        if page.page as usize != page_index + 1 || page.systems.is_empty() {
            return None;
        }
        let columns = layout
            .columns
            .iter()
            .enumerate()
            .filter(|(_, column)| column.page == page.page)
            .collect::<Vec<_>>();
        for system in &page.systems {
            if system.staves.len() != index.staves.len()
                || system.bar_xs.len() < 2
                || !system.bar_xs.iter().skip(1).all(|boundary| {
                    page.objects.iter().any(|object| {
                        object.kind == "BarLine"
                            && (object.bbox.x0 - boundary).abs() < 0.01
                            && object.bbox.y1 >= system.top
                            && object.bbox.y0 <= system.bot
                    })
                })
            {
                return None;
            }
        }
        for object in page.objects.iter().filter(|object| {
            !object.hit_only
                && matches!(
                    object.kind.as_str(),
                    "NoteHead" | "TabNoteHead" | "Rest" | "MultiMeasureRest"
                )
        }) {
            let moment = object
                .timing
                .map(|time| Moment(time.numerator, time.denominator))
                .or_else(|| {
                    object
                        .note
                        .map(|note| Moment(note.numerator, note.denominator))
                })?;
            let staff = object.staff_index?;
            let span = object.span?;
            let (x, y) = object.bbox.center();
            let mut possible = page
                .systems
                .iter()
                .enumerate()
                .filter_map(|(i, system)| {
                    let staff = system.staves.get(staff)?;
                    let distance = (y - (staff.top + staff.bot) / 2.).abs();
                    (x >= staff.x0 && x <= staff.x1).then_some((i, distance))
                })
                .collect::<Vec<_>>();
            possible.sort_by(|a, b| a.1.total_cmp(&b.1));
            let &(system, distance) = possible.first()?;
            if possible
                .get(1)
                .is_some_and(|other| other.1 - distance < 1e-6)
            {
                return None;
            }
            let (ordinal, _) = columns.iter().find(|(_, column)| {
                column.system_index == system && x >= column.x0 && x < column.x1
            })?;
            // The engraved staff follows staff changes while the source stays in its voice.
            if !voice_spans
                .iter()
                .any(|voice| voice.byte_start <= span.byte_start && span.byte_end <= voice.byte_end)
            {
                return None;
            }
            let measure = u32::try_from(ordinal + 1).ok()?;
            moments.entry(measure).or_default().push(moment);
            measures.entry((staff, measure)).or_default().push(span);
        }
    }
    if measures.len() != layout.columns.len().checked_mul(index.staves.len())? {
        return None;
    }
    let mut previous: Option<Moment> = None;
    for values in moments.values() {
        let first = *values.iter().min_by(|a, b| a.cmp(**b))?;
        if previous.is_some_and(|last| last.cmp(first) != Ordering::Less) {
            return None;
        }
        previous = values.iter().max_by(|a, b| a.cmp(**b)).copied();
    }
    for spans in measures.values_mut() {
        spans.sort_unstable_by_key(|span| (span.byte_start, span.byte_end));
        spans.dedup();
    }
    Some(measures)
}
