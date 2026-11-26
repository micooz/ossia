import { spawnSync } from 'node:child_process';

import { candidateRoots } from '../engine/engine.mjs';
import { root } from './render-fixture.mjs';

const engine = process.env.OSSIA_TEST_LILYPOND_ROOT ?? candidateRoots(root).roots[0].dir;
const test = spawnSync(
  'cargo',
  ['test', '-p', 'ossia-engine', '--test', 'native_musicxml', '--', '--ignored'],
  {
    cwd: root,
    env: { ...process.env, OSSIA_TEST_LILYPOND_ROOT: engine },
    stdio: 'inherit',
  },
);
if (test.error) throw test.error;
process.exitCode = test.status ?? 1;
