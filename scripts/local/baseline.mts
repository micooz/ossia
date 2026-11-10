import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  buildIndex,
  buildLayout,
  diffTracks,
  parseMidi,
  readSvg,
  resolveRegion,
  serialize,
  tokenize,
  TT,
} from '../../packages/ly/src/mod.ts';
import { fixtures, hash, json, readSource, span, write } from './common.mts';
import type { Fixture } from './fixtures.mts';
import { checkImport } from './reference/import-gate.mts';
import {
  locatePoint,
  locatorTable,
  resolveLocator,
  voicesForMeasure,
} from './reference/locate.mts';

export function analyzeSource(source: string) {
  const observed = tokenize(source);
  assert.equal(serialize(observed), source);
  const normalized: { kind: string; mode: string; raw: string; span: ReturnType<typeof span> }[] =
    [];
  for (let i = 0; i < observed.length; i++) {
    const t = observed[i]!;
    let raw = t.raw;
    const next = observed[i + 1];
    if (
      t.t === TT.RAW &&
      /^[\uD800-\uDBFF]$/.test(raw) &&
      next?.t === TT.RAW &&
      next.mode === t.mode &&
      /^[\uDC00-\uDFFF]$/.test(next.raw)
    ) {
      raw += next.raw;
      i++;
    }
    const range = span(source, t.s, t.s + raw.length);
    assert.equal(
      Buffer.from(source).subarray(range.byteStart, range.byteEnd).toString('utf8'),
      raw,
    );
    normalized.push({ kind: t.t, mode: t.mode, raw, span: range });
  }
  assert.equal(normalized.map((t) => t.raw).join(''), source);
  const index = buildIndex(source);
  const pitches = (from: number, to: number) => {
    const result = resolveRegion(source, from, to);
    return {
      ...result,
      events: result.events.map(({ s, ...event }) => ({
        ...event,
        observedUtf16Offset: s,
        span: span(source, s, s + event.raw.length),
      })),
    };
  };
  const voices = index.voices.map((v) => ({
    name: v.name,
    span: span(source, v.from, v.to),
    measures: v.measures.map((m) => ({ ...m, span: span(source, m.from, m.to) })),
    pitch: pitches(v.from, v.to),
  }));
  return {
    sourceHash: hash(Buffer.from(source)),
    observedUtf16Tokens: observed,
    tokens: normalized,
    voices,
    maxMeasures: index.maxMeasures,
    totalBars: index.totalBars,
    staffOrder: locatorTable(source, []).staffs,
    wholeSourcePitch: pitches(0, source.length),
  };
}

function midiResult(bytes: Buffer) {
  try {
    const tracks = parseMidi(bytes);
    return { tracks, selfDiff: diffTracks(tracks, tracks) };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

export function buildBaseline() {
  const manifest = JSON.parse(fs.readFileSync(path.join(fixtures, 'manifest.json'), 'utf8')) as {
    fixtures: Fixture[];
  };
  const samples = manifest.fixtures.map((f) => {
    const file = path.join(fixtures, f.file);
    const bytes = fs.readFileSync(file);
    const actual =
      f.kind === 'ly'
        ? analyzeSource(readSource(file).text)
        : f.kind === 'midi'
          ? midiResult(bytes)
          : { inputBytes: bytes.length };
    return { id: f.id, expected: f.expected, actual };
  });
  const derivedFile = path.join(fixtures, 'engine/manifest.json');
  assert.ok(
    fs.existsSync(derivedFile),
    'run pnpm probe:engine to create the engine reference inputs',
  );
  const derived = JSON.parse(fs.readFileSync(derivedFile, 'utf8')) as {
    id: string;
    source: string;
    pages: string[];
    midi?: string;
    log: string[];
  }[];
  const renders = derived.map((f) => {
    const source = readSource(path.join(fixtures, f.source)).text;
    const pages = f.pages.map((file) => fs.readFileSync(path.join(fixtures, file), 'utf8'));
    const table = locatorTable(source, pages);
    const first = table.pages[0]?.layout;
    const column = first?.columns[0];
    const staff = first?.systems[0]?.staffs[0];
    const point =
      column && staff ? { x: (column.x0 + column.x1) / 2, y: (staff.top + staff.bot) / 2 } : null;
    return {
      id: f.id,
      source: analyzeSource(source),
      importReport: checkImport(source, f.log),
      layouts: pages.map((svg, i) => ({
        page: i + 1,
        layout: buildLayout(svg),
        anchors: readSvg(svg)
          .filter((e) => e.anchor)
          .map((e) => e.anchor),
      })),
      grid: table.grid,
      midi: f.midi ? midiResult(fs.readFileSync(path.join(fixtures, f.midi))) : null,
      locatorCases: {
        point,
        firstPage: point ? locatePoint(table, 1, point.x, point.y) : null,
        invalidPage: point ? locatePoint(table, 999, point.x, point.y) : null,
        unknownStaff: voicesForMeasure(table, 999, 1),
        explicitUnknownStaff: resolveLocator(table, { staff: 999, measureStart: 1 }),
        explicitUnknownVoice: resolveLocator(table, { voice: 'missing', measureStart: 1 }),
        outside: locatePoint(table, 1, -1000, -1000),
      },
    };
  });
  return { formatVersion: 1, engineVersion: '2.26.0', samples, renders };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const result = json(buildBaseline());
  const referenceFlag = process.argv.indexOf('--reference');
  const reference =
    referenceFlag >= 0
      ? path.resolve(process.argv[referenceFlag + 1] ?? '')
      : path.join(fixtures, 'observed.json');
  assert.ok(referenceFlag < 0 || process.argv[referenceFlag + 1], '--reference requires a path');
  if (process.argv.includes('--check')) {
    const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-baseline-'));
    try {
      const actual = path.join(temp, 'observed.json');
      write(actual, result);
      assert.equal(
        fs.readFileSync(actual, 'utf8'),
        fs.readFileSync(reference, 'utf8'),
        'baseline drift; inspect changes before regenerating',
      );
    } finally {
      fs.rmSync(temp, { recursive: true, force: true });
    }
    console.log('Local baseline matches reference');
  } else {
    write(reference, result);
    console.log('Rebuilt tests/fixtures/local/observed.json');
  }
}
