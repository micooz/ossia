use crate::{build_index, resolve_pitches, tokenize, MidiDivision, MidiFile, TokenKind};
use ossia_types::*;
use std::collections::BTreeSet;

/// Analyze only referenced voices; lexical note counts describe the complete input text.
pub fn analyze_source(source: &str) -> SourceAnalysis {
    let index = build_index(source);
    let mut reasons = Vec::new();
    let measures = match index.measure_count() {
        Ok(n) => Some(n),
        Err(e) => {
            reasons.push(e);
            None
        }
    };
    let names: BTreeSet<_> = index.staves.iter().flat_map(|s| s.voices.iter()).collect();
    let voices = (!names.is_empty()).then_some(names.len());
    let mut events = Vec::new();
    for name in names {
        if let Some(voice) = index.voice(name) {
            match resolve_pitches(source, voice.music_span) {
                Ok(result) => {
                    if let Some(reason) = result.unsupported {
                        reasons.push(format!("{name}: {reason}"));
                    }
                    for event in result.events.into_iter().filter(|e| !e.pitches.is_empty()) {
                        events.push(WrittenEvent {
                            voice: event
                                .voice
                                .map_or_else(|| name.clone(), |branch| format!("{name}[{branch}]")),
                            span: event.span,
                            pitches: event.pitches,
                            pitch_tokens: event
                                .members
                                .iter()
                                .map(|member| member.raw.clone())
                                .collect(),
                            measure: index
                                .measure_at(event.span.byte_start)
                                .map(|measure| measure.measure_start),
                        });
                    }
                }
                Err(error) => reasons.push(format!("{name}: {error}")),
            }
        }
    }
    let count = tokenize(source)
        .iter()
        .filter(|t| t.kind == TokenKind::Note)
        .count();
    SourceAnalysis {
        status: if reasons.is_empty() {
            AnalysisStatus::Complete
        } else if voices.is_some() {
            AnalysisStatus::Partial
        } else {
            AnalysisStatus::Unavailable
        },
        note_tokens: if count > 0 || reasons.is_empty() {
            Some(count)
        } else {
            None
        },
        reasons,
        measures,
        voices,
        events,
    }
}

/// Normalize MIDI note-ons and releases to exact quarter-note time.
pub fn analyze_performance(midi: &MidiFile) -> PerformanceAnalysis {
    let mut reasons = Vec::new();
    let ppqn = match midi.division {
        MidiDivision::Ppqn { ticks_per_quarter } if midi.format != 2 => {
            Some(u64::from(ticks_per_quarter))
        }
        _ => {
            reasons.push("quarter-note time requires PPQN and a shared format-0/1 timeline".into());
            None
        }
    };
    let mut notes = Vec::new();
    let (mut measure_status, mut measure_reason) = performance_measure_status(midi, ppqn);
    for track in &midi.tracks {
        let mut ordered: Vec<_> = track.notes.iter().collect();
        ordered.sort_by_key(|n| (n.time, n.channel, n.note));
        for note in ordered {
            if let Some(reason) = &note.duration_reason {
                reasons.push(format!(
                    "track {} channel {} pitch {}: {reason}",
                    track.track, note.channel, note.note
                ));
            }
            let measure = ppqn.and_then(|d| measure_for_tick(note.time, d, midi));
            if measure.is_none() && measure_status == AnalysisStatus::Complete {
                measure_status = AnalysisStatus::Partial;
                measure_reason =
                    Some("some performance notes could not be assigned to a measure".into());
            }
            notes.push(PerformedNote {
                track: track.track,
                channel: note.channel,
                pitch: note.note,
                start: ppqn.map(|d| QuarterTime::new(note.time, d).expect("positive PPQN")),
                duration: ppqn.and_then(|d| {
                    note.duration
                        .map(|n| QuarterTime::new(n, d).expect("positive PPQN"))
                }),
                pitch_name: crate::midi_pitch_name(note.note),
                measure,
            });
        }
    }
    reasons.sort();
    reasons.dedup();
    PerformanceAnalysis {
        status: if reasons.is_empty() {
            AnalysisStatus::Complete
        } else {
            AnalysisStatus::Partial
        },
        reasons,
        notes,
        measure_status,
        measure_reason,
    }
}

