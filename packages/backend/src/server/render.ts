// Service B — rendering: one LilyPond child process per task (ADR-0001 §5).
//
// Isolation is a subprocess with its own cwd, timeout and HOME, so an engine crash or hang costs
// one task instead of the service. Outputs are addressed by the content hash they came from, so a
// re-render of the same `.ly` is a cache hit — for SVG and MIDI, which are byte-deterministic.
// PDF is the exception: Ghostscript stamps a creation date and a document UUID, so each render of
// one immutable `.ly` hashes differently (see the gap noted in .docs).
//
// Two engine constraints shape every pass below: `--svg` emits no PDF, so PDF/MIDI is a second run;
// and there is no `--midi` option — MIDI exists only when the source has an active `\midi` block,
// so the PDF/MIDI pass runs over a derived text that enables musicxml2ly's commented block.
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { engine, engineEnv, type EngineInfo } from './engine.ts';
import { putAsset, type AssetRef } from './store.ts';

const run = promisify(execFile);

export class RenderError extends Error {
  constructor(
    message: string,
    readonly log: string[],
    readonly timedOut = false,
  ) {
    super(message);
  }
}

/**
 * Replace absolute paths with the name the client already knows. LilyPond prefixes every diagnostic
 * with the file it compiled, and that file lives in a scratch directory: handing it out would expose
 * the server's layout (G6), so no log line ever leaves with a real path in it.
 */
// A real path is at least three segments and starts at a word boundary (`/var/folders/xr/…`); a
// duration like `81/4` must survive untouched, hence the lookbehind and the `{2,}`.
const ABS_PATH = new RegExp(String.raw`(?<=^|[\s"'"'(=:])/(?:[A-Za-z0-9._-]+/){2,}[A-Za-z0-9._-]+`);
export const redactPaths = (text: string): string =>
  text.replace(new RegExp(ABS_PATH.source, 'g'), (m) => m.slice(m.lastIndexOf('/') + 1));

/** Filter LilyPond's stderr to what the product should surface. */
export const filterLog = (stderr: string): string[] =>
  stderr
    .split('\n')
    // Fontconfig warnings are a host-environment artifact, not a defect in the score
    // (alternatives-evaluation §3.16: filter them, never treat one as a compile error).
    .filter((l) => l.trim().length > 0 && !/Fontconfig|^\t|Cairo-|GL-|pango/i.test(l))
    .filter((l) => /error|warning|unexpected|not a note name|syntax|no such/i.test(l))
    .map((l) => redactPaths(l))
    .slice(0, 40);

const pageOf = (f: string): number | null => {
  const m = /-(\d+)\.[a-z]+$/i.exec(f);
  return m ? +m[1]! : null;
};
const byPage = (a: string, b: string): number =>
  (pageOf(a) ?? 0) - (pageOf(b) ?? 0) || a.localeCompare(b);

interface Exec {
  stderr: string;
  dir: string;
  clean: () => void;
}

async function exec(args: string[], source: string, tag: string, timeoutMs: number): Promise<Exec> {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `ossia-${tag}-`));
  const src = path.join(dir, 'score.ly');
  fs.writeFileSync(src, source);
  let stderr = '';
  try {
    const r = await run(engine().bin, [...args, '-o', path.join(dir, 'score'), src], {
      cwd: dir,
      timeout: timeoutMs,
      maxBuffer: 1 << 26,
      env: engineEnv(dir),
    });
    stderr = `${r.stderr ?? ''}\n${r.stdout ?? ''}`;
  } catch (err) {
    const e = err as { killed?: boolean; code?: number | null; stderr?: string; stdout?: string };
    stderr = `${e.stderr ?? ''}\n${e.stdout ?? ''}`;
    const log = filterLog(stderr);
    const timedOut = !!e.killed || e.code === null;
    throw new RenderError(
      timedOut ? 'render timed out' : 'compile failed',
      log.length ? log : [stderr.split('\n').find((l) => l.trim()) || 'compile failed'],
      timedOut,
    );
  }
  return { stderr, dir, clean: () => fs.rmSync(dir, { recursive: true, force: true }) };
}

/**
 * Collect engine output files of one extension into the asset store, ordered by page.
 *
 * SVG pages are sanitized *before* they are stored, not only on the way out: LilyPond's
 * point-and-click hrefs carry the scratch directory of this particular run, so storing them raw
 * would give the same `.ly` a different asset hash on every render and defeat the cache. The
 * anchor's line/column tail survives, which is all L2/L3 need.
 */
