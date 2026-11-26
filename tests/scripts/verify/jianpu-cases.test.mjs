import assert from 'node:assert/strict';

import { test, vi } from 'vite-plus/test';

import { auditJianpu, runJianpuCases } from '../../../scripts/verify/jianpu-cases.mjs';

const unsupported = {
  formatErrors: ['svg', 'pdf'].map((format) => ({
    format,
    error: {
      code: 'unsupported-source',
      message: 'Jianpu layout does not support a key change',
      detail: { position: { line: 5, column: 5 } },
    },
  })),
  lineOffset: 1,
  pageCount: 0,
  pdf: false,
};

test('matches an expected rejection against both visual formats and the original source line', () => {
  const report = auditJianpu(unsupported, { unsupported: 'key change', line: 4 });
  assert.equal(report.status, 'expected-unsupported');
  assert.deepEqual(
    report.diagnostics.map(({ line, column }) => [line, column]),
    [
      [4, 5],
      [4, 5],
    ],
  );
  assert.equal(
    auditJianpu(unsupported, { unsupported: 'key change', line: 5 }).status,
    'unsupported',
  );
  assert.equal(
    auditJianpu({ ...unsupported, pageCount: 1 }, { unsupported: 'key change' }).status,
    'unsupported',
  );
  assert.equal(
    auditJianpu({}, { unsupported: 'key change' }).issues[0].kind,
    'expected-unsupported-now-renders',
  );
});

test('an exploratory scan reports unsupported files and continues after a failure', async () => {
  const consoleLog = vi.spyOn(console, 'log').mockImplementation(() => {});
  try {
    const render = vi
      .fn()
      .mockImplementationOnce(() => {
        throw new Error('missing file');
      })
      .mockReturnValueOnce(unsupported);
    assert.equal(await runJianpuCases(['--library', 'missing.ly', 'other.ly'], render), 1);
    assert.equal(render.mock.calls.length, 2);
    const reports = consoleLog.mock.calls.map(([line]) => JSON.parse(line));
    assert.equal(reports[0].issues[0].kind, 'audit-error');
    assert.equal(reports[1].status, 'unsupported');
    assert.deepEqual(reports[2], {
      total: 2,
      passed: 0,
      'expected-unsupported': 0,
      unsupported: 1,
      failed: 1,
    });
  } finally {
    consoleLog.mockRestore();
  }
});

test('finds colliding simultaneous numbers and an incomplete measure grid', () => {
  const object = (line, kind) => ({
    kind,
    staffIndex: 0,
    span: { start: { line } },
    timing: { numerator: 1, denominator: 4 },
    bbox: { x0: 10, x1: 12, y0: 10, y1: 12 },
  });
  const bundle = {
    jianpu: {
      pdf: true,
      sameMidi: true,
      samePlayback: true,
      samePrintedRhythm: true,
      lineOffset: 1,
    },
    layout: {
      sourceHash: 'source',
      capability: 'measure',
      reason: 'partial grid',
      pages: [{ page: 1, systems: [], objects: [object(3, 'Rest'), object(4, 'NoteHead')] }],
    },
    selection: { reliable: true, staves: [], voices: [], pages: [] },
    geometry: { renderKey: 'render', sourceHash: 'source', pages: [] },
    playback: { renderKey: 'render', sourceHash: 'source', notes: [], groups: [] },
  };
  const { status, issues } = auditJianpu(bundle);
  assert.equal(status, 'failed');
  assert.ok(issues.some(({ kind }) => kind === 'incomplete-measure-mapping'));
  assert.deepEqual(issues.find(({ kind }) => kind === 'overlapping-rhythms')?.lines, [2, 3]);
  const changed = auditJianpu(
    { ...bundle, jianpu: { ...bundle.jianpu, samePlayback: false, samePrintedRhythm: false } },
    { compareOriginal: true },
  );
  assert.ok(changed.issues.some(({ kind }) => kind === 'different-browser-playback'));
  assert.ok(changed.issues.some(({ kind }) => kind === 'different-printed-rhythm'));
});
