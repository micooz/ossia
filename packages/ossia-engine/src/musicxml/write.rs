use super::events::Tick;
use super::measures::{Barline, Measure};
use super::note_xml::{duration_type, write_note_group};
use super::score::{key_signature, Chord, Direction, Metronome, Part, Score, Tempo};
use super::tablature::write_tuning;
use super::{invalid_events, Result};
use crate::{failure, io_error, RunContext};
use ossia_types::AppError;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::collections::BTreeSet;

pub(super) fn encode(score: &Score, run: &RunContext) -> Result<Vec<u8>> {
    let mut xml = Xml(Writer::new(Vec::new()));
    xml.0
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(io_error)?;
    xml.start("score-partwise", &[("version", "4.0")])?;
    if !score.metadata.title.is_empty() {
        xml.start("work", &[])?;
        xml.value("work-title", &score.metadata.title)?;
        xml.end("work")?;
    }
    xml.start("identification", &[])?;
    for (role, name) in [
        ("composer", &score.metadata.composer),
        ("lyricist", &score.metadata.poet),
    ] {
        if !name.is_empty() {
            xml.text("creator", &[("type", role)], name)?;
        }
    }
    xml.start("encoding", &[])?;
    xml.value("software", "Ossia")?;
    xml.end("encoding")?;
    xml.end("identification")?;

    xml.start("part-list", &[])?;
    for (i, part) in score.parts.iter().enumerate() {
        xml.start("score-part", &[("id", &format!("P{}", i + 1))])?;
        xml.value("part-name", &part.name)?;
        xml.end("score-part")?;
    }
    xml.end("part-list")?;

    for (i, part) in score.parts.iter().enumerate() {
        xml.start("part", &[("id", &format!("P{}", i + 1))])?;
        let mut voice_number = 0;
        let mut slur_base = 0;
        let mut tracks = Vec::new();
        let mut voice_bases = Vec::new();
        for (staff_index, staff) in part.staves.iter().enumerate() {
            voice_bases.push(voice_number);
            for voice in &staff.voices {
                voice_number += 1;
                tracks.push((staff_index + 1, voice_number, slur_base, voice));
                slur_base += usize::from(
                    voice
                        .notes
                        .values()
                        .chain(voice.graces.values().flatten().map(|(_, note)| note))
                        .flat_map(|note| &note.slurs)
                        .map(|(number, _)| *number)
                        .max()
                        .unwrap_or(0),
                );
            }
        }
        if slur_base > 16 {
            return Err(AppError::unsupported(
                "MusicXML export does not support this many distinct slurs in one part",
            ));
        }
        let grouped_rests = multi_measure_rests(part, &score.measures);
        for (index, measure) in score.measures.iter().enumerate() {
            run.check()?;
            if xml.0.get_ref().len() as u64 > super::MAX_NOTATION_BYTES {
                return Err(failure("output-limit", "MusicXML output exceeds 32 MiB"));
            }
            let number = if measure.fragment == 0 {
                measure.number.to_string()
            } else {
                format!("{}.{}", measure.number, measure.fragment)
            };
            if measure.implicit {
                xml.start("measure", &[("number", &number), ("implicit", "yes")])?;
            } else {
                xml.start("measure", &[("number", &number)])?;
            }
            write_barline(&mut xml, &measure.left, true)?;
            write_attributes(
                &mut xml,
                part,
                measure.from,
                (index == 0).then_some(score.divisions),
                measure.time,
                grouped_rests[index],
            )?;

            let offsets: BTreeSet<_> = part
                .staves
                .iter()
                .flat_map(|staff| {
                    let range = (measure.from + 1)..measure.to;
                    staff
                        .clefs
                        .range(range.clone())
                        .map(|(&time, _)| time)
                        .chain(staff.keys.range(range.clone()).map(|(&time, _)| time))
                        .chain(staff.transpositions.range(range).map(|(&time, _)| time))
                        .chain(
                            staff
                                .tablature
                                .range((measure.from + 1)..measure.to)
                                .map(|(&time, _)| time),
                        )
                })
                .collect();
            let mut cursor = measure.from;
            for time in offsets {
                seek(&mut xml, "forward", time - cursor, None)?;
                write_attributes(&mut xml, part, time, None, None, 0)?;
                cursor = time;
            }
            seek(&mut xml, "backup", cursor - measure.from, None)?;

            if i == 0 {
                for (&time, tempo) in score.tempo.range(measure.from..measure.to) {
                    write_tempo(&mut xml, tempo, time - measure.from)?;
                }
            }
            for (staff_index, staff) in part.staves.iter().enumerate() {
                for (time, voice, target, direction) in
                    staff.dynamics.iter().filter(|(time, _, _, _)| {
                        (measure.from..measure.to).contains(time)
                            || (*time == measure.to && index + 1 == score.measures.len())
                    })
                {
                    write_direction(
                        &mut xml,
                        direction,
                        time - measure.from,
                        *target,
                        voice_bases[staff_index] + voice,
                    )?;
                }
            }
            for (&time, chord) in part.harmonies.range(measure.from..measure.to) {
                write_harmony(&mut xml, chord, time - measure.from)?;
            }

            if tracks.is_empty() {
                seek(&mut xml, "forward", measure.to - measure.from, None)?;
            }
            for (track_index, &(home_staff, voice_number, slur_base, voice)) in
                tracks.iter().enumerate()
            {
                let mut cursor = measure.from;
                if track_index > 0 {
                    seek(&mut xml, "backup", measure.to - measure.from, None)?;
                }
                for (&time, note) in voice.notes.range(measure.from..measure.to) {
                    seek(
                        &mut xml,
                        "forward",
                        time - cursor,
                        Some((voice_number, voice.staff_at(cursor, 0, home_staff))),
                    )?;
                    if let Some(group) = voice.graces.get(&time) {
                        for (grace, note) in group {
                            let staff = voice.staff_at(time, *grace, home_staff);
                            write_note_group(&mut xml, note, voice_number, staff, slur_base)?;
                        }
                    }
                    let staff = voice.staff_at(time, 0, home_staff);
                    write_note_group(&mut xml, note, voice_number, staff, slur_base)?;
                    cursor = time + note.duration;
                }
                seek(
                    &mut xml,
                    "forward",
                    measure.to - cursor,
                    Some((voice_number, voice.staff_at(cursor, 0, home_staff))),
                )?;
            }
            write_barline(&mut xml, &measure.right, false)?;
            xml.end("measure")?;
        }
        xml.end("part")?;
    }
    xml.end("score-partwise")?;
    Ok(xml.0.into_inner())
}

