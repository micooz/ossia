import { execFileSync, execFile } from 'node:child_process';
import crypto from 'node:crypto';
// Vendored LilyPond engine: verify / install / self-heal (ADR-0001 D4, ADR-0002 §8).
//
// The engine is ~128 MB unpacked, so it lives in vendor/lilypond/<tag> and is
// gitignored. vendor/ENGINE.lock is the committed source of truth: the tarball is
// fetched once, sha256-verified, and extracted with `tar` (which restores mtimes —
// LilyPond refuses to start when a compiled `.go` is older than its `.scm`).
//
//   node scripts/engine.mjs verify    # exit non-zero + reason when the tree is unusable
//   node scripts/engine.mjs install   # download + extract when missing or broken
//   node scripts/engine.mjs path      # print the engine root the server should use
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const VENDOR = path.join(ROOT, 'vendor');

// `key = "value"` lines, `#` comments. Deliberately tiny: the lock is read at server
// startup too, so it must not drag in a TOML dependency.
export function parseLock(text) {
  const out = {};
  for (const line of text.split('\n')) {
    const m = /^\s*([A-Za-z0-9_]+)\s*=\s*"([^"]*)"\s*$/.exec(line);
    if (m) out[m[1]] = m[2];
  }
  return out;
}

export function readLock(root = ROOT) {
  const p = path.join(root, 'vendor', 'ENGINE.lock');
  if (!fs.existsSync(p)) throw new Error(`missing engine lock: ${p}`);
  const lock = parseLock(fs.readFileSync(p, 'utf8'));
  for (const k of ['tag', 'version', 'url', 'file', 'sha256']) {
    if (!lock[k]) throw new Error(`engine lock missing field: ${k}`);
  }
  return lock;
}

export const sha256 = (file) =>
  crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

// Where the server looks for the engine, in precedence order. `OSSIA_LILYPOND_ROOT`
// lets a developer point at a system install without touching the vendor tree.
export function candidateRoots(root = ROOT, env = process.env, homedir = os.homedir()) {
  const lock = readLock(root);
  const list = [];
  if (env.OSSIA_LILYPOND_ROOT) list.push({ src: 'env', dir: env.OSSIA_LILYPOND_ROOT });
  list.push({ src: 'vendor', dir: path.join(root, 'vendor', 'lilypond', lock.tag) });
  list.push({
    src: 'appdata',
    dir: path.join(homedir, 'Library', 'Application Support', 'Ossia', 'engine', lock.tag),
  });
  return { lock, roots: list };
}

export function lilypondBin(dir) {
  return path.join(dir, 'bin', 'lilypond');
}

// Structural checks only; `--version` proves the Guile modules actually load.
function verifyTree(dir, lock) {
  const problems = [];
  if (!fs.existsSync(dir)) return [`not extracted: ${dir}`];
  const bin = lilypondBin(dir);
  if (!fs.existsSync(bin)) problems.push(`missing ${bin}`);
  const scm = path.join(dir, 'share', 'lilypond', lock.version, 'scm');
  if (!fs.existsSync(scm)) problems.push(`missing ${scm}`);
  if (problems.length) return problems;
  // mtime rule: every compiled module must be at least as new as its source module.
  const newest = (glob) => {
    let max = 0;
    for (const f of walk(dir, glob)) max = Math.max(max, fs.statSync(f).mtimeMs);
    return max;
  };
  const goMax = newest('.go');
  const scmMax = newest('.scm');
  if (scmMax > goMax) {
    problems.push(
      `compiled modules older than sources (max .scm ${new Date(scmMax).toISOString()}` +
        ` > max .go ${new Date(goMax).toISOString()}); run \`node scripts/engine.mjs install --refetch\``,
    );
  }
  try {
    const out = execFileSync(bin, ['--version'], { encoding: 'utf8', timeout: 30000 });
    if (!out.includes(lock.version))
      problems.push(`lilypond --version says: ${out.split('\n')[0]}`);
  } catch (e) {
    problems.push(`lilypond --version failed: ${String(e.message).slice(0, 200)}`);
  }
  return problems;
}

