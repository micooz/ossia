import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { auditionAttacks, auditAudition } from './audition-audit.mjs';
import { auditPlayback } from './playback-audit.mjs';
import { fixtureRenderer, fixtures, root } from './render-fixture.mjs';

/** Audit the selected scores and run the browser checks if all data checks pass. */
export function runAuditionCases(ids, renderFixture) {
  const cases = JSON.parse(readFileSync(path.join(fixtures, 'cases.json'), 'utf8'));
  if (ids.some((id) => !cases.some((entry) => entry.id === id))) throw new Error('unknown case');
  const selected = cases.filter((entry) => !ids.length || ids.includes(entry.id));
  if (!selected.length) throw new Error('no audition cases selected');
  const render = renderFixture ?? fixtureRenderer();
  const directory = mkdtempSync(path.join(os.tmpdir(), 'ossia-audition-'));
  let failed = false;
  try {
    const bundles = selected.map((entry) => {
      const bundle = render(entry);
      const report = auditAudition(bundle.playback, bundle.geometry);
      console.log(JSON.stringify({ id: entry.id, ...report }));
      if (report.issues.length) failed = true;
      const playback = auditPlayback(bundle.playback, bundle.geometry);
      if (playback.issues.length || playback.untimedGroups) {
        console.error(
          JSON.stringify({
            id: entry.id,
            issues: playback.issues,
            untimedGroups: playback.untimedGroups,
          }),
        );
        failed = true;
      }
      return {
        id: entry.id,
        ...bundle,
        auditionAttacks: auditionAttacks(bundle.playback),
        trace: playback.trace.map((frame) => ({ time: frame.time, ...frame.position })),
      };
    });
    const input = path.join(directory, 'fixtures.json');
    const outputStart = performance.now();
    const output = JSON.stringify(bundles);
    writeFileSync(input, output);
    if (!failed && process.env.OSSIA_AUDITION_OUTPUT) {
      writeFileSync(process.env.OSSIA_AUDITION_OUTPUT, output);
    }
    console.log(
      `Audition fixture serialization: ${((performance.now() - outputStart) / 1000).toFixed(2)}s`,
    );
    if (!failed && !process.env.OSSIA_AUDITION_OUTPUT)
      execFileSync(
        process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm',
        [
          '--filter',
          '@ossia/ui',
          'exec',
          'playwright',
          'test',
          '--project=browser',
          '--reporter=list',
          'tests/score/audition-fixture.browser.spec.ts',
        ],
        {
          cwd: root,
          stdio: 'inherit',
          shell: process.platform === 'win32',
          env: { ...process.env, OSSIA_AUDITION_FIXTURES: input },
        },
      );
    return Number(failed);
  } finally {
    rmSync(directory, { recursive: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href)
  process.exitCode = runAuditionCases(process.argv.slice(2));
