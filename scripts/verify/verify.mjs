import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { runAuditionCases } from './audition-cases.mjs';
import { runJianpuCases } from './jianpu-cases.mjs';
import { runPlaybackCases } from './playback-cases.mjs';
import { fixtureRenderer, fixtures } from './render-fixture.mjs';
import { runSelectionCases } from './selection-cases.mjs';

/** Run the full verification suite or select playback cases by ID. */
export async function runVerification(ids = []) {
  if (!ids.length) {
    console.log('Verifying Jianpu fixtures');
    const status = await runJianpuCases();
    if (status) return status;
  }
  const render = fixtureRenderer('all');
  const renders = new Map();
  const renderOnce = (entry) => {
    const filename = path.resolve(fixtures, entry.source);
    if (!renders.has(filename)) renders.set(filename, render(entry));
    return renders.get(filename);
  };

  for (const { name, run } of [
    { name: 'playback', run: runPlaybackCases },
    { name: 'audition', run: runAuditionCases },
    { name: 'selection', run: runSelectionCases },
  ]) {
    console.log(`Verifying ${name} fixtures`);
    const status = await run(ids, renderOnce);
    if (status) return status;
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href)
  process.exitCode = await runVerification(process.argv.slice(2));