fn multi_measure_rests(part: &Part, measures: &[Measure]) -> Vec<usize> {
    let mut counts = vec![0; measures.len()];
    let [staff] = part.staves.as_slice() else {
        return counts;
    };
    let [voice] = staff.voices.as_slice() else {
        return counts;
    };
    let mut from = None;
    for (i, measure) in measures.iter().enumerate() {
        let boundary = measure.time.is_some()
            || measure.left.repeat.is_some()
            || measure.left.ending.is_some()
            || staff.keys.range(measure.from..measure.to).next().is_some()
            || staff.clefs.range(measure.from..measure.to).next().is_some()
            || part
                .harmonies
                .range(measure.from..measure.to)
                .next()
                .is_some();
        let rest = voice
            .notes
            .get(&measure.from)
            .is_some_and(|note| note.measure_rest)
            && !measure.implicit
            && measure.right.repeat.is_none()
            && measure.right.ending.is_none();
        if (!rest || boundary) && from.is_some() {
            let start = from.take().unwrap();
            if i - start > 1 {
                counts[start] = i - start;
            }
        }
        if rest && from.is_none() {
            from = Some(i);
        }
    }
    if let Some(start) = from {
        if measures.len() - start > 1 {
            counts[start] = measures.len() - start;
        }
    }
    counts
}

