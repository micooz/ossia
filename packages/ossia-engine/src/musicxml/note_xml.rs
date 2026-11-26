use super::events::Pitch;
use super::notes::{Head, Note};
use super::write::Xml;
use super::{invalid_events, Result};
use ossia_types::AppError;
use std::collections::BTreeSet;

pub(super) fn write_note_group(
    xml: &mut Xml,
    note: &Note,
    voice: usize,
    staff: usize,
    slur_base: usize,
) -> Result<()> {
    if note.heads.is_empty() {
        return write_note(xml, note, None, false, voice, staff, slur_base);
    }
    for (index, head) in note.heads.iter().enumerate() {
        write_note(xml, note, Some(head), index != 0, voice, staff, slur_base)?;
    }
    Ok(())
}

fn write_note(
    xml: &mut Xml,
    note: &Note,
    head: Option<&Head>,
    chord: bool,
    voice: usize,
    staff: usize,
    slur_base: usize,
) -> Result<()> {
    xml.start("note", &[])?;
    if let Some(slash) = note.grace {
        xml.empty("grace", &[("slash", if slash { "yes" } else { "no" })])?;
    }
    if chord {
        xml.empty("chord", &[])?;
    }
    if let Some(head) = head {
        let Pitch(_, octave, alter) = head.pitch;
        xml.start("pitch", &[])?;
        xml.value("step", head.pitch.step()?)?;
        if alter.0 != 0 {
            xml.value("alter", alter.integer().ok_or_else(invalid_events)?)?;
        }
        xml.value("octave", octave)?;
        xml.end("pitch")?;
    } else if note.measure_rest {
        xml.empty("rest", &[("measure", "yes")])?;
    } else {
        xml.empty("rest", &[])?;
    }
    if note.grace.is_none() {
        xml.value("duration", note.duration)?;
    }
    let tie_start = head.is_some_and(|head| head.tie_start);
    let tie_stop = head.is_some_and(|head| head.tie_stop);
    ties(xml, "tie", tie_start, tie_stop)?;
    xml.value("voice", voice)?;
    if !note.measure_rest {
        let kind = duration_type(note.written.1).ok_or_else(|| {
            AppError::unsupported("MusicXML export does not support this written note duration")
        })?;
        xml.value("type", kind)?;
        for _ in 0..note.written.2 {
            xml.empty("dot", &[])?;
        }
    }
    if let Some(head) = head.filter(|head| head.force || head.cautionary) {
        let value = match head.pitch.2.integer() {
            Some(-2) => "flat-flat",
            Some(-1) => "flat",
            Some(0) => "natural",
            Some(1) => "sharp",
            Some(2) => "double-sharp",
            _ => {
                return Err(AppError::unsupported(
                    "MusicXML export does not support this explicit accidental",
                ))
            }
        };
        let attrs: &[(&str, &str)] = if head.cautionary {
            &[("cautionary", "yes"), ("parentheses", "yes")]
        } else {
            &[]
        };
        xml.text("accidental", attrs, value)?;
    }
    if let Some((actual, normal)) = note.tuplet {
        xml.start("time-modification", &[])?;
        xml.value("actual-notes", actual)?;
        xml.value("normal-notes", normal)?;
        xml.end("time-modification")?;
    }
    xml.value("staff", staff)?;
    if !chord {
        for (index, beam) in note.beams.iter().enumerate() {
            xml.text(
                "beam",
                &[("number", &(index + 1).to_string())],
                beam.xml_text(),
            )?;
        }
    }

    let mut articulations = BTreeSet::new();
    if let Some(head) = head {
        articulations.extend(head.articulations.iter().copied());
    }
    if !chord {
        articulations.extend(note.articulations.iter().copied());
    }
    let group_notations = !chord
        && (!note.slurs.is_empty()
            || note.tuplet_start
            || note.tuplet_stop
            || note.tremolo.is_some());
    let tab = head.and_then(|head| head.tab.as_ref());
    let arpeggio = note.arpeggio.filter(|_| head.is_some());
    if tie_start
        || tie_stop
        || group_notations
        || !articulations.is_empty()
        || tab.is_some()
        || arpeggio.is_some()
    {
        xml.start("notations", &[])?;
        ties(xml, "tied", tie_start, tie_stop)?;
        if !chord {
            for &(number, kind) in &note.slurs {
                xml.empty(
                    "slur",
                    &[
                        ("type", kind),
                        ("number", &(slur_base + usize::from(number)).to_string()),
                    ],
                )?;
            }
            if note.tuplet_start {
                xml.empty("tuplet", &[("type", "start")])?;
            }
            if note.tuplet_stop {
                xml.empty("tuplet", &[("type", "stop")])?;
            }
            if let Some(lines) = note.tremolo {
                xml.start("ornaments", &[])?;
                xml.text("tremolo", &[("type", "single")], &lines.to_string())?;
                xml.end("ornaments")?;
            }
        }
        if !articulations.is_empty() {
            xml.start("articulations", &[])?;
            for name in articulations {
                xml.empty(name, &[])?;
            }
            xml.end("articulations")?;
        }
        if let Some(tab) = tab {
            xml.start("technical", &[])?;
            xml.value("string", tab.string)?;
            xml.value("fret", tab.fret)?;
            xml.end("technical")?;
        }
        if let Some(direction) = arpeggio {
            let attributes: &[(&str, &str)] = match direction {
                -1 => &[("direction", "down")],
                1 => &[("direction", "up")],
                _ => &[],
            };
            xml.empty("arpeggiate", attributes)?;
        }
        xml.end("notations")?;
    }

    if !chord {
        for lyric in &note.lyrics {
            xml.numbered("lyric", Some(lyric.verse))?;
            if let Some(text) = &lyric.text {
                xml.value("syllabic", lyric.syllabic)?;
                xml.value("text", text)?;
            }
            if let Some(kind) = lyric.extend {
                xml.empty("extend", &[("type", kind)])?;
            }
            xml.end("lyric")?;
        }
    }
    xml.end("note")
}

pub(super) fn duration_type(log: i8) -> Option<&'static str> {
    let index = usize::try_from(i16::from(log) + 3).ok()?;
    [
        "maxima", "long", "breve", "whole", "half", "quarter", "eighth", "16th", "32nd", "64th",
        "128th", "256th", "512th", "1024th",
    ]
    .get(index)
    .copied()
}

fn ties(xml: &mut Xml, element: &str, start: bool, stop: bool) -> Result<()> {
    if stop {
        xml.empty(element, &[("type", "stop")])?;
    }
    if start {
        xml.empty(element, &[("type", "start")])?;
    }
    Ok(())
}
