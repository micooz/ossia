use super::events::{ticks, Duration, Event, Fraction, Id, Pitch, RawNote, Tick};
use super::tablature::{positions, TabPosition};
use super::{invalid_events, Result};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug)]
pub(super) struct Head {
    pub pitch: Pitch,
    pub tab: Option<TabPosition>,
    pub force: bool,
    pub cautionary: bool,
    pub tie_start: bool,
    pub tie_stop: bool,
    pub articulations: Vec<&'static str>,
}

#[derive(Debug)]
pub(super) struct Lyric {
    pub verse: usize,
    pub syllabic: &'static str,
    pub text: Option<String>,
    pub extend: Option<&'static str>,
}

#[derive(Debug)]
pub(super) struct Note {
    pub duration: Tick,
    pub written: Duration,
    pub heads: Vec<Head>,
    pub measure_rest: bool,
    pub grace: Option<bool>,
    pub slurs: Vec<(u8, &'static str)>,
    pub tuplet: Option<(u32, u32)>,
    pub tuplet_start: bool,
    pub tuplet_stop: bool,
    pub lyrics: Vec<Lyric>,
    pub articulations: Vec<&'static str>,
    pub tremolo: Option<u32>,
    pub beams: Vec<Beam>,
    pub arpeggio: Option<i8>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Beam {
    Begin,
    Continue,
    End,
    #[serde(rename = "forward hook")]
    ForwardHook,
    #[serde(rename = "backward hook")]
    BackwardHook,
}

impl Beam {
    pub fn xml_text(&self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::Continue => "continue",
            Self::End => "end",
            Self::ForwardHook => "forward hook",
            Self::BackwardHook => "backward hook",
        }
    }
}

impl Note {
    fn new(duration: Tick, written: Duration, measure_rest: bool) -> Self {
        Self {
            duration,
            written,
            heads: Vec::new(),
            measure_rest,
            grace: None,
            slurs: Vec::new(),
            tuplet: None,
            tuplet_start: false,
            tuplet_stop: false,
            lyrics: Vec::new(),
            articulations: Vec::new(),
            tremolo: None,
            beams: Vec::new(),
            arpeggio: None,
        }
    }

