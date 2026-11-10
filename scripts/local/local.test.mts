import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { parseMidi, resolveRegion } from '../../packages/ly/src/mod.ts';
import { analyzeSource, buildBaseline } from './baseline.mts';
import { fixtures, hash, json, position, readSource, root } from './common.mts';
import type { Fixture } from './fixtures.mts';
import { startHostProbe } from './host.mts';

await test('raw bytes and scalar positions survive normalization', () => {
  const file = path.join(fixtures, 'inputs/unicode-crlf-bom.ly');
  const { text, bytes, sourceHash } = readSource(file);
  assert.ok(text.startsWith('\ufeff'));
  assert.ok(text.includes('\r\n'));
  assert.equal(hash(Buffer.from(text)), sourceHash);
  assert.deepEqual(Buffer.from(text), bytes);
  const raw = analyzeSource('😀\r\n');
  assert.equal(raw.observedUtf16Tokens.length, 3);
  assert.equal(raw.tokens.length, 2);
  assert.deepEqual(raw.tokens[0]!.span.end, { byte: 4, line: 1, column: 2 });
  assert.throws(() => position('😀', 1), /surrogate/);
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-invalid-encoding-'));
  try {
    const invalid = path.join(temporary, 'invalid.ly');
    fs.writeFileSync(invalid, Buffer.from([0xff]));
    assert.throws(() => readSource(invalid), TypeError);
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});

await test('baseline reference is stable and independent of cwd; drift fails', () => {
  const reference = path.join(fixtures, 'observed.json');
  const original = fs.readFileSync(reference);
  assert.equal(json(buildBaseline()), original.toString());
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-reference-'));
  const driftReference = path.join(temporary, 'reference.json');
  const run = () =>
    spawnSync(
      process.execPath,
      [
        '--experimental-strip-types',
        path.join(root, 'scripts/local/baseline.mts'),
        '--check',
        '--reference',
        driftReference,
      ],
      { cwd: os.tmpdir(), encoding: 'utf8' },
    );
  try {
    fs.writeFileSync(driftReference, original);
    assert.equal(run().status, 0);
    fs.writeFileSync(driftReference, '{}\n');
    assert.notEqual(run().status, 0, 'drift must fail');
    fs.writeFileSync(driftReference, original);
    assert.equal(run().status, 0);
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});

await test('parallel staves count each measure once across both pages', () => {
  const render = buildBaseline().renders.find((r) => r.id === 'polyphonic-pages')!;
  assert.equal(render.layouts.length, 2);
  assert.deepEqual(
    render.layouts.map((p) => p.layout.systems[0]!.staffs.length),
    [2, 2],
  );
  assert.deepEqual(
    render.grid.columns.map((c) => [c.page, c.measure]),
    [
      [1, 1],
      [1, 2],
      [2, 3],
      [2, 4],
    ],
  );
  assert.deepEqual(render.grid.pageStart, { 1: 0, 2: 2 });
});

await test('shared examples bind snapshots and artifacts by their actual byte hashes', () => {
  const examples = JSON.parse(
    fs.readFileSync(path.join(fixtures, 'contract-examples.json'), 'utf8'),
  ) as {
    sourceSnapshot: { bytes: number[]; sourceHash: string };
    renderManifest: { sourceHash: string; pages: { sha256: string }[] };
    version: { sourceHash: string };
  };
  const bytes = fs.readFileSync(path.join(fixtures, 'inputs/compressed-rest.ly'));
  assert.deepEqual(Buffer.from(examples.sourceSnapshot.bytes), bytes);
  assert.equal(examples.sourceSnapshot.sourceHash, hash(bytes));
  assert.equal(examples.renderManifest.sourceHash, hash(bytes));
  assert.equal(examples.version.sourceHash, hash(bytes));
  assert.equal(
    examples.renderManifest.pages[0]!.sha256,
    hash(fs.readFileSync(path.join(fixtures, 'engine/compressed-rest/score.svg'))),
  );
});

await test('known TS errors stay distinct from normative expectations', () => {
  const result = buildBaseline();
  const compressed = result.samples.find((s) => s.id === 'compressed-rest')!;
  assert.equal(compressed.expected.measures, 5);
  assert.ok('maxMeasures' in compressed.actual && compressed.actual.maxMeasures === 3);
  assert.ok('voices' in compressed.actual && compressed.actual.voices[0]!.measures.length === 3);
  const render = result.renders.find((r) => r.id === 'compressed-rest')!;
  assert.equal(render.locatorCases.invalidPage?.page, 1);
  assert.deepEqual(render.locatorCases.unknownStaff, ['Upper']);
  assert.deepEqual(render.locatorCases.explicitUnknownStaff, []);
  assert.deepEqual(render.locatorCases.explicitUnknownVoice, []);
  assert.equal(render.locatorCases.outside, null);
  const malformed = result.samples.find((s) => s.id === 'midi-truncated')!;
  assert.equal(malformed.expected.parse, 'error');
  assert.ok('tracks' in malformed.actual && malformed.actual.tracks);
});

await test('MIDI recipes have hand-counted note-ons and controlled malformed behavior', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(fixtures, 'manifest.json'), 'utf8')) as {
    fixtures: Fixture[];
  };
  for (const f of manifest.fixtures.filter((f) => f.kind === 'midi')) {
    const bytes = fs.readFileSync(path.join(fixtures, f.file));
    if (f.expected.notes)
      assert.deepEqual(
        parseMidi(bytes).flatMap((t) => t.notes),
        f.expected.notes,
      );
    if (f.id === 'midi-truncated-vlq') assert.throws(() => parseMidi(bytes), /truncated varlength/);
  }
  const result = buildBaseline();
  assert.ok(result.samples.some((s) => s.id === 'midi-truncated-vlq' && 'error' in s.actual));
  assert.ok(result.renders.length > 0, 'malformed MIDI must not stop later inputs');
});

