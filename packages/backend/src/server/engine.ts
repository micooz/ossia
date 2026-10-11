// Service B/E substrate: locate and self-heal the vendored LilyPond engine.
//
// Precedence: OSSIA_LILYPOND_ROOT -> vendor/lilypond/<tag>
// -> ~/Library/Application Support/Ossia/engine/<tag>. The vendor tree is gitignored,
// so a fresh clone resolves "not installed" and the first `engine:ensure` restores it from
// vendor/ENGINE.lock. `verify` mirrors scripts/engine.mjs so both entry points agree.
import { execFile, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

const run = promisify(execFile);

export interface EngineLock {
  tag: string;
  version: string;
  url: string;
  file: string;
  sha256: string;
}

// `next dev` / `next start` run with cwd = packages/backend, so the repo root is two levels up.
// Overridable for tests and for any deployment whose cwd is not the repo tree.
export const repoRoot = (): string =>
  process.env.OSSIA_REPO_ROOT || path.resolve(process.cwd(), '../..');

export function readLock(root = repoRoot()): EngineLock {
  const p = path.join(root, 'vendor', 'ENGINE.lock');
  if (!fs.existsSync(p)) throw new Error(`missing engine lock: ${p}`);
  const out: Record<string, string> = {};
  for (const line of fs.readFileSync(p, 'utf8').split('\n')) {
    const m = /^\s*([A-Za-z0-9_]+)\s*=\s*"([^"]*)"\s*$/.exec(line);
    if (m) out[m[1]!] = m[2]!;
  }
  for (const k of ['tag', 'version', 'url', 'file', 'sha256']) {
    if (!out[k]) throw new Error(`engine lock missing field: ${k}`);
  }
  return out as unknown as EngineLock;
}

export function candidateRoots(
  env: NodeJS.ProcessEnv = process.env,
): { src: string; dir: string }[] {
  const lock = readLock();
  const list: { src: string; dir: string }[] = [];
  if (env.OSSIA_LILYPOND_ROOT) list.push({ src: 'env', dir: env.OSSIA_LILYPOND_ROOT });
  list.push({ src: 'vendor', dir: path.join(repoRoot(), 'vendor', 'lilypond', lock.tag) });
  list.push({
    src: 'appdata',
    dir: path.join(os.homedir(), 'Library', 'Application Support', 'Ossia', 'engine', lock.tag),
  });
  return list;
}

export const lilypondBin = (dir: string) => path.join(dir, 'bin', 'lilypond');
export const musicxml2lyBin = (dir: string) => path.join(dir, 'bin', 'musicxml2ly');

function* walk(dir: string, ext: string): Generator<string> {
  const stack = [dir];
  while (stack.length) {
    const d = stack.pop()!;
    let ents: fs.Dirent[];
    try {
      ents = fs.readdirSync(d, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const ent of ents) {
      const p = path.join(d, ent.name);
      if (ent.isDirectory()) stack.push(p);
      else if (ent.isFile() && ent.name.endsWith(ext)) yield p;
    }
  }
}

// Structural + functional check. The mtime rule (R: compiled Guile modules must not be older
// than their sources) is what turns a 0.3 s render into a 165 s one when broken.
export function verifyTree(dir: string, lock: EngineLock = readLock()): string[] {
  const problems: string[] = [];
  if (!fs.existsSync(dir)) return [`not extracted: ${dir}`];
  const bin = lilypondBin(dir);
  if (!fs.existsSync(bin)) problems.push(`missing ${bin}`);
  if (!fs.existsSync(path.join(dir, 'share', 'lilypond', lock.version, 'scm'))) {
    problems.push(`missing scm tree for ${lock.version}`);
  }
  if (problems.length) return problems;
  const mt = mtimes(dir);
  if (mt.scmMax > mt.goMax) problems.push('compiled modules older than sources');
  try {
    const out = execFileSync(bin, ['--version'], { encoding: 'utf8', timeout: 30000 });
    if (!out.includes(lock.version)) problems.push(`lilypond --version: ${out.split('\n')[0]}`);
  } catch (e) {
    problems.push(`lilypond --version failed: ${String((e as Error).message).slice(0, 160)}`);
  }
  return problems;
}

/** Newest mtime among the Guile sources and their compiled modules. */
function mtimes(dir: string) {
  let goMax = 0,
    scmMax = 0;
  for (const f of walk(dir, '.go')) goMax = Math.max(goMax, fs.statSync(f).mtimeMs);
  for (const f of walk(dir, '.scm')) scmMax = Math.max(scmMax, fs.statSync(f).mtimeMs);
  return { goMax, scmMax };
}

export interface EngineInfo {
  root: string;
  src: string;
  bin: string;
  version: string;
}

let cached: EngineInfo | null = null;

// Resolve the engine, repairing the mtime rule in place when that is the only problem.
export function engine(): EngineInfo {
  if (cached) return cached;
  const lock = readLock();
  const candidates = candidateRoots();
  const tried: string[] = [];
  for (const c of candidates) {
    if (!fs.existsSync(c.dir)) {
      tried.push(`${c.src}: absent`);
      continue;
    }
    const problems = verifyTree(c.dir, lock);
    if (!problems.length) {
      cached = { root: c.dir, src: c.src, bin: lilypondBin(c.dir), version: lock.version };
      return cached;
    }
    // Self-heal: the mtime rule is the only failure we can fix without re-extracting.
    if (problems.length === 1 && /older than sources/.test(problems[0]!)) {
      healMtimes(c.dir);
      if (!verifyTree(c.dir, lock).length) {
        cached = { root: c.dir, src: c.src, bin: lilypondBin(c.dir), version: lock.version };
        return cached;
      }
      tried.push(`${c.src}: mtime healed but still broken`);
      continue;
    }
    tried.push(`${c.src}: ${problems.join('; ')}`);
  }
  throw new Error(
    `LilyPond engine unusable (looked in ${candidates.map((c) => c.dir).join(', ')}).\n  ${tried.join('\n  ')}\n  fix: pnpm engine:install`,
  );
}

/** Set every compiled module to just after the newest source module (fs, not `touch`, so a
 *  ~400 file list cannot blow the argv limit). */
export function healMtimes(dir: string): number {
  const { scmMax } = mtimes(dir);
  const stamp = new Date(Math.max(Date.now(), scmMax) + 60_000);
  let n = 0;
  for (const f of walk(dir, '.go')) {
    fs.utimesSync(f, stamp, stamp);
    n++;
  }
  return n;
}

// LilyPond writes fontconfig caches under $HOME; pointing it at a private, writable scratch
// directory keeps those warnings out of render logs and makes renders reproducible.
export function engineEnv(scratch: string): NodeJS.ProcessEnv {
  fs.mkdirSync(scratch, { recursive: true });
  return { ...process.env, HOME: scratch, LC_ALL: 'C' };
}

export async function probe(dir: string): Promise<{ ok: boolean; out: string }> {
  try {
    const r = await run(lilypondBin(dir), ['--version'], { timeout: 30000 });
    return { ok: true, out: r.stdout.trim().split('\n')[0]! };
  } catch (e) {
    return { ok: false, out: String((e as Error).message) };
  }
}
