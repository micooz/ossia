import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { candidateRoots } from '../engine/engine.mjs';

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const fixtures = path.join(root, 'tests/fixtures/playback');

/** Build the fixture executable and prepare a renderer for playback, selection, or both. */
export function fixtureRenderer(mode = 'playback') {
  const buildStart = performance.now();
  try {
    execFileSync('cargo', ['build', '--quiet', '-p', 'ossia-engine', '--bin', 'playback-fixture'], {
      cwd: root,
      stdio: 'inherit',
    });
  } finally {
    console.log(
      `Fixture compilation (${mode}): ${((performance.now() - buildStart) / 1000).toFixed(2)}s`,
    );
  }
  const exe = path.join(
    path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target'),
    'debug',
    process.platform === 'win32' ? 'playback-fixture.exe' : 'playback-fixture',
  );
  const engine = process.env.OSSIA_TEST_LILYPOND_ROOT ?? candidateRoots(root).roots[0].dir;
  const modeArgs = mode === 'playback' ? [] : [`--${mode}`];
  return (entry) => {
    const filename = path.resolve(fixtures, entry.source);
    const source = readFileSync(filename, 'utf8');
    const referenceArgs =
      mode === 'jianpu' && entry.expect?.compareOriginal ? ['--compare-original'] : [];
    const renderStart = performance.now();
    try {
      const raw = execFileSync(exe, [engine, filename, ...modeArgs, ...referenceArgs], {
        cwd: root,
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
      });
      return { ...JSON.parse(raw), source };
    } finally {
      console.log(
        `Fixture render (${mode}, ${entry.id ?? entry.source}): ${((performance.now() - renderStart) / 1000).toFixed(2)}s`,
      );
    }
  };
}
