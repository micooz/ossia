import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { afterEach, beforeEach, test, vi } from 'vite-plus/test';

import { candidateRoots } from '../../../scripts/engine/engine.mjs';
import { fixtureRenderer, fixtures, root } from '../../../scripts/verify/render-fixture.mjs';

vi.mock('node:child_process', () => ({ execFileSync: vi.fn() }));
vi.mock('node:fs', () => ({ readFileSync: vi.fn() }));
vi.mock('../../../scripts/engine/engine.mjs', () => ({ candidateRoots: vi.fn() }));

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubEnv('CARGO_TARGET_DIR', undefined);
  vi.stubEnv('OSSIA_TEST_LILYPOND_ROOT', undefined);
  candidateRoots.mockReturnValue({ roots: [{ dir: path.join(root, 'bundled-engine') }] });
  readFileSync.mockReturnValue('c4');
  execFileSync.mockImplementation((exe) => (exe === 'cargo' ? undefined : '{"pages":[]}'));
});

afterEach(() => vi.unstubAllEnvs());

const name = process.platform === 'win32' ? 'playback-fixture.exe' : 'playback-fixture';

test('builds the fixture binary once and renders each requested source with the bundled engine', () => {
  const render = fixtureRenderer();
  assert.deepEqual(render({ source: 'first.ly' }), { pages: [], source: 'c4' });
  assert.deepEqual(render({ source: 'second.ly' }), { pages: [], source: 'c4' });
  assert.deepEqual(execFileSync.mock.calls[0], [
    'cargo',
    ['build', '--quiet', '-p', 'ossia-engine', '--bin', 'playback-fixture'],
    { cwd: root, stdio: 'inherit' },
  ]);
  assert.equal(execFileSync.mock.calls.length, 3);
  assert.deepEqual(readFileSync.mock.calls, [
    [path.join(fixtures, 'first.ly'), 'utf8'],
    [path.join(fixtures, 'second.ly'), 'utf8'],
  ]);
  assert.deepEqual(execFileSync.mock.calls[1].slice(0, 2), [
    path.join(root, 'target', 'debug', name),
    [path.join(root, 'bundled-engine'), path.join(fixtures, 'first.ly')],
  ]);
});

test('honors engine and target overrides and passes selection mode for absolute library files', () => {
  const engine = path.join(root, 'other-engine');
  const source = path.join(root, '.ossia', 'documents', 'example', 'score.ly');
  vi.stubEnv('OSSIA_TEST_LILYPOND_ROOT', engine);
  vi.stubEnv('CARGO_TARGET_DIR', 'custom-target');
  fixtureRenderer('selection')({ source });
  assert.equal(candidateRoots.mock.calls.length, 0);
  assert.deepEqual(readFileSync.mock.calls[0], [source, 'utf8']);
  assert.deepEqual(execFileSync.mock.calls[1].slice(0, 2), [
    path.join(root, 'custom-target', 'debug', name),
    [engine, source, '--selection'],
  ]);
});

test('requests playback and selection data in the same fixture invocation', () => {
  fixtureRenderer('all')({ source: 'score.ly' });
  assert.deepEqual(execFileSync.mock.calls[1][1], [
    path.join(root, 'bundled-engine'),
    path.join(fixtures, 'score.ly'),
    '--all',
  ]);
});

test('requests the Jianpu audit data from the native executable', () => {
  fixtureRenderer('jianpu')({ source: 'score.ly' });
  assert.equal(execFileSync.mock.calls[1][1].at(-1), '--jianpu');
});

test('propagates a failed build and invalid renderer output', () => {
  execFileSync.mockImplementationOnce(() => {
    throw new Error('cargo failed');
  });
  assert.throws(() => fixtureRenderer(), /cargo failed/);
  assert.equal(readFileSync.mock.calls.length, 0);
  const render = fixtureRenderer();
  execFileSync.mockReturnValueOnce('not-json');
  assert.throws(() => render({ source: 'score.ly' }), SyntaxError);
});