    pub fn measure_rest(duration: Tick) -> Self {
        Self::new(duration, Duration(Fraction(1, 1), 0, 0), true)
    }
}

#[derive(Debug, Default)]
pub(super) struct Voice {
    pub notes: BTreeMap<Tick, Note>,
    pub graces: BTreeMap<Tick, Vec<(Tick, Note)>>,
    pub staff_changes: BTreeMap<(Tick, Tick), usize>,
}

impl Voice {
    pub fn staff_at(&self, time: Tick, grace: Tick, home_staff: usize) -> usize {
        self.staff_changes
            .range(..=(time, grace))
            .next_back()
            .map_or(home_staff, |(_, &staff)| staff)
    }
}

pub(super) fn build_voice(events: &[&Event], scale: i64, source: &str, tab: bool) -> Result<Voice> {
    let mut frames = BTreeMap::<(Tick, Tick), Vec<&Event>>::new();
    for event in events {
        // LilyPond can announce a metered tuplet boundary during the grace
        // timestep at that beat. Apply it to the metered notes.
        let grace = match event.kind.as_str() {
            "tuplet" | "tuplet-end" => 0,
            _ => event.grace,
        };
        frames.entry((event.time, grace)).or_default().push(event);
    }

    let mut voice = Voice::default();
    let mut ties = HashMap::<Pitch, ((Tick, Tick), &Event)>::new();
    let mut slurs = Vec::<(bool, u8)>::new();
    let mut tuplet = None;
    let mut last_note = None;
    for ((time, grace), frame) in frames {
        let mut positions = positions(&frame, source)?;
        for event in frame.iter().filter(|event| event.kind == "tuplet-end") {
            let (end, _, _, _) = tuplet
                .take()
                .ok_or_else(|| event.fail(source, "an orphaned tuplet end"))?;
            if end != time {
                return Err(event.fail(source, "an inconsistent tuplet end"));
            }
            voice
                .notes
                .get_mut(&last_note.ok_or_else(invalid_events)?)
                .ok_or_else(invalid_events)?
                .tuplet_stop = true;
        }
        let mut tuplet_start = false;
        for event in frame.iter().filter(|event| event.kind == "tuplet") {
            let (normal, actual, length): (u32, u32, Fraction) = event.fields()?;
            if tuplet.is_some() {
                return Err(event.fail(source, "nested tuplets"));
            }
            let length = ticks(length, scale)?;
            if normal == 0 || actual == 0 || length <= 0 {
                return Err(invalid_events());
            }
            tuplet = Some((
                time.checked_add(length).ok_or_else(invalid_events)?,
                actual,
                normal,
                *event,
            ));
            tuplet_start = true;
        }

        let mut note = None::<Note>;
        let mut representative = None;
        let mut group_tie = None;
        for event in &frame {
            match event.kind.as_str() {
                "note" => {
                    let RawNote(pitch, written, slash, force, cautionary, articulations) =
                        event.fields()?;
                    if pitch.chromatic().is_none() || !(0..=9).contains(&pitch.1) {
                        return Err(event.fail(source, "microtonal or out-of-range pitches"));
                    }
                    representative = Some(*event);
                    let mut candidate = Note::new(ticks(written.0, scale)?, written, false);
                    candidate.grace = (grace != 0).then_some(slash);
                    let note = note.get_or_insert(candidate);
                    if note.written != written
                        || note.measure_rest
                        || note.grace != (grace != 0).then_some(slash)
                        || note.heads.iter().any(|head| head.pitch == pitch)
                    {
                        return Err(event.fail(source, "overlapping notes in a single voice"));
                    }
                    let mut head = Head {
                        pitch,
                        tab: positions.remove(&pitch),
                        force,
                        cautionary,
                        tie_start: false,
                        tie_stop: false,
                        articulations: Vec::new(),
                    };
                    if tab && head.tab.is_none() {
                        return Err(
                            event.fail(source, "a tablature note without a string and fret")
                        );
                    }
                    for art in articulations {
                        match art.0.as_str() {
                            "tie-event" => head.tie_start = true,
                            "articulation-event" => {
                                if let Some(value) = articulation(&art.2) {
                                    head.articulations.push(value);
                                }
                            }
                            "slur-event" | "phrasing-slur-event" => {
                                apply_slur(&mut slurs, note, event, &art.0, art.1, source)?
                            }
                            _ => {}
                        }
                    }
                    note.heads.push(head);
                }
                "rest" | "measure-rest" => {
                    let (written,): (Duration,) = event.fields()?;
                    if note.is_some() {
                        return Err(event.fail(source, "overlapping rests in a single voice"));
                    }
                    representative = Some(*event);
                    note = Some(Note::new(
                        ticks(written.0, scale)?,
                        written,
                        event.kind == "measure-rest",
                    ));
                }
                "tie-event" => group_tie = Some(*event),
                "repeat-tie-event" => return Err(event.fail(source, "explicit repeat ties")),
                _ => {}
            }
        }
        if !positions.is_empty() {
            return Err(invalid_events());
        }

        if let Some(ref mut note) = note {
            // An explicit tie event can belong to just one of several
            // simultaneous notes delivered to this LilyPond voice.
            let tied_heads = frame
                .iter()
                .filter(|event| event.kind == "tied-head")
                .map(|event| event.fields::<(Pitch,)>().map(|(pitch,)| pitch))
                .collect::<Result<HashSet<_>>>()?;
            let event = representative.ok_or_else(invalid_events)?;
            let end = if grace == 0 {
                (
                    time.checked_add(note.duration).ok_or_else(invalid_events)?,
                    0,
                )
            } else {
                (
                    time,
                    grace
                        .checked_add(note.duration)
                        .ok_or_else(invalid_events)?,
                )
            };
            if note.duration <= 0 {
                return Err(event.fail(source, "a nonpositive note duration"));
            }
            if grace == 0 {
                if let Some((end_of_tuplet, actual, normal, _)) = tuplet {
                    if end.0 > end_of_tuplet || note.measure_rest {
                        return Err(event.fail(source, "a note crossing a tuplet boundary"));
                    }
                    note.tuplet = Some((actual, normal));
                    note.tuplet_start = tuplet_start;
                }
            }
            if grace == 0 && !note.measure_rest && !written_duration_matches(note, scale) {
                return Err(event.fail(source, "a scaled note duration outside a supported tuplet"));
            }
            for annotation in &frame {
                match annotation.kind.as_str() {
                    "slur-event" | "phrasing-slur-event" => {
                        let (direction,): (i8,) = annotation.fields()?;
                        apply_slur(
                            &mut slurs,
                            note,
                            annotation,
                            &annotation.kind,
                            direction,
                            source,
                        )?;
                    }
                    "articulation-event" => {
                        let (name,): (String,) = annotation.fields()?;
                        if let Some(value) = articulation(&name) {
                            note.articulations.push(value);
                        }
                    }
                    "tremolo-event" => {
                        let (value,): (u32,) = annotation.fields()?;
                        let division = value.checked_ilog2().ok_or_else(invalid_events)?;
                        let lines = division
                            .checked_sub(i32::from(note.written.1).max(2) as u32)
                            .filter(|lines| (1..=8).contains(lines))
                            .ok_or_else(|| annotation.fail(source, "this tremolo value"))?;
                        note.tremolo = Some(lines);
                    }
                    "beam" => {
                        let (beams,): (Vec<Beam>,) = annotation.fields()?;
                        if !note.beams.is_empty() || beams.len() > 8 {
                            return Err(event.fail(source, "overlapping or invalid beams"));
                        }
                        note.beams = beams;
                    }
                    "arpeggio" => {
                        let (direction,): (i8,) = annotation.fields()?;
                        note.arpeggio = Some(direction);
                    }
                    _ => {}
                }
            }
            for head in &mut note.heads {
                if let Some(&(tie_end, start)) = ties.get(&head.pitch) {
                    if (grace == 0 || start.grace != 0) && tie_end != (time, grace) {
                        return Err(start.fail(source, "a tie without an adjacent matching pitch"));
                    }
                    if tie_end == (time, grace) {
                        ties.remove(&head.pitch);
                        head.tie_stop = true;
                    }
                }
                let group_applies = group_tie.is_some()
                    && (tied_heads.is_empty() || tied_heads.contains(&head.pitch));
                if group_applies || head.tie_start {
                    head.tie_start = true;
                    let start = group_tie.unwrap_or(event);
                    if ties.insert(head.pitch, (end, start)).is_some() {
                        return Err(start.fail(source, "overlapping ties to the same pitch"));
                    }
                }
            }
        } else if let Some(event) = frame.iter().find(|event| {
            matches!(
                event.kind.as_str(),
                "tie-event" | "slur-event" | "phrasing-slur-event"
            )
        }) {
            return Err(event.fail(source, "a tie or slur without a note anchor"));
        }
        if let Some(note) = note {
            if note.grace.is_some() {
                voice.graces.entry(time).or_default().push((grace, note));
                continue;
            }
            if let Some((start, previous)) = voice.notes.last_key_value() {
                if start + previous.duration > time {
                    return Err(representative
                        .ok_or_else(invalid_events)?
                        .fail(source, "overlapping durations in a single voice"));
                }
            }
            last_note = Some(time);
            voice.notes.insert(time, note);
        }
    }
    if let Some((_, event)) = ties
        .values()
        .min_by_key(|(time, event)| (*time, &event.position))
    {
        return Err(event.fail(source, "a tie without an adjacent matching pitch"));
    }
    if let Some((_, _, _, event)) = tuplet {
        return Err(event.fail(source, "an unclosed tuplet"));
    }
    if !slurs.is_empty() {
        return Err(super::unsupported(source, &[], "an unclosed slur"));
    }
    for time in voice.graces.keys() {
        if !voice.notes.contains_key(time) {
            let event = events
                .iter()
                .find(|event| event.time == *time && event.grace != 0 && event.kind == "note")
                .ok_or_else(invalid_events)?;
            return Err(event.fail(
                source,
                "a grace group without a following note at the same beat",
            ));
        }
    }
    Ok(voice)
}

fn written_duration_matches(note: &Note, scale: i64) -> bool {
    let Duration(_, log, dots) = note.written;
    if !(-3..=10).contains(&log) || dots > 8 {
        return false;
    }
    let (actual, normal) = note.tuplet.unwrap_or((1, 1));
    let dot_divisor = 1_i128 << dots;
    let beat_divisor = 1_i128 << log.max(0);
    let beat_multiplier = 1_i128 << (-log).max(0);
    i128::from(note.duration) * i128::from(actual) * beat_divisor * dot_divisor
        == i128::from(scale) * i128::from(normal) * beat_multiplier * (2 * dot_divisor - 1)
}

fn apply_slur(
    slurs: &mut Vec<(bool, u8)>,
    note: &mut Note,
    event: &Event,
    kind: &str,
    direction: i8,
    source: &str,
) -> Result<()> {
    let phrasing = kind == "phrasing-slur-event";
    if direction == -1 {
        let number = (1..=16)
            .find(|number| !slurs.iter().any(|(_, n)| n == number))
            .ok_or_else(|| event.fail(source, "more than sixteen concurrent slurs"))?;
        slurs.push((phrasing, number));
        note.slurs.push((number, "start"));
    } else {
        let index = slurs
            .iter()
            .rposition(|(open, _)| *open == phrasing)
            .ok_or_else(|| event.fail(source, "a slur ending without a start"))?;
        let (_, number) = slurs.remove(index);
        note.slurs.push((number, "stop"));
    }
    Ok(())
}

fn articulation(name: &str) -> Option<&'static str> {
    match name {
        "staccato" => Some("staccato"),
        "staccatissimo" => Some("staccatissimo"),
        "tenuto" => Some("tenuto"),
        "accent" => Some("accent"),
        "marcato" => Some("strong-accent"),
        "portato" => Some("detached-legato"),
        _ => None,
    }
}

