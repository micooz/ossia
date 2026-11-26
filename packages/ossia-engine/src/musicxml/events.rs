use super::Result;
use super::{invalid_events, unsupported};
use crate::RunContext;
use ossia_types::AppError;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) type Id = u32;
pub(super) type Tick = i64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
pub(super) struct Fraction(pub i64, pub i64);

impl Fraction {
    fn scaled(self, scale: i64) -> Result<i64> {
        if self.1 <= 0 || scale % self.1 != 0 {
            return Err(invalid_events());
        }
        self.0
            .checked_mul(scale / self.1)
            .ok_or_else(invalid_events)
    }

    pub fn integer(self) -> Option<i64> {
        (self.1 == 1).then_some(self.0)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
pub(super) struct Pitch(pub u8, pub i8, pub Fraction);

impl Pitch {
    pub fn step(self) -> Result<&'static str> {
        ["C", "D", "E", "F", "G", "A", "B"]
            .get(self.0 as usize)
            .copied()
            .ok_or_else(invalid_events)
    }

    pub fn chromatic(self) -> Option<i64> {
        let step = *[0, 2, 4, 5, 7, 9, 11].get(self.0 as usize)?;
        Some(i64::from(self.1) * 12 + step + self.2.integer()?)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub(super) struct Duration(pub Fraction, pub i8, pub u8);

#[derive(Debug, Deserialize)]
pub(super) struct Articulation(pub String, pub i8, pub String);

#[derive(Debug, Deserialize)]
pub(super) struct RawNote(
    pub Pitch,
    pub Duration,
    pub bool,
    pub bool,
    pub bool,
    pub Vec<Articulation>,
);

#[derive(Debug, Deserialize)]
pub(super) struct Meter(pub Fraction, pub Fraction, pub u32, pub u32);

pub(super) type Key = (Pitch, Vec<(u8, Fraction)>);
pub(super) type Clef = (String, i8, i8);
pub(super) type Repeat = (u32, Vec<u32>, i8);
pub(super) type Harmony = (Vec<Pitch>, Vec<Pitch>);

#[derive(Debug, Deserialize)]
pub(super) struct Context {
    pub id: Id,
    pub parent: Id,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct Metadata {
    pub title: String,
    pub composer: String,
    pub poet: String,
}

#[derive(Debug, Deserialize)]
struct RawEvent(String, Id, (Fraction, Fraction), Vec<usize>, Value);

#[derive(Debug)]
pub(super) struct Event {
    pub id: Id,
    pub time: Tick,
    pub grace: Tick,
    pub position: Vec<usize>,
    pub kind: String,
    fields: Value,
}

impl Event {
    pub fn fields<T: DeserializeOwned>(&self) -> Result<T> {
        decode(&self.fields)
    }

    pub fn fail(&self, source: &str, feature: impl std::fmt::Display) -> AppError {
        unsupported(source, &self.position, feature)
    }
}

pub(super) struct Notation {
    pub metadata: Metadata,
    pub contexts: BTreeMap<Id, Context>,
    pub events: Vec<Event>,
    pub scale: i64,
}

impl Notation {
    pub fn read(data: &str, source: &str, run: &RunContext) -> Result<Self> {
        let mut metadata = None;
        let mut contexts = BTreeMap::new();
        let mut raw = Vec::new();
        let mut scale = 4_i64;
        for (line_index, line) in data.lines().enumerate() {
            if line_index % 1024 == 0 {
                run.check()?;
            }
            let value: Value = serde_json::from_str(line).map_err(|_| invalid_events())?;
            let row = value.as_array().ok_or_else(invalid_events)?;
            match row
                .first()
                .and_then(Value::as_str)
                .ok_or_else(invalid_events)?
            {
                "reject" => {
                    let text = row
                        .get(1)
                        .and_then(Value::as_str)
                        .ok_or_else(invalid_events)?;
                    let position = row.get(2).map(decode).transpose()?.unwrap_or_else(Vec::new);
                    return Err(super::source_error(source, &position, text.into()));
                }
                "metadata" if metadata.is_none() => {
                    let (title, composer, poet) = decode_fields(&row[1..])?;
                    metadata = Some(Metadata {
                        title,
                        composer,
                        poet,
                    });
                }
                "context" => {
                    let (id, parent, kind, name) = decode_fields(&row[1..])?;
                    if id == 0
                        || (parent != 0 && !contexts.contains_key(&parent))
                        || contexts
                            .insert(
                                id,
                                Context {
                                    id,
                                    parent,
                                    kind,
                                    name,
                                },
                            )
                            .is_some()
                    {
                        return Err(invalid_events());
                    }
                }
                _ => {
                    let event: RawEvent =
                        serde_json::from_value(value).map_err(|_| invalid_events())?;
                    if !contexts.contains_key(&event.1) {
                        return Err(invalid_events());
                    }
                    if event.0 == "unsupported" {
                        let (text,): (String,) = decode(&event.4)?;
                        return Err(unsupported(source, &event.3, text));
                    }
                    accumulate_scale(&mut scale, event.2 .0, source, &event.3)?;
                    accumulate_scale(&mut scale, event.2 .1, source, &event.3)?;
                    match event.0.as_str() {
                        "note" => {
                            let note: RawNote = decode(&event.4)?;
                            accumulate_scale(&mut scale, note.1 .0, source, &event.3)?;
                        }
                        "rest" | "measure-rest" | "skip" => {
                            let (duration,): (Duration,) = decode(&event.4)?;
                            // Scaled skips carry timing but no printed duration.
                            if duration.0 .0 > 0 {
                                accumulate_scale(&mut scale, duration.0, source, &event.3)?;
                            }
                        }
                        "grid" => {
                            let meter: Meter = decode(&event.4)?;
                            accumulate_scale(&mut scale, meter.0, source, &event.3)?;
                            accumulate_scale(&mut scale, meter.1, source, &event.3)?;
                        }
                        "tuplet" => {
                            let (_, _, len): (u32, u32, Fraction) = decode(&event.4)?;
                            accumulate_scale(&mut scale, len, source, &event.3)?;
                        }
                        _ => {}
                    }
                    raw.push(event);
                }
            }
        }
        let events = raw
            .into_iter()
            .map(|RawEvent(kind, id, (time, grace), position, fields)| {
                Ok(Event {
                    id,
                    time: time.scaled(scale)?,
                    grace: grace.scaled(scale)?,
                    position,
                    kind,
                    fields,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            metadata: metadata
                .ok_or_else(|| unsupported(source, &[], "a source without an engravable score"))?,
            contexts,
            events,
            scale,
        })
    }
}

fn decode<T: DeserializeOwned>(value: &Value) -> Result<T> {
    T::deserialize(value).map_err(|_| invalid_events())
}

fn decode_fields<T: DeserializeOwned>(values: &[Value]) -> Result<T> {
    decode(&Value::Array(values.to_vec()))
}

fn accumulate_scale(scale: &mut i64, fraction: Fraction, source: &str, at: &[usize]) -> Result<()> {
    let denom = fraction.1;
    if denom <= 0 {
        return Err(invalid_events());
    }
    let mut a = *scale;
    let mut b = denom;
    while b != 0 {
        (a, b) = (b, a % b);
    }
    *scale = (*scale / a)
        .checked_mul(denom)
        .filter(|n| *n <= i64::from(i32::MAX))
        .ok_or_else(|| unsupported(source, at, "this combination of rhythmic subdivisions"))?;
    Ok(())
}

pub(super) fn ticks(time: Fraction, scale: i64) -> Result<Tick> {
    time.scaled(scale)
}