function collect(dir: string, kind: 'svg' | 'pdf' | 'midi'): AssetRef[] {
  const ext = kind === 'midi' ? /\.midi?$/i : new RegExp(`\\.${kind}$`, 'i');
  const files = fs
    .readdirSync(dir)
    .filter((f) => ext.test(f))
    .sort(byPage);
  return files.map((f) => {
    const buf = fs.readFileSync(path.join(dir, f));
    return putAsset(
      kind,
      kind === 'svg' ? Buffer.from(sanitizeSvg(buf.toString('utf8')), 'utf8') : buf,
      pageOf(f),
    );
  });
}

/**
 * `textedit:///var/folders/…/score.ly:104:51:52` -> `textedit://score.ly:104:51:52`.
 * Strips the server-side scratch path from every anchor (G6: no server paths leave the service)
 * and makes the stored bytes depend only on the score.
 */
export const sanitizeSvg = (svg: string): string =>
  svg.replace(/textedit:\/\/[^"'\s<]*?([^/"'\s]+\.ly):/g, 'textedit://$1:');

export interface RenderSet {
  pages: AssetRef[];
  pdf: AssetRef | null;
  midi: AssetRef | null;
  ms: number;
  log: string[];
}

/**
 * Full render of one version: the SVG pass for display + locating, then the PDF/MIDI pass for
 * export and playback. Both are needed because LilyPond emits PDF and SVG in separate runs.
 */
export async function renderSet(ly: string, opts: { timeoutMs?: number } = {}): Promise<RenderSet> {
  const t0 = performance.now();
  const svg = await exec(['--svg'], withPointAndClick(ly), 'svg', opts.timeoutMs ?? 180_000);
  const pages = collect(svg.dir, 'svg');
  svg.clean();
  if (!pages.length) throw new RenderError('engine produced no SVG', filterLog(svg.stderr));
  const out = await exec([], withMidi(ly), 'pdf', opts.timeoutMs ?? 120_000);
  const pdf = collect(out.dir, 'pdf')[0] ?? null;
  const midi = collect(out.dir, 'midi')[0] ?? null;
  out.clean();
  return {
    pages,
    pdf,
    midi,
    ms: performance.now() - t0,
    log: filterLog(`${svg.stderr}\n${out.stderr}`),
  };
}

/** MIDI-only pass for gates G-c/G-d: engraving skipped, ~0.5 s on a 32-measure score. */
export async function renderMidi(
  ly: string,
): Promise<{ tracks: import('@ossia/ly').SmfTrack[] } | { error: string }> {
  const { parseMidi } = await import('@ossia/ly');
  let job: Exec;
  try {
    job = await exec([], midiOnlyText(ly), 'midi', 120_000);
  } catch (e) {
    if (e instanceof RenderError) return { error: e.log[0] ?? e.message };
    throw e;
  }
  try {
    const midi = collect(job.dir, 'midi')[0];
    if (!midi) return { error: 'no midi output' };
    return { tracks: parseMidi(fs.readFileSync(midi.file)) };
  } finally {
    job.clean();
  }
}

/**
 * Enable MIDI in a derived text without touching layout: uncomment musicxml2ly's
 * `% \midi { … }`, and round the fractional tempo LilyPond rejects as "unexpected REAL".
 */
export function withMidi(text: string): string {
  if (hasActiveMidi(text)) return text;
  let out = text.replace(/^(\s*)% (\\midi \{)/m, '$1$2');
  out = out.replace(
    /(\\tempo\s+\w+\s*=\s*)(\d+\.\d+)/,
    (_m, k: string, v: string) => k + Math.round(+v),
  );
  if (out === text) {
    // No commented block to enable (hand-written or midi2ly source): add one before the final `}`.
    const i = text.lastIndexOf('\n}');
    if (i < 0) throw new Error('no \\midi block and no score closing brace to add one');
    return `${text.slice(0, i)}\n  \\midi {}\n${text.slice(i)}`;
  }
  return out;
}

/** Drop the score-level `\layout {}` so the engine skips engraving (byte-identical MIDI). */
export const withoutLayout = (t: string): string =>
  t.replace(/^[ \t]*\\layout \{\}[ \t]*\r?\n/m, '');

export const midiOnlyText = (ly: string): string => withoutLayout(withMidi(ly));

const hasActiveMidi = (text: string): boolean =>
  text.split('\n').some((l) => !/^\s*%/.test(l) && /\\midi\b/.test(l));

/**
 * ADR-0001 §6.3 renders with `\pointAndClickOn` so L3 anchors exist. Applied to the render text
 * only — never to the stored source — and always in a line-count-preserving way, so the anchor
 * line numbers inside the SVG still address the stored `.ly`.
 */
export function withPointAndClick(ly: string): string {
  if (/\\pointAndClickOn\b/.test(ly)) return ly;
  // Every backslash in a replacement string is doubled: in a single-quoted `'$1$2 \pointAndClickOn$3'`
  // JS consumes `\p`, emitting `\version "2.14.0" pointAndClickOn` — a syntax error the engine only
  // reports for midi2ly sources, since musicxml2ly output escapes the trap by carrying
  // `\pointAndClickOff` and taking the branch above.
  if (/\\pointAndClickOff\b/.test(ly)) return ly.replace('\\pointAndClickOff', '\\pointAndClickOn');
  if (/^\s*\\version\s+"[^"]*"/m.test(ly))
    return ly.replace(/^(\s*)(\\version\s+"[^"]*")(\s*)$/m, '$1$2 \\pointAndClickOn$3');
  return `\\pointAndClickOn
${ly}`;
}

/** Import a MusicXML baseline through `musicxml2ly` (ADR-0001 D3: one-time conversion). */
export async function musicxmlToLy(
  xml: Buffer,
  name = 'input.xml',
): Promise<{ ly: string; log: string[] }> {
  const e = engine();
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-imp-'));
  const src = path.join(dir, name);
  fs.writeFileSync(src, xml);
  try {
    const r = await run(
      path.join(e.root, 'bin', 'musicxml2ly'),
      ['--no-stem-directions', '-o', path.join(dir, 'out.ly'), src],
      { cwd: dir, timeout: 180_000, maxBuffer: 1 << 26, env: engineEnv(dir) },
    );
    const log = filterLog(`${r.stderr ?? ''}\n${r.stdout ?? ''}`);
    return { ly: readProduced(dir), log };
  } catch (err) {
    const e2 = err as { stderr?: string; stdout?: string };
    throw new RenderError(
      'musicxml2ly failed',
      filterLog(`${e2.stderr ?? ''}\n${e2.stdout ?? ''}`),
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// musicxml2ly ignores `-o` when it derives a name from the input, so take the first `.ly` it wrote.
function readProduced(dir: string): string {
  const produced = fs
    .readdirSync(dir)
    .filter((f) => f.endsWith('.ly'))
    .sort();
  if (!produced.length) throw new RenderError('musicxml2ly produced no source', []);
  const ly = fs.readFileSync(path.join(dir, produced[0]!), 'utf8');
  if (!ly.trim()) throw new RenderError('musicxml2ly produced an empty source', []);
  // The tool records the absolute temp path of the input in a comment; that path does not exist
  // later and would make the stored hash depend on the scratch directory name.
  return ly.replace(/^(% automatically converted by musicxml2ly from )\S+$/m, '$1<imported file>');
}

/** midi2ly for MIDI imports (chain 1 accepts MusicXML or MIDI). */
export async function midiToLy(midi: Buffer): Promise<{ ly: string; log: string[] }> {
  const e = engine();
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-midi2ly-'));
  const src = path.join(dir, 'input.midi');
  fs.writeFileSync(src, midi);
  try {
    const r = await run(
      path.join(e.root, 'bin', 'midi2ly'),
      ['-o', path.join(dir, 'out.ly'), src],
      { cwd: dir, timeout: 180_000, maxBuffer: 1 << 26, env: engineEnv(dir) },
    );
    const produced = fs
      .readdirSync(dir)
      .filter((f) => f.endsWith('.ly'))
      .sort();
    if (!produced.length)
      throw new RenderError('midi2ly produced no source', filterLog(`${r.stderr}`));
    return {
      ly: fs.readFileSync(path.join(dir, produced[0]!), 'utf8'),
      log: filterLog(`${r.stderr ?? ''}`),
    };
  } catch (err) {
    const e2 = err as { stderr?: string };
    throw new RenderError('midi2ly failed', filterLog(e2.stderr ?? ''));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

export { engine };
export type { EngineInfo };
