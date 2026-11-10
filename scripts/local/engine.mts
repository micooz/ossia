import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { checkImport } from '../../packages/backend/src/server/import-gate.ts';
import { readSvg } from '../../packages/ly/src/mod.ts';
import { fixtures, hash, json, position, readSource, root, scrub, write } from './common.mts';
import type { Fixture } from './fixtures.mts';

const engineRoot =
  process.env.OSSIA_LILYPOND_ROOT ?? path.join(root, 'vendor/lilypond/2.26.0-darwin-arm64');
const scratch = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-engine-')));
const out = path.join(fixtures, 'engine');
const evidence: Record<string, unknown>[] = [];
const failures: string[] = [];
const derived: { id: string; source: string; pages: string[]; midi?: string; log: string[] }[] = [];
const env = {
  ...process.env,
  HOME: path.join(scratch, 'home'),
  XDG_CACHE_HOME: path.join(scratch, 'cache'),
  GUILE_AUTO_COMPILE: '0',
  LC_ALL: 'C',
};
fs.mkdirSync(env.HOME, { recursive: true });

function run(id: string, tool: string, args: string[], cwd: string) {
  const r = spawnSync(path.join(engineRoot, 'bin', tool), args, {
    cwd,
    env,
    encoding: 'utf8',
    timeout: 120_000,
    maxBuffer: 16 << 20,
  });
  const log = scrub(`${r.stdout ?? ''}${r.stderr ?? ''}`, scratch).trim();
  evidence.push({
    id,
    tool,
    args,
    cwd: scrub(cwd, scratch),
    exitCode: r.status,
    signal: r.signal,
    error: r.error?.message ? scrub(r.error.message, scratch) : null,
    log,
  });
  return { ok: r.status === 0, log };
}
function requireResult(condition: unknown, message: string) {
  if (!condition) failures.push(message);
}
function persist(id: string, dir: string, source: string, log: string[]) {
  const files = fs.readdirSync(dir).sort();
  const pages = files
    .filter((f) => f.endsWith('.svg'))
    .map((f) => {
      const file = `engine/${id}/${f}`;
      write(path.join(fixtures, file), scrub(fs.readFileSync(path.join(dir, f), 'utf8'), scratch));
      return file;
    });
  const midiName = files.find((f) => /\.(midi|mid)$/.test(f));
  const midi = midiName ? `engine/${id}/${midiName}` : undefined;
  if (midi && midiName) write(path.join(fixtures, midi), fs.readFileSync(path.join(dir, midiName)));
  const sourceFile = `engine/${id}/score.ly`;
  write(path.join(fixtures, sourceFile), source);
  derived.push({ id, source: sourceFile, pages, midi, log });
  return {
    pages: pages.length,
    files: files
      .filter((f) => /\.(svg|pdf|midi|mid)$/.test(f))
      .map((file) => {
        const bytes = file.endsWith('.svg')
          ? fs.readFileSync(path.join(fixtures, `engine/${id}/${file}`))
          : fs.readFileSync(path.join(dir, file));
        return {
          file,
          bytes: bytes.length,
          sha256: hash(bytes),
          normalizedPaths: file.endsWith('.svg'),
        };
      }),
    pdfs: files
      .filter((f) => f.endsWith('.pdf'))
      .map((f) => ({ file: f, bytes: fs.statSync(path.join(dir, f)).size })),
    midi: midi ?? null,
  };
}
function compile(
  id: string,
  source: string,
  options: { expectedSuccess?: boolean; external?: boolean; expectMidi?: boolean } = {},
) {
  const dir = path.join(scratch, id);
  fs.mkdirSync(dir, { recursive: true });
  write(path.join(dir, 'score.ly'), source);
  if (options.external) write(path.join(dir, 'user-music.ily'), "UserMusic = { c'1 | }\n");
  const svg = run(
    id,
    'lilypond',
    ['-dpoint-and-click=#t', '--svg', '-o', 'score', 'score.ly'],
    dir,
  );
  const pdf = run(id, 'lilypond', ['-o', 'score', 'score.ly'], dir);
  requireResult(
    svg.ok === (options.expectedSuccess !== false) &&
      pdf.ok === (options.expectedSuccess !== false),
    `${id}: unexpected compiler status`,
  );
  if (svg.ok && pdf.ok) {
    const artifacts = persist(id, dir, source, [svg.log, pdf.log]);
    evidence.push({ id, artifacts });
    requireResult(artifacts.pages > 0 && artifacts.pdfs.length > 0, `${id}: missing SVG/PDF`);
    if (options.expectMidi !== false) requireResult(artifacts.midi, `${id}: missing MIDI`);
    return { dir, artifacts };
  }
  return { dir, artifacts: null };
}

