use super::events::{Event, Harmony, Pitch, Tick};
use super::score::{classify_chord, Chord};
use super::write::Xml;
use super::{invalid_events, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug)]
pub(super) struct TabPosition {
    pub string: u8,
    pub fret: u16,
}

pub(super) fn positions(events: &[&Event], source: &str) -> Result<HashMap<Pitch, TabPosition>> {
    let mut positions = HashMap::new();
    for event in events.iter().filter(|event| event.kind == "tab-note") {
        let (pitch, string, fret): (Pitch, u8, u16) = event.fields()?;
        if string == 0
            || positions
                .insert(pitch, TabPosition { string, fret })
                .is_some()
        {
            return Err(event.fail(source, "ambiguous tablature string and fret assignments"));
        }
    }
    Ok(positions)
}

pub(super) fn attach_frames(
    events: &[&Event],
    harmonies: &mut BTreeMap<Tick, Chord>,
    source: &str,
) -> Result<()> {
    let mut written = BTreeMap::new();
    for event in events.iter().filter(|event| event.kind == "harmony") {
        if written
            .insert(event.time, event.fields::<Harmony>()?)
            .is_some()
        {
            return Err(event.fail(source, "multiple fretboard chords at the same beat"));
        }
    }
    let pitch_class = |pitch: Option<Pitch>| pitch.map(|Pitch(step, _, alter)| (step, alter));
    for event in events.iter().filter(|event| event.kind == "fretboard") {
        let fail = || {
            event.fail(
                source,
                "a fretboard diagram without one matching chord symbol",
            )
        };
        let (pitches, bass) = written.get(&event.time).ok_or_else(fail)?;
        let diagram = classify_chord(pitches, bass).ok_or_else(fail)?;
        let chord = harmonies
            .get_mut(&event.time)
            .filter(|chord| {
                chord.kind == diagram.kind
                    && pitch_class(chord.root) == pitch_class(diagram.root)
                    && pitch_class(chord.bass) == pitch_class(diagram.bass)
                    && chord.frame.is_none()
            })
            .ok_or_else(fail)?;
        chord.frame = Some(Frame::read(event, source)?);
    }
    Ok(())
}

pub(super) struct Frame {
    strings: u8,
    frets: u16,
    first: u16,
    notes: BTreeMap<u8, FrameNote>,
    barres: Vec<(u8, u8, u16)>,
}

#[derive(Default)]
struct FrameNote {
    fret: u16,
    finger: Option<String>,
}

impl Frame {
    fn read(event: &Event, source: &str) -> Result<Self> {
        let (strings, frets, markings): (u8, u16, Vec<Vec<Value>>) = event.fields()?;
        let fail = || event.fail(source, "this fretboard diagram");
        if strings == 0 || frets == 0 {
            return Err(fail());
        }
        let mut notes = BTreeMap::new();
        let mut barres = Vec::new();
        let mut strings_marked = BTreeSet::new();
        let mut barre_ends = BTreeSet::new();
        for item in &markings {
            let number = |at| item.get(at).and_then(Value::as_u64);
            let string = |at| {
                number(at)
                    .and_then(|n| u8::try_from(n).ok())
                    .filter(|n| (1..=strings).contains(n))
                    .ok_or_else(fail)
            };
            let fret = |at| {
                number(at)
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|&n| n > 0)
                    .ok_or_else(fail)
            };
            let position = match item.first().and_then(Value::as_str) {
                Some("mute") => {
                    if !strings_marked.insert(string(1)?) {
                        return Err(fail());
                    }
                    continue;
                }
                Some("barre") => {
                    let from = string(1)?;
                    let to = string(2)?;
                    if !barre_ends.insert(from) || !barre_ends.insert(to) {
                        return Err(fail());
                    }
                    barres.push((from.max(to), from.min(to), fret(3)?));
                    continue;
                }
                Some("open") => FrameNote::default(),
                Some("place-fret") => {
                    let finger = match item.get(3) {
                        None => None,
                        Some(Value::String(value)) => Some(value.clone()),
                        Some(Value::Number(value)) => Some(value.to_string()),
                        _ => return Err(fail()),
                    };
                    FrameNote {
                        fret: fret(2)?,
                        finger,
                    }
                }
                _ => return Err(fail()),
            };
            let string = string(1)?;
            if !strings_marked.insert(string) {
                return Err(fail());
            }
            notes.insert(string, position);
        }
        for &(start, stop, fret) in &barres {
            for string in [start, stop] {
                if strings_marked.contains(&string) && !notes.contains_key(&string) {
                    return Err(fail());
                }
                let note = notes
                    .entry(string)
                    .or_insert_with(|| FrameNote { fret, finger: None });
                if note.fret != fret {
                    return Err(fail());
                }
            }
        }
        if notes.is_empty() {
            return Err(fail());
        }
        let min = notes
            .values()
            .map(|note| note.fret)
            .filter(|&f| f > 0)
            .min();
        let max = notes.values().map(|note| note.fret).max().unwrap_or(0);
        let first = if max > frets { min.unwrap_or(1) } else { 1 };
        let frets = frets.max(max.saturating_sub(first) + 1);
        Ok(Self {
            strings,
            frets,
            first,
            notes,
            barres,
        })
    }

    pub fn write(&self, xml: &mut Xml) -> Result<()> {
        xml.start("frame", &[("unplayed", "x")])?;
        xml.value("frame-strings", self.strings)?;
        xml.value("frame-frets", self.frets)?;
        if self.first > 1 {
            xml.value("first-fret", self.first)?;
        }
        for (&string, note) in self.notes.iter().rev() {
            xml.start("frame-note", &[])?;
            xml.value("string", string)?;
            xml.value("fret", note.fret)?;
            if let Some(finger) = &note.finger {
                xml.value("fingering", finger)?;
            }
            if let Some(kind) = self.barres.iter().find_map(|&(start, stop, _)| {
                if string == start {
                    Some("start")
                } else if string == stop {
                    Some("stop")
                } else {
                    None
                }
            }) {
                xml.empty("barre", &[("type", kind)])?;
            }
            xml.end("frame-note")?;
        }
        xml.end("frame")
    }
}

pub(super) fn write_tuning(xml: &mut Xml, tuning: &[Pitch], staff: Option<usize>) -> Result<()> {
    xml.numbered("staff-details", staff)?;
    xml.value("staff-lines", tuning.len())?;
    for (index, &Pitch(step, octave, alter)) in tuning.iter().rev().enumerate() {
        xml.start("staff-tuning", &[("line", &(index + 1).to_string())])?;
        xml.value("tuning-step", Pitch(step, octave, alter).step()?)?;
        if alter.0 != 0 {
            xml.value("tuning-alter", alter.integer().ok_or_else(invalid_events)?)?;
        }
        xml.value("tuning-octave", octave)?;
        xml.end("staff-tuning")?;
    }
    xml.end("staff-details")
}