// Cheap recursive filename scan (the tree is ~1900 files, so this is milliseconds).
function* walk(dir, ext) {
  const stack = [dir];
  while (stack.length) {
    const d = stack.pop();
    let ents;
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

// tar -tzf listing is enough to know whether the archive is already on disk and correct;
// we only hash when we are about to extract.
async function ensureTarball(lock, opts = {}) {
  const dist = path.join(VENDOR, 'dist');
  const tarball = path.join(dist, lock.file);
  fs.mkdirSync(dist, { recursive: true });
  if (fs.existsSync(tarball) && !opts.refetch) {
    const got = sha256(tarball);
    if (got === lock.sha256) return tarball;
    if (!opts.allowDownload) throw new Error(`tarball sha mismatch: ${got} != ${lock.sha256}`);
  } else if (!opts.allowDownload) {
    throw new Error(`engine tarball not present: ${tarball}`);
  }
  process.stderr.write(`downloading ${lock.url}\n`);
  const res = await fetch(lock.url, { signal: AbortSignal.timeout(600000) });
  if (!res.ok) throw new Error(`download failed: HTTP ${res.status}`);
  const buf = Buffer.from(await res.arrayBuffer());
  const got = crypto.createHash('sha256').update(buf).digest('hex');
  if (got !== lock.sha256) throw new Error(`downloaded sha mismatch: ${got} != ${lock.sha256}`);
  fs.writeFileSync(tarball, buf);
  return tarball;
}

// Extract with `tar` (preserves mtimes + executable bits). `.go` files are then touched so
// the compiled modules are strictly newer than their sources on any host clock.
export async function install(root = ROOT, opts = {}) {
  const lock = readLock(root);
  const dir = path.join(root, 'vendor', 'lilypond', lock.tag);
  const tarball = await ensureTarball(lock, opts);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  process.stderr.write(`extracting ${path.basename(tarball)} -> ${path.relative(root, dir)}\n`);
  await run('tar', ['-xzf', tarball, '-C', dir, '--strip-components=1'], { maxBuffer: 1 << 24 });
  const touched = await refreshGoMtimes(dir);
  process.stderr.write(`refreshed ${touched} compiled modules\n`);
  const problems = verifyTree(dir, lock);
  if (problems.length) throw new Error(`engine still broken:\n  ${problems.join('\n  ')}`);
  return { dir, lock };
}

// Heal the Guile mtime rule (alternatives-evaluation §3.13): a copy that flattens mtimes
// makes LilyPond recompile its whole module tree on every run (measured: 0.3 s -> 165 s).
// Touching every `.go` to "now, or just after the newest `.scm`" restores the invariant
// without trusting the host clock. `fs.utimesSync` instead of `touch` because the file list
// is long enough to blow the argv limit.
export async function refreshGoMtimes(dir) {
  let scmMax = 0;
  for (const f of walk(dir, '.scm')) scmMax = Math.max(scmMax, fs.statSync(f).mtimeMs);
  const stamp = new Date(Math.max(Date.now(), scmMax + 60_000));
  const go = [...walk(dir, '.go')];
  if (!go.length) throw new Error(`no compiled modules under ${dir}`);
  for (const f of go) fs.utimesSync(f, stamp, stamp);
  return go.length;
}

export async function main(argv) {
  const cmd = argv[0] || 'verify';
  const opts = { refetch: argv.includes('--refetch'), allowDownload: argv.includes('--download') };
  const { lock, roots } = candidateRoots();
  if (cmd === 'path') {
    for (const c of roots)
      if (!verifyTree(c.dir, lock).length) {
        console.log(c.dir);
        return;
      }
    console.error('no usable engine; run `node scripts/engine.mjs install`');
    process.exitCode = 1;
    return;
  }
  if (cmd === 'install') {
    const { dir } = await install(ROOT, opts);
    console.log(`engine ready: ${dir}`);
    return;
  }
  if (cmd === 'verify') {
    const usable = roots.filter((c) => fs.existsSync(c.dir));
    if (!usable.length) {
      console.error(`engine not installed (looked in:\n  ${roots.map((r) => r.dir).join('\n  ')})`);
      console.error('run: node scripts/engine.mjs install');
      process.exitCode = 1;
      return;
    }
    const problems = verifyTree(usable[0].dir, lock);
    if (problems.length) {
      console.error(`engine unusable at ${usable[0].dir}:\n  ${problems.join('\n  ')}`);
      process.exitCode = 1;
      return;
    }
    console.log(`engine ok [${usable[0].src}] ${usable[0].dir} (lilypond ${lock.version})`);
    return;
  }
  console.error(`unknown command: ${cmd}`);
  process.exitCode = 2;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main(process.argv.slice(2)).catch((e) => {
    console.error(String(e.message || e));
    process.exit(1);
  });
}