fn write_attributes(
    xml: &mut Xml,
    part: &Part,
    time: Tick,
    divisions: Option<Tick>,
    meter: Option<(u32, u32)>,
    multi_rest: usize,
) -> Result<()> {
    let keys = part
        .staves
        .iter()
        .enumerate()
        .filter_map(|(i, staff)| staff.keys.get(&time).map(|k| (i, k)))
        .collect::<Vec<_>>();
    let clefs = part
        .staves
        .iter()
        .enumerate()
        .filter_map(|(i, staff)| staff.clefs.get(&time).map(|c| (i, c)))
        .collect::<Vec<_>>();
    let transpositions = part
        .staves
        .iter()
        .enumerate()
        .filter_map(|(i, staff)| staff.transpositions.get(&time).map(|value| (i, value)))
        .collect::<Vec<_>>();
    let tunings = part
        .staves
        .iter()
        .enumerate()
        .filter_map(|(i, staff)| staff.tablature.get(&time).map(|tuning| (i, tuning)))
        .collect::<Vec<_>>();
    if divisions.is_none()
        && meter.is_none()
        && keys.is_empty()
        && clefs.is_empty()
        && transpositions.is_empty()
        && tunings.is_empty()
        && multi_rest == 0
    {
        return Ok(());
    }
    xml.start("attributes", &[])?;
    if let Some(value) = divisions {
        xml.value("divisions", value)?;
    }
    let all_keys_equal = keys.len() == part.staves.len()
        && keys
            .windows(2)
            .all(|pair| key_signature(pair[0].1) == key_signature(pair[1].1));
    for (i, key) in keys {
        let (fifths, mode) = key_signature(key).ok_or_else(invalid_events)?;
        xml.numbered(
            "key",
            (part.staves.len() > 1 && !all_keys_equal).then_some(i + 1),
        )?;
        xml.value("fifths", fifths)?;
        xml.value("mode", mode)?;
        xml.end("key")?;
        if all_keys_equal {
            break;
        }
    }
    if let Some((beats, beat_type)) = meter {
        xml.start("time", &[])?;
        xml.value("beats", beats)?;
        xml.value("beat-type", beat_type)?;
        xml.end("time")?;
    }
    if divisions.is_some() && part.staves.len() > 1 {
        xml.value("staves", part.staves.len())?;
    }
    for (i, (glyph, position, transpose)) in clefs {
        let sign = match glyph.as_str() {
            "clefs.G" | "clefs.G_change" => "G",
            "clefs.F" | "clefs.F_change" => "F",
            "clefs.C" | "clefs.C_change" => "C",
            "clefs.tab" | "clefs.tab_change" | "clefs.moderntab" | "clefs.moderntab_change" => {
                "TAB"
            }
            _ => {
                return Err(AppError::unsupported(format!(
                    "MusicXML export does not support clef {glyph}"
                )))
            }
        };
        let line = if sign == "TAB" {
            5
        } else {
            3 + i32::from(*position) / 2
        };
        if !(1..=5).contains(&line) || *transpose % 7 != 0 {
            return Err(AppError::unsupported(
                "MusicXML export does not support this clef position",
            ));
        }
        xml.numbered("clef", (part.staves.len() > 1).then_some(i + 1))?;
        xml.value("sign", sign)?;
        if sign != "TAB"
            || part.staves[i]
                .tablature
                .range(..=time)
                .next_back()
                .is_some_and(|(_, tuning)| tuning.len() >= 5)
        {
            xml.value("line", line)?;
        }
        if *transpose != 0 {
            xml.value("clef-octave-change", transpose / 7)?;
        }
        xml.end("clef")?;
    }
    for (i, tuning) in tunings {
        write_tuning(xml, tuning, (part.staves.len() > 1).then_some(i + 1))?;
    }
    for (i, (diatonic, chromatic)) in transpositions {
        xml.numbered("transpose", (part.staves.len() > 1).then_some(i + 1))?;
        xml.value("diatonic", diatonic)?;
        xml.value("chromatic", chromatic)?;
        xml.end("transpose")?;
    }
    if multi_rest > 1 {
        xml.start("measure-style", &[])?;
        xml.value("multiple-rest", multi_rest)?;
        xml.end("measure-style")?;
    }
    xml.end("attributes")
}

fn write_harmony(xml: &mut Xml, chord: &Chord, offset: Tick) -> Result<()> {
    xml.start("harmony", &[("placement", "above")])?;
    xml.start("root", &[])?;
    if let Some(root) = chord.root {
        xml.value("root-step", root.step()?)?;
        if root.2 .0 != 0 {
            xml.value("root-alter", root.2.integer().ok_or_else(invalid_events)?)?;
        }
    } else {
        xml.text("root-step", &[("text", "")], "C")?;
    }
    xml.end("root")?;
    xml.value("kind", chord.kind)?;
    if let Some(bass) = chord.bass {
        xml.start("bass", &[])?;
        xml.value("bass-step", bass.step()?)?;
        if bass.2 .0 != 0 {
            xml.value("bass-alter", bass.2.integer().ok_or_else(invalid_events)?)?;
        }
        xml.end("bass")?;
    }
    if let Some(frame) = &chord.frame {
        frame.write(xml)?;
    }
    emit_offset(xml, offset)?;
    xml.value("staff", 1)?;
    xml.end("harmony")
}

fn write_tempo(xml: &mut Xml, tempo: &Tempo, offset: Tick) -> Result<()> {
    xml.start("direction", &[("placement", "above")])?;
    if !tempo.text.is_empty() {
        xml.start("direction-type", &[])?;
        xml.value("words", &tempo.text)?;
        xml.end("direction-type")?;
    }
    if let Some(Metronome(log, dots, count, _)) = &tempo.metronome {
        xml.start("direction-type", &[])?;
        xml.start("metronome", &[])?;
        let kind = duration_type(*log).ok_or_else(|| {
            AppError::unsupported("MusicXML export does not support this metronome beat unit")
        })?;
        xml.value("beat-unit", kind)?;
        for _ in 0..*dots {
            xml.empty("beat-unit-dot", &[])?;
        }
        xml.value("per-minute", count.0 as f64 / count.1 as f64)?;
        xml.end("metronome")?;
        xml.end("direction-type")?;
    }
    emit_offset(xml, offset)?;
    if let Some(Metronome(_, _, _, tempo)) = &tempo.metronome {
        xml.empty(
            "sound",
            &[("tempo", &(tempo.0 as f64 / tempo.1 as f64).to_string())],
        )?;
    }
    xml.end("direction")
}

