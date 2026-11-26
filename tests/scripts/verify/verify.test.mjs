import assert from 'node:assert/strict';

import { beforeEach, test, vi } from 'vite-plus/test';

import { runAuditionCases } from '../../../scripts/verify/audition-cases.mjs';
import { runJianpuCases } from '../../../scripts/verify/jianpu-cases.mjs';
import { runPlaybackCases } from '../../../scripts/verify/playback-cases.mjs';
import { fixtureRenderer } from '../../../scripts/verify/render-fixture.mjs';
import { runSelectionCases } from '../../../scripts/verify/selection-cases.mjs';
import { runVerification } from '../../../scripts/verify/verify.mjs';

vi.mock('../../../scripts/verify/audition-cases.mjs', () => ({ runAuditionCases: vi.fn() }));
vi.mock('../../../scripts/verify/jianpu-cases.mjs', () => ({ runJianpuCases: vi.fn() }));
vi.mock('../../../scripts/verify/playback-cases.mjs', () => ({ runPlaybackCases: vi.fn() }));
vi.mock('../../../scripts/verify/render-fixture.mjs', () => ({
  fixtureRenderer: vi.fn(),
  fixtures: '/fixtures',
}));
vi.mock('../../../scripts/verify/selection-cases.mjs', () => ({ runSelectionCases: vi.fn() }));

beforeEach(() => vi.resetAllMocks());

test('renders each source once and shares its data across all three suites', async () => {
  const renderFixture = vi.fn(({ source }) => ({ source }));
  fixtureRenderer.mockReturnValue(renderFixture);
  const checked = [];
  runPlaybackCases.mockImplementation((ids, render) => {
    assert.deepEqual(ids, []);
    checked.push(render({ source: 'score.ly' }), render({ source: 'other.ly' }));
    return 0;
  });
  runAuditionCases.mockImplementation((ids, render) => {
    assert.deepEqual(ids, []);
    assert.equal(render({ source: 'other.ly' }), checked[1]);
    assert.equal(render({ source: './score.ly' }), checked[0]);
    return 0;
  });
  runSelectionCases.mockImplementation(async (ids, render) => {
    assert.deepEqual(ids, []);
    assert.equal(render({ source: 'score.ly' }), checked[0]);
    assert.equal(render({ source: 'other.ly' }), checked[1]);
    return 0;
  });

  assert.equal(await runVerification(), 0);
  assert.equal(runJianpuCases.mock.calls.length, 1);
  assert.deepEqual(fixtureRenderer.mock.calls, [['all']]);
  assert.deepEqual(renderFixture.mock.calls, [[{ source: 'score.ly' }], [{ source: 'other.ly' }]]);
  assert.equal(runSelectionCases.mock.calls.length, 1);
});

test('stops after a failed audit and returns its status', async () => {
  runPlaybackCases.mockReturnValue(0);
  runAuditionCases.mockReturnValue(1);
  assert.equal(await runVerification(), 1);
  assert.equal(runSelectionCases.mock.calls.length, 0);
});

test('passes selected case IDs to every suite', async () => {
  const ids = ['first', 'second'];
  assert.equal(await runVerification(ids), 0);
  assert.equal(runJianpuCases.mock.calls.length, 0);
  for (const suite of [runPlaybackCases, runAuditionCases, runSelectionCases])
    assert.deepEqual(
      suite.mock.calls.map(([ids]) => ids),
      [['first', 'second']],
    );
});
