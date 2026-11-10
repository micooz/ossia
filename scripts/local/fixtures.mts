import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { fixtures, json, write } from './common.mts';

export interface Fixture {
  id: string;
  file: string;
  kind: 'ly' | 'xml' | 'midi';
  covers: string[];
  expected: Record<string, unknown>;
}

const version = '\\version "2.26.0"\n';
const score = (music: string) =>
  `${version}\\score { \\new Staff { ${music} } \\layout {} \\midi {} }\n`;
const note = (step: string, duration = 4, voice = 1) =>
  `<note><pitch><step>${step}</step><octave>4</octave></pitch><duration>${duration}</duration><voice>${voice}</voice><type>whole</type></note>`;
const xml = (music: string) => `<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
<score-partwise version="4.0"><work><work-title>Ossia synthetic</work-title></work>
<part-list><score-part id="P1"><part-name>Synthetic</part-name></score-part></part-list>
<part id="P1"><measure number="1"><attributes><divisions>1</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>${music}</measure></part></score-partwise>\n`;

function smf(events: number[]) {
  const header = Buffer.from('4d54686400000006000100010060', 'hex');
  const track = Buffer.alloc(8);
  track.write('MTrk');
  track.writeUInt32BE(events.length, 4);
  return Buffer.concat([header, track, Buffer.from(events)]);
}

