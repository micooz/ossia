// Minimal Standard MIDI File reader: note-ons per track, in file order.
// Ported from `work/p1-4/midi.mjs` with two fixes found while probing the vendored engine:
// meta-event lengths are varlength (not one byte), and a track may end mid-event.
// Used by the pitch state-machine harness and by gates G-d / G-e (whole-file MIDI diff).

export interface NoteOn {
  note: number;
  time: number;
}
export interface SmfTrack {
  track: number;
  notes: NoteOn[];
}

const NAMES = ['C', 'Db', 'D', 'Eb', 'E', 'F', 'Gb', 'G', 'Ab', 'A', 'Bb', 'B'];
export const name = (n: number) => `${NAMES[n % 12]}${Math.floor(n / 12) - 1}`;

export function parseMidi(buf: Buffer): SmfTrack[] {
  if (buf.subarray(0, 4).toString('latin1') !== 'MThd') throw new Error('not SMF');
  const ntracks = buf.readUInt16BE(10);
  let p = 14;
  const out: SmfTrack[] = [];
  for (let t = 0; t < ntracks; t++) {
    if (buf.subarray(p, p + 4).toString('latin1') !== 'MTrk') throw new Error('bad track header');
    const len = buf.readUInt32BE(p + 4);
    p += 8;
    const end = p + len;
    let running: number | null = null;
    let time = 0;
    const notes: NoteOn[] = [];
    const varlen = (): number => {
      let d = 0;
      for (;;) {
        const x = buf[p++];
        if (x === undefined) throw new Error('truncated varlength');
        d = (d << 7) | (x & 0x7f);
        if (!(x & 0x80)) break;
      }
      return d;
    };
    while (p < end) {
      time += varlen();
      let st = buf[p]!;
      if (st & 0x80) {
        p++;
        running = st;
      } else {
        st = running!;
      }
      if (st == null) throw new Error('no running status');
      const hi = st & 0xf0;
      if (hi === 0x90 || hi === 0x80) {
        const note = buf[p]!,
          vel = buf[p + 1]!;
        p += 2;
        if (hi === 0x90 && vel > 0) notes.push({ note, time });
      } else if (hi >= 0x80 && hi <= 0xe0) p += hi === 0xc0 || hi === 0xd0 ? 1 : 2;
      // `varlen()` mutates `p`, so its return value must be added in a separate statement:
      // `p += varlen()` would read the stale `p` first and rewind past the length prefix.
      else if (st === 0xff) {
        p += 1;
        const l = varlen();
        p += l;
      } else if (st === 0xf0 || st === 0xf7) {
        const l = varlen();
        p += l;
      } else throw new Error(`unknown status ${st.toString(16)}`);
    }
    p = end;
    out.push({ track: t, notes });
  }
  return out;
}

// Compare two parsed track lists so an assertion can say "only the target measure of the
// target voice changed" (G-d blast radius / G-e semantic channel).
export interface TrackChange {
  track: number;
  kind: 'count' | 'pitch';
  before?: number;
  after?: number;
  diffs?: { i: number; before: string; after: string }[];
  nDiffs?: number;
}

export function diffTracks(before: SmfTrack[], after: SmfTrack[]): TrackChange[] {
  const changes: TrackChange[] = [];
  const n = Math.max(before.length, after.length);
  for (let t = 0; t < n; t++) {
    const a = before[t]?.notes ?? [],
      b = after[t]?.notes ?? [];
    if (a.length !== b.length) {
      changes.push({ track: t, kind: 'count', before: a.length, after: b.length });
      continue;
    }
    const diffs: { i: number; before: string; after: string }[] = [];
    for (let i = 0; i < a.length; i++) {
      if (a[i]!.note !== b[i]!.note || a[i]!.time !== b[i]!.time) {
        diffs.push({
          i,
          before: `${name(a[i]!.note)}@${a[i]!.time}`,
          after: `${name(b[i]!.note)}@${b[i]!.time}`,
        });
      }
    }
    if (diffs.length)
      changes.push({ track: t, kind: 'pitch', nDiffs: diffs.length, diffs: diffs.slice(0, 6) });
  }
  return changes;
}
