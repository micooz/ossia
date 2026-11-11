use ossia_types::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum MidiDivision {
    Ppqn {
        ticks_per_quarter: u16,
    },
    Smpte {
        frames_per_second: i8,
        ticks_per_frame: u8,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiNote {
    pub note: u8,
    pub time: u64,
    pub channel: u8,
    pub duration: Option<u64>,
    pub duration_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiTrack {
    pub track: usize,
    pub notes: Vec<MidiNote>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiFile {
    pub format: u16,
    pub division: MidiDivision,
    pub tracks: Vec<MidiTrack>,
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], AppError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| AppError::invalid("MIDI length overflow"))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| AppError::invalid("truncated MIDI data"))?;
        self.pos = end;
        Ok(slice)
    }
    fn byte(&mut self) -> Result<u8, AppError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, AppError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, AppError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn vlq(&mut self) -> Result<u32, AppError> {
        let mut n = 0u32;
        for _ in 0..4 {
            let b = self.byte()?;
            n = (n << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(AppError::invalid("MIDI VLQ exceeds four bytes"))
    }
    fn data(&mut self) -> Result<u8, AppError> {
        let b = self.byte()?;
        if b & 0x80 != 0 {
            return Err(AppError::invalid("status byte in MIDI event data"));
        }
        Ok(b)
    }
}

/// Parse SMF formats 0/1/2 with checked chunk/event boundaries. Times are absolute track ticks.
pub fn parse_midi(bytes: &[u8]) -> Result<MidiFile, AppError> {
    let mut r = Reader { bytes, pos: 0 };
    if r.take(4)? != b"MThd" {
        return Err(AppError::invalid("not a Standard MIDI File"));
    }
    let len = r.u32()? as usize;
    if len < 6 {
        return Err(AppError::invalid("short MIDI header"));
    }
    let mut h = Reader {
        bytes: r.take(len)?,
        pos: 0,
    };
    let format = h.u16()?;
    let ntracks = h.u16()?;
    let division = h.u16()?;
    if format > 2 {
        return Err(AppError::unsupported("unknown MIDI format"));
    }
    if ntracks == 0 || (format == 0 && ntracks != 1) {
        return Err(AppError::invalid("invalid MIDI track count"));
    }
    let division = if division & 0x8000 == 0 {
        if division == 0 {
            return Err(AppError::invalid("zero MIDI tick division"));
        }
        MidiDivision::Ppqn {
            ticks_per_quarter: division,
        }
    } else {
        let fps = (division >> 8) as u8 as i8;
        let tpf = division as u8;
        if ![-24, -25, -29, -30].contains(&fps) || tpf == 0 {
            return Err(AppError::invalid("invalid SMPTE division"));
        }
        MidiDivision::Smpte {
            frames_per_second: fps,
            ticks_per_frame: tpf,
        }
    };
    let mut tracks = Vec::new();
    for track in 0..usize::from(ntracks) {
        if r.take(4)? != b"MTrk" {
            return Err(AppError::invalid("bad MIDI track header"));
        }
        let len = r.u32()? as usize;
        let mut t = Reader {
            bytes: r.take(len)?,
            pos: 0,
        };
        let mut notes: Vec<MidiNote> = Vec::new();
        let mut active = HashMap::<(u8, u8), Vec<usize>>::new();
        let mut time = 0u64;
        let mut running = None;
        let mut ended = false;
        while t.pos < t.bytes.len() {
            time = time
                .checked_add(u64::from(t.vlq()?))
                .ok_or_else(|| AppError::invalid("MIDI time overflow"))?;
            let next = *t
                .bytes
                .get(t.pos)
                .ok_or_else(|| AppError::invalid("missing MIDI event"))?;
            let status = if next & 0x80 != 0 {
                t.pos += 1;
                next
            } else {
                running.ok_or_else(|| AppError::invalid("no MIDI running status"))?
            };
            match status {
                0x80..=0xef => {
                    running = Some(status);
                    let first = t.data()?;
                    let second = if status & 0xf0 != 0xc0 && status & 0xf0 != 0xd0 {
                        Some(t.data()?)
                    } else {
                        None
                    };
                    let key = (status & 0xf, first);
                    if status & 0xf0 == 0x90 && second.unwrap_or(0) > 0 {
                        let pending = active.entry(key).or_default();
                        if !pending.is_empty() {
                            for &i in pending.iter() {
                                notes[i].duration_reason =
                                    Some("overlapping same-pitch note-ons".into());
                            }
                        }
                        let ambiguous = !pending.is_empty();
                        pending.push(notes.len());
                        notes.push(MidiNote {
                            note: first,
                            time,
                            channel: status & 0xf,
                            duration: None,
                            duration_reason: Some(
                                if ambiguous {
                                    "overlapping same-pitch note-ons"
                                } else {
                                    "missing note-off"
                                }
                                .into(),
                            ),
                        });
                    } else if status & 0xf0 == 0x80 || status & 0xf0 == 0x90 {
                        if let Some(pending) = active.get_mut(&key) {
                            if pending.len() == 1 {
                                let n = &mut notes[pending[0]];
                                if n.duration_reason.as_deref() == Some("missing note-off") {
                                    n.duration = Some(time - n.time);
                                    n.duration_reason = None;
                                }
                            }
                            if !pending.is_empty() {
                                pending.remove(0);
                            }
                        }
                    }
                }
                0xff => {
                    running = None;
                    let kind = t.data()?;
                    let len = t.vlq()? as usize;
                    t.take(len)?;
                    if kind == 0x2f {
                        if len != 0 || t.pos != t.bytes.len() {
                            return Err(AppError::invalid("invalid end-of-track event"));
                        }
                        ended = true;
                    }
                }
                0xf0 | 0xf7 => {
                    running = None;
                    let len = t.vlq()? as usize;
                    t.take(len)?;
                }
                _ => return Err(AppError::invalid("unsupported status in MIDI track")),
            }
        }
        if !ended {
            return Err(AppError::invalid("MIDI track has no end-of-track event"));
        }
        tracks.push(MidiTrack { track, notes });
    }
    if r.pos != bytes.len() {
        return Err(AppError::invalid("unexpected bytes after MIDI tracks"));
    }
    Ok(MidiFile {
        format,
        division,
        tracks,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteChange {
    pub index: usize,
    pub before: Option<MidiNote>,
    pub after: Option<MidiNote>,
    pub pitch_changed: bool,
    pub start_changed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackChange {
    pub track: usize,
    pub before_count: usize,
    pub after_count: usize,
    pub track_added: bool,
    pub track_removed: bool,
    pub notes: Vec<NoteChange>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiDiff {
    pub tracks: Vec<TrackChange>,
    pub limitations: String,
}

/// Compare file-order note-ons per track. PPQN differences are normalized to quarter-note time.
/// This does not compare note-offs, durations, velocity, tempo maps or musical intent.
pub fn diff_midi(before: &MidiFile, after: &MidiFile) -> Result<MidiDiff, AppError> {
    if before.format == 2 || after.format == 2 {
        return Err(AppError::unsupported(
            "independent format-2 timelines cannot be compared",
        ));
    }
    let (a_ppqn, b_ppqn) = match (before.division, after.division) {
        (
            MidiDivision::Ppqn {
                ticks_per_quarter: a,
            },
            MidiDivision::Ppqn {
                ticks_per_quarter: b,
            },
        ) if a > 0 && b > 0 => (a, b),
        _ => {
            return Err(AppError::unsupported(
                "MIDI comparison requires nonzero PPQN timelines",
            ))
        }
    };
    let mut tracks = Vec::new();
    for track in 0..before.tracks.len().max(after.tracks.len()) {
        let a = before.tracks.get(track);
        let b = after.tracks.get(track);
        let an = a.map_or(&[][..], |t| t.notes.as_slice());
        let bn = b.map_or(&[][..], |t| t.notes.as_slice());
        let mut notes = Vec::new();
        for index in 0..an.len().max(bn.len()) {
            let x = an.get(index);
            let y = bn.get(index);
            let pitch_changed = x.map(|n| n.note) != y.map(|n| n.note);
            let start_changed = match (x, y) {
                (Some(x), Some(y)) => {
                    u128::from(x.time) * u128::from(b_ppqn)
                        != u128::from(y.time) * u128::from(a_ppqn)
                }
                _ => true,
            };
            if pitch_changed || start_changed || x.map(|n| n.channel) != y.map(|n| n.channel) {
                notes.push(NoteChange {
                    index,
                    before: x.cloned(),
                    after: y.cloned(),
                    pitch_changed,
                    start_changed,
                });
            }
        }
        if !notes.is_empty() || a.is_none() || b.is_none() {
            tracks.push(TrackChange {
                track,
                before_count: an.len(),
                after_count: bn.len(),
                track_added: a.is_none(),
                track_removed: b.is_none(),
                notes,
            });
        }
    }
    Ok(MidiDiff {
        tracks,
        limitations:
            "Note-ons only; excludes note-offs, duration, velocity, tempo maps and musical intent."
                .into(),
    })
}