fn write_direction(
    xml: &mut Xml,
    direction: &Direction,
    offset: Tick,
    staff: usize,
    voice: usize,
) -> Result<()> {
    xml.start("direction", &[("placement", "below")])?;
    xml.start("direction-type", &[])?;
    match direction {
        Direction::Dynamic(value) => {
            xml.start("dynamics", &[])?;
            match value.as_str() {
                "p" | "pp" | "ppp" | "pppp" | "ppppp" | "pppppp" | "f" | "ff" | "fff" | "ffff"
                | "fffff" | "ffffff" | "mp" | "mf" | "sf" | "sfp" | "sfpp" | "fp" | "rf"
                | "rfz" | "sfz" | "sffz" | "fz" | "n" | "pf" | "sfzp" => xml.empty(value, &[])?,
                _ => xml.value("other-dynamics", value)?,
            }
            xml.end("dynamics")?;
        }
        Direction::Wedge(kind) => {
            if voice > 16 {
                return Err(AppError::unsupported(
                    "MusicXML export does not support this many hairpin voices",
                ));
            }
            xml.empty("wedge", &[("type", kind), ("number", &voice.to_string())])?;
        }
    }
    xml.end("direction-type")?;
    emit_offset(xml, offset)?;
    xml.value("voice", voice)?;
    xml.value("staff", staff)?;
    xml.end("direction")
}

fn write_barline(xml: &mut Xml, bar: &Barline, left: bool) -> Result<()> {
    if bar.repeat.is_none() && bar.ending.is_none() {
        return Ok(());
    }
    xml.start(
        "barline",
        &[("location", if left { "left" } else { "right" })],
    )?;
    if let Some(number) = bar.ending {
        let kind = if left {
            "start"
        } else if number == 1 {
            "stop"
        } else {
            "discontinue"
        };
        xml.empty("ending", &[("number", &number.to_string()), ("type", kind)])?;
    }
    if let Some(times) = bar.repeat {
        if left {
            xml.empty("repeat", &[("direction", "forward")])?;
        } else {
            xml.empty(
                "repeat",
                &[("direction", "backward"), ("times", &times.to_string())],
            )?;
        }
    }
    xml.end("barline")
}

fn seek(xml: &mut Xml, element: &str, duration: Tick, track: Option<(usize, usize)>) -> Result<()> {
    if duration < 0 {
        return Err(invalid_events());
    }
    if duration > 0 {
        xml.start(element, &[])?;
        xml.value("duration", duration)?;
        if let Some((voice, staff)) = track {
            xml.value("voice", voice)?;
            xml.value("staff", staff)?;
        }
        xml.end(element)?;
    }
    Ok(())
}

fn emit_offset(xml: &mut Xml, offset: Tick) -> Result<()> {
    if offset != 0 {
        xml.value("offset", offset)?;
    }
    Ok(())
}

pub(super) struct Xml(Writer<Vec<u8>>);

impl Xml {
    pub(super) fn start(&mut self, name: &str, attributes: &[(&str, &str)]) -> Result<()> {
        let mut tag = BytesStart::new(name);
        tag.extend_attributes(attributes.iter().copied());
        self.0.write_event(Event::Start(tag)).map_err(io_error)
    }

    pub(super) fn numbered(&mut self, name: &str, number: Option<usize>) -> Result<()> {
        if let Some(number) = number {
            self.start(name, &[("number", &number.to_string())])
        } else {
            self.start(name, &[])
        }
    }

    pub(super) fn end(&mut self, name: &str) -> Result<()> {
        self.0
            .write_event(Event::End(BytesEnd::new(name)))
            .map_err(io_error)
    }

    pub(super) fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) -> Result<()> {
        let mut tag = BytesStart::new(name);
        tag.extend_attributes(attributes.iter().copied());
        self.0.write_event(Event::Empty(tag)).map_err(io_error)
    }

    pub(super) fn value(&mut self, name: &str, value: impl ToString) -> Result<()> {
        self.text(name, &[], &value.to_string())
    }

    pub(super) fn text(
        &mut self,
        name: &str,
        attributes: &[(&str, &str)],
        value: &str,
    ) -> Result<()> {
        if value.chars().any(|c| !matches!(c, '\t' | '\r' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')) {
            return Err(failure("musicxml-text", "notation contains a character forbidden by XML 1.0"));
        }
        self.start(name, attributes)?;
        self.0
            .write_event(Event::Text(BytesText::new(value)))
            .map_err(io_error)?;
        self.end(name)
    }
}
