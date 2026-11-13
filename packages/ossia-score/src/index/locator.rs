use super::ScoreIndex;
use ossia_types::{AppError, ErrorCode, LocatedRegion, LocationCapability, Locator};

/// Resolve only voices belonging to the explicitly requested staff; no cross-staff fallback.
pub fn resolve_locator(index: &ScoreIndex, locator: &Locator) -> Result<LocatedRegion, AppError> {
    if locator.measure_start == 0 || locator.measure_end < locator.measure_start {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            "invalid measure range",
        ));
    }
    let staff = index
        .staves
        .get(locator.staff_index)
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "staff not found"))?;
    let names: Vec<_> = if let Some(name) = &locator.voice {
        if !staff.voices.contains(name) {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "voice not found on staff",
            ));
        }
        vec![name]
    } else {
        staff.voices.iter().collect()
    };
    let mut spans = Vec::new();
    let mut reason = None;
    for name in names {
        if let Some(voice) = index.voice(name) {
            if voice.capability != LocationCapability::Measure {
                reason = voice.reason.clone();
            }
            spans.extend(
                voice
                    .indexed_measures()
                    .filter(|m| {
                        m.measure_end >= locator.measure_start
                            && m.measure_start <= locator.measure_end
                    })
                    .map(|m| m.span),
            );
        }
    }
    let capability = if reason.is_some() {
        spans.clear();
        LocationCapability::AnchorOnly
    } else if spans.is_empty() {
        reason = Some("no-hit".into());
        LocationCapability::None
    } else {
        LocationCapability::Measure
    };
    Ok(LocatedRegion {
        locator: locator.clone(),
        spans,
        capability,
        reason,
    })
}
