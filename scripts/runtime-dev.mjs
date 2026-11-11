import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { candidateRoots } from './engine.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const child = spawn(
  'cargo',
  [
    'run',
    '-p',
    'ossia-desktop',
    '--no-default-features',
    '--',
    '--home',
    path.join(root, '.ossia'),
    '--port',
    '3212',
    '--engine-root',
    candidateRoots(root).roots[0].dir,
    '--docs-root',
    path.join(root, 'vendor/docs/lilypond/2.26.0'),
    '--static-dir',
    path.join(root, 'packages/ui/dist'),
    '--dev-origin',
    'http://127.0.0.1:5173',
    'serve',
  ],
  { cwd: root, stdio: 'inherit' },
);
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
child.on('error', (error) => {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
});
child.on('exit', (code) => {
  process.exitCode = code ?? 1;
});
