import { readFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { fixtureRenderer, fixtures, root } from './render-fixture.mjs';
import { auditSelection } from './selection-audit.mjs';

/** Verify committed cases or audit the selected library scores. */
export async function runSelectionCases(args, renderFixture) {
  const libraryMode = args.includes('--library');
  const baseArg = args.find((arg) => arg.startsWith('--base='));
  const ids = args.filter((arg) => arg !== '--library' && arg !== baseArg);
  const selected = libraryMode ? await libraryCases(ids, baseArg) : fixtureCases(ids);
  if (!selected.length)
    throw new Error(libraryMode ? 'the project library is empty' : 'select known fixture IDs');

  const render = renderFixture ?? fixtureRenderer('selection');
  let failed = false;
  for (const entry of selected) {
    try {
      const { cells, clicks, issues } = auditSelection(render(entry));
      const expected = entry.selection;
      if (expected?.measures && cells !== expected.measures * expected.staves)
        issues.push({ kind: 'unexpected-measure-count', expected });
      if (expected?.measures && clicks.length === cells) {
        for (let staffIndex = 0; staffIndex < expected.staves; staffIndex++) {
          const staff = clicks.filter((click) => click.staffIndex === staffIndex);
          if (
            staff.length !== expected.measures ||
            staff.some(
              ({ target: { locator } }, i) =>
                locator.measureStart !== i + 1 || locator.measureEnd !== i + 1,
            )
          )
            issues.push({ kind: 'wrong-engraved-measure-order', staffIndex });
        }
      }
      console.log(
        JSON.stringify({
          id: entry.id,
          title: entry.title,
          cells,
          clickable: clicks.length,
          issues,
        }),
      );
      if (issues.length) failed = true;
    } catch (error) {
      console.error(JSON.stringify({ id: entry.id, error: String(error) }));
      failed = true;
    }
  }
  return Number(failed);
}

/** Resolve current source paths in the development library. */
export async function libraryCases(ids, baseArg) {
  const library = path.join(root, '.ossia');
  const documents = ids.length ? ids.map((id) => ({ id })) : await listLibrary(library, baseArg);
  return documents.map(({ id, title }) => {
    if (!/^[\da-f]{8}(?:-[\da-f]{4}){3}-[\da-f]{12}$/i.test(id))
      throw new Error(`invalid document ID: ${id}`);
    return { id, title, source: path.join(library, 'documents', id.toLowerCase(), 'score.ly') };
  });
}

function fixtureCases(ids) {
  const cases = JSON.parse(readFileSync(path.join(fixtures, 'cases.json'), 'utf8'));
  if (ids.some((id) => !cases.some((entry) => entry.id === id)))
    throw new Error('select known fixture IDs');
  return cases.filter((entry) => !ids.length || ids.includes(entry.id));
}

async function listLibrary(library, baseArg) {
  const base = new URL(baseArg?.slice('--base='.length) ?? 'http://127.0.0.1:3211/api/v1/');
  if (!base.pathname.endsWith('/')) base.pathname += '/';
  const get = async (resource) => {
    const response = await fetch(new URL(resource, base));
    if (!response.ok) throw new Error(`could not read ${resource}: HTTP ${response.status}`);
    return response.json();
  };
  const { runtime } = await get('status');
  if (!runtime?.home || path.resolve(runtime.home) !== library)
    throw new Error(`the server must be using ${library}`);
  const documents = [];
  let offset = 0;
  do {
    const page = await get(`documents?limit=200&offset=${offset}`);
    documents.push(...page.items.map(({ documentId: id, title }) => ({ id, title })));
    offset = page.nextOffset;
  } while (offset !== null);
  return documents;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href)
  process.exitCode = await runSelectionCases(process.argv.slice(2));
