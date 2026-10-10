import { execFileSync } from 'node:child_process';
// Tauri's `beforeDevCommand`: make the SPA assets exist, then serve them with HMR.
//
// Both halves are needed. tauri-codegen embeds `frontendDist` at compile time, so a fresh clone with
// no packages/ui/dist fails to build the shell even in dev; and the dev server has to stay in the
// foreground so `tauri dev` waits on devUrl. Vite resolves its root from the process cwd, which is
// why this spawns with cwd = packages/ui instead of duplicating port/host settings here.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const UI = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../ui');
const vite = path.join(UI, 'node_modules/.bin/vite');
const args = process.argv.slice(2);

if (args[0] === 'build') {
  execFileSync(vite, ['build'], { cwd: UI, stdio: 'inherit' });
} else {
  if (!fs.existsSync(path.join(UI, 'dist/index.html'))) {
    process.stderr.write('SPA dist missing -> building it once for tauri-codegen\n');
    execFileSync(vite, ['build'], { cwd: UI, stdio: 'inherit' });
  }
  execFileSync(vite, [], { cwd: UI, stdio: 'inherit' });
}