export function generateFixtures() {
  const manifest: Fixture[] = [];
  const add = (
    id: string,
    kind: Fixture['kind'],
    data: string | Uint8Array,
    covers: string[],
    expected: Fixture['expected'],
  ) => {
    const file = `inputs/${id}.${kind === 'xml' ? 'musicxml' : kind === 'midi' ? 'mid' : 'ly'}`;
    write(path.join(fixtures, file), data);
    manifest.push({ id, file, kind, covers, expected });
  };
  add(
    'compressed-rest',
    'ly',
    `${version}Upper = \\relative c' { c1 | % 1\n R1*4 | % 5\n}\n\\score { \\new Staff = "upper" \\Upper \\layout {} \\midi {} }\n`,
    ['converter-style', 'compressed-rest', 'score-tail'],
    {
      capability: 'measure',
      measures: 5,
      voiceCount: 1,
      spans: [
        [1, 1],
        [2, 5],
      ],
      voiceEnd: 'closing expression brace; excludes score block',
    },
  );
  add(
    'polyphonic-pages',
    'ly',
    `${version}\\paper { ragged-last = ##t }\nUpper = \\relative c' { c1 | % 1\n d1 | % 2\n \\pageBreak e1 | % 3\n f1 | % 4\n}\nSecond = \\relative c' { g1 | % 1\n a1 | % 2\n b1 | % 3\n c1 | % 4\n}\nLower = \\relative c { c1 | % 1\n d1 | % 2\n e1 | % 3\n f1 | % 4\n}\n\\score { \\new PianoStaff <<\n \\new Staff = "upper" << \\context Voice = "one" { \\voiceOne \\Upper } \\context Voice = "two" { \\voiceTwo \\Second } >>\n \\new Staff = "lower" { \\clef bass \\context Voice = "three" { \\Lower } }\n>> \\layout {} \\midi {} }\n`,
    ['two-staves', 'multiple-voices', 'multiple-pages', 'staff-order'],
    {
      capability: 'measure',
      pages: 2,
      measures: 4,
      staffCount: 2,
      voiceCount: 3,
      upperPitches: [60, 62, 64, 65],
      lowerPitches: [48, 50, 52, 53],
    },
  );
  add(
    'named-variable',
    'ly',
    `${version}Music = { c'1 | d'1 | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`,
    ['named-variable'],
    { capability: 'measure', measures: 2 },
  );
  add(
    'pitch-state',
    'ly',
    `${version}Music = \\relative c' { c4 <e g>4 r4 f4 | g2 c,2 | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`,
    ['relative', 'chord-reference', 'rest-reference'],
    { pitches: [60, 64, 67, 65, 67, 60], measures: 2 },
  );
  add(
    'midi-context-reuse',
    'ly',
    `${version}trackBchannelA = \\relative c' { c1 | d1 | }\ntrackBchannelB = \\relative c { c1 | d1 | }\ntrackA = << \\context Voice = voiceA \\trackBchannelA >>\ntrackB = << \\context Voice = voiceB \\trackBchannelB >>\n\\score { << \\context Staff=trackB \\trackA \\context Staff=trackB \\trackB >> \\layout {} \\midi {} }\n`,
    ['unquoted-context', 'reused-staff'],
    { staffCount: 1, voiceCount: 2, staffVoices: ['trackBchannelA', 'trackBchannelB'] },
  );
  add('inline-staff', 'ly', score("c'1 | d'1 |"), ['inline-staff'], {
    capability: 'measure',
    measures: 2,
  });
  add(
    'absolute',
    'ly',
    `${version}Music = \\absolute { c'4 e'4 g'4 r4 | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`,
    ['absolute', 'rest'],
    { capability: 'measure', pitches: [60, 64, 67], measures: 1 },
  );
  add(
    'same-line',
    'ly',
    `${version}Music = \\relative c' { c1 | d1 | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`,
    ['same-line-measures'],
    { capability: 'measure', measures: 2, distinctByteSpans: true },
  );
  add(
    'unicode-crlf-bom',
    'ly',
    `\ufeff${version}% 中文😀\nMusic = \\relative c' { c4^\\markup { "中文😀" }\td4 e4 f4 | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`.replaceAll(
      '\n',
      '\r\n',
    ),
    ['unicode', 'emoji', 'CRLF', 'BOM', 'tab', 'markup'],
    { roundTrip: true, preserveBytes: true },
  );
  add('raw-emoji', 'ly', '😀\r\n', ['surrogate-boundary', 'invalid-lilypond'], {
    compile: 'error',
    normalizedRawTokens: 2,
    roundTrip: true,
  });
  add(
    'scheme-markup',
    'ly',
    `${version}#(define label "{ c4 | } 中文😀")\n% c4 | is only a comment\nMusic = \\relative c' { c1^\\markup { "not music: d4 |" } | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`,
    ['scheme-string', 'markup-string', 'comment'],
    { capability: 'measure', pitches: [60], measures: 1 },
  );
  add(
    'chordmode',
    'ly',
    `${version}Music = \\chordmode { c1 | g1:7 | }\n\\score { \\new ChordNames \\Music \\layout {} }\n`,
    ['chordmode'],
    { pitchCapability: 'unsupported', capability: 'anchor-only' },
  );
  add(
    'repeat',
    'ly',
    `${version}Music = \\relative c' { \\repeat volta 2 { c1 | d1 | } }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`,
    ['repeat'],
    { pitchCapability: 'unsupported', capability: 'anchor-only' },
  );
  add(
    'transpose',
    'ly',
    `${version}Music = \\transpose c d \\relative c' { c1 | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }\n`,
    ['transpose'],
    { pitchCapability: 'unsupported', capability: 'anchor-only' },
  );
  add(
    'standard-include',
    'ly',
    `${version}\\include "english.ly"\n\\score { \\new Staff { cs'1 | } \\layout {} \\midi {} }\n`,
    ['standard-include'],
    { compile: 'success', singleFile: true },
  );
  for (const id of ['external-missing', 'external-present'])
    add(
      id,
      'ly',
      `${version}\\include "user-music.ily"\n\\score { \\new Staff \\UserMusic \\layout {} \\midi {} }\n`,
      [id, 'external-include'],
      { singleFile: false, capability: 'unsupported-source' },
    );
  add('xml-clean', 'xml', xml(note('C')), ['MusicXML', 'clean'], {
    import: 'clean',
    pitches: [60],
  });
  add(
    'xml-loss',
    'xml',
    xml(
      '<direction><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>not-a-number</per-minute></metronome></direction-type></direction>' +
        note('C'),
    ),
    ['MusicXML', 'conversion-loss'],
    { import: 'needs-confirmation', warning: 'Invalid bpm' },
  );
  add(
    'xml-negative-skip',
    'xml',
    xml(note('C') + '<backup><duration>2</duration></backup>' + note('D', 2)),
    ['MusicXML', 'blocking-timeline'],
    { import: 'refused', warning: 'Negative skip' },
  );
  add(
    'midi-valid',
    'midi',
    smf([0, 0x90, 60, 64, 96, 0x80, 60, 0, 0, 0xff, 0x2f, 0]),
    ['MIDI', 'conversion'],
    { notes: [{ note: 60, time: 0 }] },
  );
  add(
    'midi-running-zero',
    'midi',
    smf([0, 0x90, 60, 64, 96, 62, 64, 96, 62, 0, 0, 0xff, 0x2f, 0]),
    ['running-status', 'zero-velocity'],
    {
      notes: [
        { note: 60, time: 0 },
        { note: 62, time: 96 },
      ],
    },
  );
  add(
    'midi-long-meta',
    'midi',
    smf([
      0,
      0xff,
      1,
      0x81,
      0,
      ...Array<number>(128).fill(65),
      0,
      0x90,
      65,
      64,
      96,
      0x80,
      65,
      0,
      0,
      0xff,
      0x2f,
      0,
    ]),
    ['variable-meta-length'],
    { notes: [{ note: 65, time: 0 }] },
  );
  add('midi-truncated', 'midi', smf([0, 0x90, 60, 64]).subarray(0, 25), ['truncated-track'], {
    parse: 'error',
  });
  add('midi-truncated-vlq', 'midi', smf([0x81]), ['truncated-varlength'], { parse: 'error' });
  write(
    path.join(fixtures, 'manifest.json'),
    json({
      formatVersion: 1,
      provenance: 'Original synthetic inputs; regenerate with pnpm baseline:fixtures',
      fixtures: manifest,
    }),
  );
  return manifest;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  console.log(`Generated ${generateFixtures().length} synthetic fixtures`);
}