/// Preserve explicit failure evidence when MIDI could not be produced or parsed.
pub fn unavailable_performance(reason: impl Into<String>) -> PerformanceAnalysis {
    PerformanceAnalysis {
        status: AnalysisStatus::Unavailable,
        reasons: vec![reason.into()],
        notes: Vec::new(),
        measure_status: AnalysisStatus::Unavailable,
        measure_reason: Some("performance evidence is unavailable".into()),
    }
}

/// True when the source declares an anacrusis or partial measure.
pub fn source_has_partial_measure(source: &str) -> bool {
    tokenize(source)
        .iter()
        .any(|token| token.mode != crate::Mode::Text && token.command() == Some("partial"))
}

/// Remove MIDI-derived measure numbers when the source has an unsupported pickup.
pub fn invalidate_performance_measures(
    performance: &mut PerformanceAnalysis,
    reason: impl Into<String>,
) {
    performance.measure_status = AnalysisStatus::Unavailable;
    performance.measure_reason = Some(reason.into());
    for note in &mut performance.notes {
        note.measure = None;
    }
}

fn performance_measure_status(
    midi: &MidiFile,
    ppqn: Option<u64>,
) -> (AnalysisStatus, Option<String>) {
    let Some(ppqn) = ppqn else {
        return (
            AnalysisStatus::Unavailable,
            Some("measure numbers require a PPQN MIDI timeline".into()),
        );
    };
    if midi.time_signatures.is_empty() {
        return (
            AnalysisStatus::Unavailable,
            Some("MIDI has no time-signature meta event".into()),
        );
    }
    if midi
        .time_signatures
        .first()
        .is_none_or(|signature| signature.tick != 0)
    {
        return (
            AnalysisStatus::Unavailable,
            Some("MIDI time-signature map does not start at tick 0".into()),
        );
    }
    let mut start = 0u64;
    for (index, signature) in midi.time_signatures.iter().enumerate() {
        if signature.tick != start {
            return (
                AnalysisStatus::Unavailable,
                Some("MIDI time-signature change is not at a measure boundary".into()),
            );
        }
        let Some(measure_ticks) = measure_ticks(ppqn, signature) else {
            return (
                AnalysisStatus::Unavailable,
                Some("MIDI time signature cannot be represented as whole ticks".into()),
            );
        };
        if let Some(next) = midi.time_signatures.get(index + 1) {
            let Some(span) = next.tick.checked_sub(start) else {
                return (
                    AnalysisStatus::Unavailable,
                    Some("MIDI time-signature ticks are not monotonic".into()),
                );
            };
            if span % measure_ticks != 0 {
                return (
                    AnalysisStatus::Unavailable,
                    Some("MIDI time-signature change is not at a measure boundary".into()),
                );
            }
            start = next.tick;
        }
    }
    (AnalysisStatus::Complete, None)
}

fn measure_for_tick(tick: u64, ppqn: u64, midi: &MidiFile) -> Option<u32> {
    let signatures = &midi.time_signatures;
    if signatures
        .first()
        .is_none_or(|signature| signature.tick != 0)
    {
        return None;
    }
    let mut measure_start = 0u64;
    let mut completed_measures = 0u64;
    for (index, signature) in signatures.iter().enumerate() {
        let measure_ticks = measure_ticks(ppqn, signature)?;
        if signature.tick != measure_start {
            return None;
        }
        let next = signatures.get(index + 1).map_or(u64::MAX, |next| next.tick);
        if tick < next {
            let offset = tick.checked_sub(measure_start)? / measure_ticks;
            return u32::try_from(completed_measures.checked_add(offset)?.checked_add(1)?).ok();
        }
        let span = next.checked_sub(measure_start)?;
        if span % measure_ticks != 0 {
            return None;
        }
        completed_measures = completed_measures.checked_add(span / measure_ticks)?;
        measure_start = next;
    }
    None
}

fn measure_ticks(ppqn: u64, signature: &crate::MidiTimeSignature) -> Option<u64> {
    let ticks = u128::from(ppqn)
        .checked_mul(4)?
        .checked_mul(u128::from(signature.numerator))?;
    let denominator = u128::from(signature.denominator);
    if denominator == 0 || ticks % denominator != 0 {
        return None;
    }
    let ticks = ticks / denominator;
    u64::try_from(ticks).ok().filter(|ticks| *ticks > 0)
}