#[derive(Default)]
struct Syllable<'a> {
    text: Option<&'a Event>,
    hyphen: bool,
    extender: bool,
}

pub(super) fn attach_lyrics(
    streams: BTreeMap<Id, Vec<&Event>>,
    voices: &mut BTreeMap<Id, Voice>,
    source: &str,
) -> Result<()> {
    let mut verse_count = HashMap::<Id, usize>::new();
    for events in streams.values() {
        let mut syllables = BTreeMap::<Tick, Syllable<'_>>::new();
        let mut target = None;
        for event in events {
            if event.grace != 0 {
                return Err(event.fail(source, "lyrics attached to grace notes"));
            }
            let (voice, _): (Id, String) = event.fields()?;
            if voice == 0 || target.is_some_and(|id| id != voice) {
                return Err(event.fail(source, "lyrics without one identifiable melody voice"));
            }
            target = Some(voice);
            let syllable = syllables.entry(event.time).or_default();
            match event.kind.as_str() {
                "lyric-event" if syllable.text.is_none() => syllable.text = Some(event),
                "hyphen-event" => syllable.hyphen = true,
                "extender-event" => syllable.extender = true,
                _ => return Err(event.fail(source, "multiple syllables at one note in one verse")),
            }
        }
        let Some(id) = target else { continue };
        let voice = voices.get_mut(&id).ok_or_else(|| {
            events[0].fail(source, "lyrics associated with a nonexportable voice")
        })?;
        let verse = verse_count.entry(id).and_modify(|n| *n += 1).or_insert(1);
        let mut within_word = false;
        let mut syllables = syllables.into_iter().peekable();
        while let Some((time, syllable)) = syllables.next() {
            let Some(event) = syllable.text else {
                return Err(events[0].fail(source, "a lyric continuation without an anchor"));
            };
            let (_, text): (Id, String) = event.fields()?;
            let note = voice
                .notes
                .get_mut(&time)
                .filter(|note| !note.heads.is_empty())
                .ok_or_else(|| event.fail(source, "a lyric syllable without a matching note"))?;
            let syllabic = match (within_word, syllable.hyphen) {
                (false, false) => "single",
                (false, true) => "begin",
                (true, true) => "middle",
                (true, false) => "end",
            };
            within_word = syllable.hyphen;
            note.lyrics.push(Lyric {
                verse: *verse,
                syllabic,
                text: Some(text),
                extend: syllable.extender.then_some("start"),
            });
            if syllable.extender {
                let next_syllable = syllables.peek().map_or(Tick::MAX, |(time, _)| *time);
                let mut melisma = voice
                    .notes
                    .range_mut(time.saturating_add(1)..next_syllable)
                    .map(|(_, note)| note)
                    .take_while(|note| !note.heads.is_empty())
                    .peekable();
                while let Some(note) = melisma.next() {
                    let kind = if melisma.peek().is_some() {
                        "continue"
                    } else {
                        "stop"
                    };
                    note.lyrics.push(Lyric {
                        verse: *verse,
                        syllabic: "",
                        text: None,
                        extend: Some(kind),
                    });
                }
            }
        }
    }
    Ok(())
}
