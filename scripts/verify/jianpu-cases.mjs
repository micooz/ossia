import { readFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { auditAudition } from './audition-audit.mjs';
import { auditPlayback } from './playback-audit.mjs';
import { fixtureRenderer, root } from './render-fixture.mjs';
import { auditSelection } from './selection-audit.mjs';
import { libraryCases } from './selection-cases.mjs';

const fixtures = path.join(root, 'tests/fixtures/jianpu');

function diagnostic({ format, error }, lineOffset) {
  const diagnostics = error.detail?.diagnostics ?? [];
  const nativeError = diagnostics.find(
    ({ position, severity }) => position && severity === 'error',
  );
  const location =
    error.detail?.position ??
    diagnostics.find(({ position, message }) => position && message.includes('Ossia Jianpu:'))
      ?.position ??
    nativeError?.position;
  return {
    format,
    code: error.code,
    message: error.message,
    ...(error.code === 'compile-failed' && nativeError && { cause: nativeError.message }),
    ...(location && { line: location.line - lineOffset, column: location.column }),
  };
}

function overlappingRhythms(layout, lineOffset) {
  const issues = [];
  for (const page of layout.pages) {
    const moments = new Map();
    for (const object of page.objects) {
      if (
        object.hitOnly ||
        !object.span ||
        object.staffIndex == null ||
        !['NoteHead', 'Rest'].includes(object.kind) ||
        !object.timing
      )
        continue;
      const { numerator, denominator } = object.timing;
      const key = `${object.staffIndex}:${numerator}/${denominator}`;
      const simultaneous = moments.get(key) ?? [];
      for (const other of simultaneous) {
        const x = Math.min(object.bbox.x1, other.bbox.x1) - Math.max(object.bbox.x0, other.bbox.x0);
        const y = Math.min(object.bbox.y1, other.bbox.y1) - Math.max(object.bbox.y0, other.bbox.y0);
        if (x > 0.15 && y > 0.15) {
          issues.push({
            kind: 'overlapping-rhythms',
            page: page.page,
            staffIndex: object.staffIndex,
            lines: [other, object].map(({ span }) => span.start.line - lineOffset),
          });
        }
      }
      simultaneous.push(object);
      moments.set(key, simultaneous);
    }
  }
  return issues;
}

function interactionBounds(geometry, playback) {
  const issues = [];
  const overlaps = (a0, a1, b0, b1) => Math.min(a1, b1) - Math.max(a0, b0) > 0.01;
  for (const page of geometry.pages) {
    const headers = page.elements
      .filter(({ region }) => region.elementKind?.startsWith('header-'))
      .map(({ bounds }) => bounds);
    for (const region of page.regions) {
      const { bounds, locator, systemIndex } = region;
      const overlapsY = (other) => overlaps(bounds.y0, bounds.y1, other.y0, other.y1);
      const overlapsX = (other) => overlaps(bounds.x0, bounds.x1, other.x0, other.x1);
      const location = { page: page.page, locator };
      if (headers.some((header) => overlapsY(header) && overlapsX(header))) {
        issues.push({ kind: 'measure-overlaps-header', ...location });
      }
      if (
        page.regions.some(
          (other) =>
            other.systemIndex === systemIndex &&
            other.locator.staffIndex < locator.staffIndex &&
            overlapsY(other.bounds) &&
            overlapsX(other.bounds),
        )
      ) {
        issues.push({ kind: 'overlapping-staff-selection', ...location });
      }
    }
    const spans = new Map();
    for (const span of playback.systems ?? []) {
      if (span.page === page.page && !spans.has(span.systemIndex)) {
        spans.set(span.systemIndex, span);
      }
    }
    for (const [systemIndex, span] of spans) {
      if (headers.some((header) => overlaps(span.y0, span.y1, header.y0, header.y1))) {
        issues.push({ kind: 'playhead-overlaps-header', page: page.page, systemIndex });
      }
    }
    for (const element of page.elements) {
      const { bounds, region, systemIndex } = element;
      if (!['NoteHead', 'Rest'].includes(region.elementKind)) continue;
      const x = (bounds.x0 + bounds.x1) / 2;
      const measure = page.regions.find(
        ({ locator, systemIndex: system, bounds: cell }) =>
          system === systemIndex &&
          locator.staffIndex === region.staffIndex &&
          x >= cell.x0 &&
          x <= cell.x1,
      );
      if (
        !measure ||
        bounds.y0 < measure.bounds.y0 - 0.01 ||
        bounds.y1 > measure.bounds.y1 + 0.01
      ) {
        issues.push({
          kind: 'rhythm-outside-measure-selection',
          page: page.page,
          systemIndex,
          label: region.label,
        });
      }
      const span = spans.get(systemIndex);
      if (
        region.elementKind === 'NoteHead' &&
        (!span || bounds.y0 < span.y0 || bounds.y1 > span.y1)
      ) {
        issues.push({
          kind: 'note-outside-playhead',
          page: page.page,
          systemIndex,
          label: region.label,
        });
      }
    }
  }
  return issues;
}

/** Check whether a score satisfies the Jianpu rendering and interaction contracts. */
export function auditJianpu(bundle, expected = {}) {
  if (bundle.compileError) {
    const failure = diagnostic({ format: 'source', error: bundle.compileError }, bundle.lineOffset);
    return {
      status: failure.code === 'unsupported-source' ? 'unsupported' : 'failed',
      diagnostics: [failure],
      issues: [{ kind: 'source-rejected' }],
    };
  }
  if (bundle.formatErrors?.length) {
    const diagnostics = bundle.formatErrors.map((failure) =>
      diagnostic(failure, bundle.lineOffset),
    );
    const visual = diagnostics.filter(({ format }) => format === 'svg' || format === 'pdf');
    const matches =
      expected.unsupported &&
      diagnostics.length === 2 &&
      visual.some(({ format }) => format === 'svg') &&
      visual.some(({ format }) => format === 'pdf') &&
      visual.every(
        ({ code, message, line }) =>
          code === 'unsupported-source' &&
          message.includes(expected.unsupported) &&
          (!expected.line || line === expected.line),
      );
    if (matches && bundle.pageCount === 0 && !bundle.pdf)
      return { status: 'expected-unsupported', diagnostics, issues: [] };
    return {
      status: visual.some(({ code }) => code === 'unsupported-source') ? 'unsupported' : 'failed',
      diagnostics,
      issues: [{ kind: expected.unsupported ? 'unexpected-diagnostic' : 'render-failed' }],
    };
  }
  if (expected.unsupported)
    return { status: 'failed', issues: [{ kind: 'expected-unsupported-now-renders' }] };

  const { jianpu, layout, playback, geometry } = bundle;
  const issues = [];
  if (!jianpu.pdf) issues.push({ kind: 'missing-pdf' });
  if (!jianpu.sameMidi) issues.push({ kind: 'different-midi', diagnostics: jianpu.baselineErrors });
  if (expected.compareOriginal && !jianpu.samePlayback)
    issues.push({ kind: 'different-browser-playback', diagnostics: jianpu.baselineErrors });
  if (expected.compareOriginal && !jianpu.samePrintedRhythm)
    issues.push({ kind: 'different-printed-rhythm', diagnostics: jianpu.baselineErrors });
  if (layout.capability !== 'measure' || layout.reason)
    issues.push({ kind: 'incomplete-measure-mapping', reason: layout.reason });
  const selection = auditSelection(bundle);
  issues.push(...selection.issues);
  const measures = expected.measures ?? jianpu.sourceMeasureCount;
  const staves = expected.staves ?? bundle.selection.staves.length;
  if (measures && selection.cells !== measures * staves)
    issues.push({
      kind: 'unexpected-measure-count',
      expected: measures * staves,
      actual: selection.cells,
    });
  if (expected.measures) {
    for (let staffIndex = 0; staffIndex < expected.staves; staffIndex++) {
      const locators = selection.clicks
        .filter((click) => click.staffIndex === staffIndex)
        .map(({ target }) => target.locator);
      if (
        locators.length !== expected.measures ||
        locators.some(
          ({ measureStart, measureEnd }, index) =>
            measureStart !== index + 1 || measureEnd !== index + 1,
        )
      )
        issues.push({ kind: 'wrong-engraved-measure-order', staffIndex });
    }
  }
  const timing = auditPlayback(playback, geometry);
  issues.push(...timing.issues);
  if (timing.untimedGroups) issues.push({ kind: 'untimed-groups', count: timing.untimedGroups });
  const audition = auditAudition(playback, geometry);
  if (!audition.noteheads) issues.push({ kind: 'empty-numbered-score' });
  issues.push(
    ...audition.issues,
    ...overlappingRhythms(layout, jianpu.lineOffset),
    ...interactionBounds(geometry, playback),
  );
  return {
    status: issues.length ? 'failed' : 'passed',
    pages: layout.pages.length,
    measureCells: selection.cells,
    noteheads: audition.noteheads,
    issues,
  };
}

/** Verify named regression cases or scan current library sources and local files. */
export async function runJianpuCases(args = [], renderFixture) {
  const libraryMode = args.includes('--library');
  const compareOriginal = args.includes('--compare-original');
  const baseArg = args.find((arg) => arg.startsWith('--base='));
  const operands = args.filter(
    (arg) => arg !== '--library' && arg !== '--compare-original' && arg !== baseArg,
  );
  const files = libraryMode ? operands.filter((arg) => arg.toLowerCase().endsWith('.ly')) : [];
  const ids = operands.filter((arg) => !files.includes(arg));
  let selected;
  if (libraryMode) {
    const library = files.length && !ids.length ? [] : await libraryCases(ids, baseArg);
    selected = library.concat(files.map((file) => ({ id: file, source: path.resolve(file) })));
  } else {
    const cases = JSON.parse(readFileSync(path.join(fixtures, 'cases.json'), 'utf8'));
    if (ids.some((id) => !cases.some((entry) => entry.id === id)))
      throw new Error('select known Jianpu fixture IDs');
    selected = cases
      .filter((entry) => !ids.length || ids.includes(entry.id))
      .map((entry) => ({ ...entry, source: path.resolve(fixtures, entry.source) }));
  }
  if (!selected.length) throw new Error('no Jianpu sources selected');

  const render = renderFixture ?? fixtureRenderer('jianpu');
  const totals = { passed: 0, 'expected-unsupported': 0, unsupported: 0, failed: 0 };
  for (const entry of selected) {
    const expect = compareOriginal ? { ...entry.expect, compareOriginal } : entry.expect;
    let report;
    try {
      report = auditJianpu(render({ ...entry, expect }), expect);
    } catch (error) {
      report = {
        status: 'failed',
        issues: [{ kind: 'audit-error', message: error.stderr?.trim() || String(error) }],
      };
    }
    totals[report.status]++;
    console.log(JSON.stringify({ id: entry.id, title: entry.title, ...report }));
  }
  console.log(JSON.stringify({ total: selected.length, ...totals }));
  return Number(totals.unsupported > 0 || totals.failed > 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href)
  process.exitCode = await runJianpuCases(process.argv.slice(2));