try {
  const version = run('version', 'lilypond', ['--version'], scratch);
  assert.ok(
    version.ok && /GNU LilyPond 2\.26\.0\b/.test(version.log),
    'requires native LilyPond 2.26.0',
  );
  const manifest = JSON.parse(fs.readFileSync(path.join(fixtures, 'manifest.json'), 'utf8')) as {
    fixtures: Fixture[];
  };
  for (const f of manifest.fixtures.filter((f) => f.kind === 'ly')) {
    const source = readSource(path.join(fixtures, f.file)).text;
    const result = compile(f.id, source, {
      expectedSuccess: !['external-missing', 'raw-emoji'].includes(f.id),
      external: f.id === 'external-present',
      expectMidi: f.id !== 'chordmode',
    });
    if (f.id === 'polyphonic-pages')
      requireResult(result.artifacts?.pages === 2, 'polyphonic-pages: expected exactly two pages');
  }
  for (const f of manifest.fixtures.filter((f) => f.kind === 'xml' || f.id === 'midi-valid')) {
    const dir = path.join(scratch, f.id);
    fs.mkdirSync(dir, { recursive: true });
    const input = f.kind === 'xml' ? 'input.musicxml' : 'input.mid';
    fs.copyFileSync(path.join(fixtures, f.file), path.join(dir, input));
    const r = run(
      f.id,
      f.kind === 'xml' ? 'musicxml2ly' : 'midi2ly',
      ['-o', 'converted.ly', input],
      dir,
    );
    requireResult(
      r.ok && fs.existsSync(path.join(dir, 'converted.ly')),
      `${f.id}: converter failed`,
    );
    if (!r.ok || !fs.existsSync(path.join(dir, 'converted.ly'))) continue;
    const converted = scrub(fs.readFileSync(path.join(dir, 'converted.ly'), 'utf8'), scratch);
    const report = checkImport(converted, r.log.split('\n'));
    evidence.push({ id: f.id, importReport: report });
    if (f.kind === 'xml')
      requireResult(
        report.status === f.expected.import,
        `${f.id}: expected import ${String(f.expected.import)}, got ${report.status}`,
      );
    write(path.join(out, f.id, 'converted.ly'), converted);
    // The converter disables anchors and comments out MIDI. Changes apply only to this derived fixture.
    const renderSource = converted
      .replace('\\pointAndClickOff', '\\pointAndClickOn')
      .replace(/^(\s*)% (\\midi \{)/m, '$1$2');
    const rendered = compile(`${f.id}-render`, renderSource, {
      expectMidi: f.id !== 'xml-negative-skip',
    });
    if (rendered.artifacts) derived.find((d) => d.id === `${f.id}-render`)!.log = r.log.split('\n');
  }
  const coordinateSources = [
    { id: 'columns-lf', prefix: '', newline: '\n', whitespace: '\t' },
    { id: 'columns-plain', prefix: '', newline: '\n', whitespace: ' ' },
    { id: 'columns-bom-crlf', prefix: '\ufeff', newline: '\r\n', whitespace: '\t\t' },
  ];
  for (const c of coordinateSources) {
    const line = `Music = \\relative c' { \\mark \\markup { "中文😀" }${c.whitespace} c4 d4 e4 f4 | }`;
    const source = `${c.prefix}\\version "2.26.0"${c.newline}${line}${c.newline}\\score { \\new Staff \\Music \\layout {} \\midi {} }${c.newline}`;
    const rendered = compile(c.id, source);
    const offset = source.indexOf('c4');
    const expected = position(source, offset);
    const anchors = rendered.artifacts
      ? fs
          .readdirSync(rendered.dir)
          .filter((f) => f.endsWith('.svg'))
          .flatMap((f) =>
            readSvg(fs.readFileSync(path.join(rendered.dir, f), 'utf8')).flatMap((e) =>
              e.anchor ? [e.anchor] : [],
            ),
          )
      : [];
    const rawLinks = [...new Set(anchors.map((a) => scrub(a.href, scratch)))];
    const linePrefix = line.slice(0, line.indexOf('c4'));
    const displayColumnZero = Array.from(linePrefix).reduce(
      (n, ch) => (ch === '\t' ? n + 8 - (n % 8) : n + 1),
      0,
    );
    evidence.push({
      id: c.id,
      sourceHash: hash(Buffer.from(source)),
      note: {
        raw: 'c4',
        ...expected,
        utf16ColumnZero: linePrefix.length,
        unicodeColumnZero: Array.from(linePrefix).length,
        byteColumnZero: Buffer.byteLength(linePrefix),
        tabExpandedColumnZero: displayColumnZero,
      },
      rawLinks,
    });
    requireResult(
      anchors.some(
        (a) =>
          a.line === expected.line &&
          a.c1 === expected.column - 1 &&
          a.c2 === displayColumnZero + 1,
      ),
      `${c.id}: unexpected source anchor columns`,
    );
    const invalid = source.replace('c4', '\\unknownProbe c4');
    const dir = path.join(scratch, `${c.id}-diagnostic`);
    fs.mkdirSync(dir, { recursive: true });
    write(path.join(dir, 'score.ly'), invalid);
    const diag = run(`${c.id}-diagnostic`, 'lilypond', ['--svg', 'score.ly'], dir);
    requireResult(
      !diag.ok && /unknownProbe/.test(diag.log),
      `${c.id}: expected unknown command diagnostic`,
    );
    requireResult(
      diag.log.includes(`score.ly:2:${displayColumnZero + 1}: error: unknown command`),
      `${c.id}: unexpected diagnostic column`,
    );
    write(path.join(out, `${c.id}-diagnostic.ly`), invalid);
  }
  write(path.join(out, 'manifest.json'), json(derived));
} catch (error) {
  failures.push(scrub(error instanceof Error ? error.message : String(error), scratch));
  throw error;
} finally {
  write(
    path.join(fixtures, 'evidence/engine.json'),
    json({
      formatVersion: 1,
      platform: process.platform,
      architecture: process.arch,
      osRelease: os.release(),
      engineVersion: '2.26.0',
      engineLock: scrub(fs.readFileSync(path.join(root, 'vendor/ENGINE.lock'), 'utf8')),
      binaryArchitecture: spawnSync('file', [path.join(engineRoot, 'bin/lilypond')], {
        encoding: 'utf8',
      })
        .stdout.replaceAll(engineRoot, '<engine>')
        .trim(),
      sourceUnmodifiedForDirectCompiles: true,
      commands: evidence,
      failures,
    }),
  );
  fs.rmSync(scratch, { recursive: true, force: true });
}
assert.deepEqual(failures, [], failures.join('\n'));
console.log(`Native engine probe passed: ${derived.length} rendered synthetic inputs`);