await test('real engine MIDI cross-checks relative chords and rests', () => {
  const source = readSource(path.join(fixtures, 'inputs/pitch-state.ly')).text;
  const expected = [60, 64, 67, 65, 67, 60];
  const events = resolveRegion(source, 0, source.length).events;
  assert.deepEqual(
    events.flatMap((e) => e.pitches),
    expected,
  );
  const notes = parseMidi(
    fs.readFileSync(path.join(fixtures, 'engine/pitch-state/score.midi')),
  ).flatMap((t) => t.notes);
  assert.deepEqual(
    notes.map((n) => n.note),
    expected,
  );
  assert.deepEqual(
    notes.map((n) => n.time),
    [0, 384, 384, 1152, 1536, 2304],
  );
});

await test('host credentials, origins, view isolation and SSE reconnection', async () => {
  let now = 1000;
  const probe = await startHostProbe({ now: () => now, ttlMs: 100 });
  const api = async (route: string, token?: string, body?: unknown, origin = probe.origin) =>
    fetch(probe.origin + route, {
      method: body ? 'POST' : 'GET',
      headers: {
        Origin: origin,
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
  try {
    const first = probe.launches[0]!;
    const second = probe.launches[1]!;
    assert.equal((await api('/state')).status, 401);
    assert.equal(
      (
        await api(
          '/bootstrap',
          undefined,
          { viewId: first.viewId, credential: first.credential },
          'https://wrong.example',
        )
      ).status,
      403,
    );
    const response = await api('/bootstrap', undefined, {
      viewId: first.viewId,
      credential: first.credential,
    });
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('set-cookie'), null);
    const { token } = (await response.json()) as { token: string };
    assert.equal(
      (await api('/bootstrap', undefined, { viewId: first.viewId, credential: first.credential }))
        .status,
      401,
    );
    const secondResponse = await api('/bootstrap', undefined, {
      viewId: second.viewId,
      credential: second.credential,
    });
    const { token: token2 } = (await secondResponse.json()) as { token: string };
    assert.equal((await api(`/state?viewId=${second.viewId}`, token)).status, 403);
    assert.equal((await api(`/select?viewId=${first.viewId}`, token, { measure: 5 })).status, 400);
    const controller = new AbortController();
    const stream = await fetch(`${probe.origin}/events?viewId=${first.viewId}`, {
      headers: { Authorization: `Bearer ${token}` },
      signal: controller.signal,
    });
    assert.equal(stream.headers.get('content-type'), 'text/event-stream');
    const reader = stream.body!.getReader();
    assert.match(new TextDecoder().decode((await reader.read()).value), /"selection":\[\]/);
    await api(`/select?viewId=${first.viewId}`, token, { measure: 2 });
    assert.match(new TextDecoder().decode((await reader.read()).value), /"selection":\[2\]/);
    const other = (await (await api(`/state?viewId=${second.viewId}`, token2)).json()) as {
      selection: number[];
    };
    assert.deepEqual(other.selection, []);
    controller.abort();
    const reconnect = await fetch(`${probe.origin}/events?viewId=${first.viewId}`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const reconnectedReader = reconnect.body!.getReader();
    assert.match(
      new TextDecoder().decode((await reconnectedReader.read()).value),
      /"selection":\[2\]/,
    );
    await reconnectedReader.cancel();
    assert.equal(probe.events.filter((e) => e.kind === 'stream-open').length, 2);
    now += 3_600_001;
    assert.equal((await api(`/state?viewId=${first.viewId}`, token)).status, 401);
    assert.ok(!json(probe.events).includes(token), 'evidence must not contain credentials');
  } finally {
    await probe.close();
  }
  const expired = await startHostProbe({ now: () => now, ttlMs: 100 });
  try {
    now += 101;
    const launch = expired.launches[0]!;
    const response = await fetch(expired.origin + '/bootstrap', {
      method: 'POST',
      headers: { Origin: expired.origin, 'Content-Type': 'application/json' },
      body: JSON.stringify({ viewId: launch.viewId, credential: launch.credential }),
    });
    assert.equal(response.status, 401);
  } finally {
    await expired.close();
  }
});
