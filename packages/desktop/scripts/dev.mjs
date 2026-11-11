import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { candidateRoots } from '../../../scripts/engine.mjs';

const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const root = path.resolve(desktop, '../..');
const windows = process.platform === 'win32';
const child = spawn(
  path.join(desktop, 'node_modules/.bin', windows ? 'tauri.cmd' : 'tauri'),
  [
    'dev',
    '--config',
    'src-tauri/tauri.dev.conf.json',
    '--',
    '--',
    '--home',
    path.join(root, '.ossia'),
    '--port',
    '3212',
    '--engine-root',
    candidateRoots(root).roots[0].dir,
    '--static-dir',
    path.join(root, 'packages/ui/dist'),
    '--dev-origin',
    'http://127.0.0.1:5173',
    ...process.argv.slice(2),
  ],
  { cwd: desktop, stdio: 'inherit', shell: windows },
);
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
child.on('error', (error) => {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
});
child.on('exit', (code) => {
  process.exitCode = code ?? 1;
});
