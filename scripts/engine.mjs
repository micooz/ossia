import { execFileSync, execFile } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const sha256 = (file) =>
  crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
export function hostTarget(platform = process.platform, arch = process.arch) {
  const target = `${platform === 'win32' ? 'mingw' : platform}-${arch === 'x64' ? 'x86_64' : arch}`;
  if (
    !['darwin-arm64', 'darwin-x86_64', 'mingw-x86_64', 'linux-arm64', 'linux-x86_64'].includes(
      target,
    )
  )
    throw new Error(`unsupported engine target: ${target}`);
  return target;
}
export function parseLock(text) {
  const lock = JSON.parse(text);
  if (lock.schemaVersion !== 1 || lock.version !== '2.26.0' || !lock.targets)
    throw new Error('unsupported engine lock');
  return lock;
}
export function readLock(root = ROOT, target = hostTarget()) {
  const lock = parseLock(fs.readFileSync(path.join(root, 'vendor/ENGINE.lock'), 'utf8'));
  const entry = lock.targets[target];
  if (!entry) throw new Error(`unlocked engine target: ${target}`);
  return { ...entry, version: lock.version, target, tag: `${lock.version}-${target}` };
}
function confined(root, relative) {
  if (
    !relative ||
    relative.includes('\\') ||
    relative.includes(':') ||
    relative.split('/').some((p) => !p || p === '..' || p === '.')
  )
    throw new Error('invalid engine resource path');
  const value = path.join(root, relative);
  const base = fs.realpathSync(root);
  let ancestor = value;
  while (!fs.existsSync(ancestor)) {
    if (
      fs.lstatSync(ancestor, { throwIfNoEntry: false })?.isSymbolicLink() &&
      !fs.existsSync(ancestor)
    )
      throw new Error('dangling resource symlink');
    ancestor = path.dirname(ancestor);
  }
  const real = fs.realpathSync(ancestor);
  if (real !== base && !real.startsWith(`${base}${path.sep}`))
    throw new Error('engine path escapes root');
  return value;
}
function files(root, dir = root) {
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .flatMap((entry) => {
      const absolute = path.join(dir, entry.name);
      const relative = path.relative(root, absolute).split(path.sep).join('/');
      if (
        relative === 'resources.json' ||
        entry.name === '__pycache__' ||
        entry.name.endsWith('.pyc')
      )
        return [];
      if (entry.isDirectory()) return files(root, absolute);
      if (!entry.isFile()) throw new Error('unexpected engine symlink or special file');
      return [relative];
    })
    .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}
export function candidateRoots(root = ROOT, env = process.env, target = hostTarget()) {
  const lock = readLock(root, target);
  // An explicit developer override is authoritative; failure must not select another build.
  return {
    lock,
    roots: [
      {
        src: env.OSSIA_LILYPOND_ROOT ? 'env' : 'vendor',
        dir: env.OSSIA_LILYPOND_ROOT ?? path.join(root, 'vendor/lilypond', lock.tag),
      },
    ],
  };
}
export function lilypondBin(dir) {
  return path.join(dir, process.platform === 'win32' ? 'bin/lilypond.exe' : 'bin/lilypond');
}
function command(dir, tool) {
  return [confined(dir, tool.program), tool.args.map((arg) => confined(dir, arg))];
}
export function verifyTree(dir, lock, native = true) {
  if (!lock.archive) throw new Error(`${lock.target} is unbuilt; use the locked native recipe`);
  const inventory = JSON.parse(fs.readFileSync(confined(dir, 'resources.json'), 'utf8'));
  if (
    inventory.schemaVersion !== 1 ||
    inventory.target !== lock.target ||
    inventory.engineBuildId !== lock.engineBuildId ||
    inventory.archiveSha256 !== lock.archive.sha256
  )
    throw new Error('engine inventory does not match lock');
  const paths = inventory.files.map((f) => f.path);
  if (
    JSON.stringify([...paths].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))) !==
    JSON.stringify(files(dir))
  )
    throw new Error('incomplete engine inventory');
  for (const file of inventory.files) {
    const absolute = confined(dir, file.path);
    const stat = fs.statSync(absolute);
    if (
      stat.size !== file.bytes ||
      sha256(absolute) !== file.sha256 ||
      (process.platform !== 'win32' && file.executable && !(stat.mode & 0o111))
    )
      throw new Error(`damaged engine resource: ${file.path}`);
  }
  for (const required of lock.requiredDirectories)
    if (
      !fs.statSync(confined(dir, required)).isDirectory() ||
      !fs.readdirSync(confined(dir, required)).length
    )
      throw new Error(`missing engine dependency: ${required}`);
  for (const tool of Object.values(lock.tools)) command(dir, tool);
  if (!native) return;
  if (lock.target !== hostTarget()) throw new Error('cannot execute a foreign-target engine');
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-engine-verify-'));
  try {
    for (const name of ['home', 'cache', 'tmp']) fs.mkdirSync(path.join(scratch, name));
    const env = {
      ...process.env,
      HOME: path.join(scratch, 'home'),
      USERPROFILE: path.join(scratch, 'home'),
      XDG_CACHE_HOME: path.join(scratch, 'cache'),
      TMPDIR: path.join(scratch, 'tmp'),
      TMP: path.join(scratch, 'tmp'),
      TEMP: path.join(scratch, 'tmp'),
      GUILE_AUTO_COMPILE: '0',
      PYTHONDONTWRITEBYTECODE: '1',
      LC_ALL: 'C.UTF-8',
    };
    for (const key of ['PYTHONHOME', 'PYTHONPATH', 'GUILE_LOAD_PATH', 'GUILE_LOAD_COMPILED_PATH'])
      delete env[key];
    for (const tool of Object.values(lock.tools)) {
      const [program, prefix] = command(dir, tool);
      const output = execFileSync(program, [...prefix, '--version'], {
        cwd: scratch,
        env,
        encoding: 'utf8',
        timeout: 30000,
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      if (!/\b2\.26\.0(?:\s|$)/.test(output)) throw new Error('wrong engine tool version');
    }
    fs.writeFileSync(
      path.join(scratch, 'score.ly'),
      '\\version "2.26.0"\n\\score { { c\'1 } \\layout {} }\n',
    );
    const [program, prefix] = command(dir, lock.tools.lilypond);
    execFileSync(program, [...prefix, '--svg', '-o', 'score', 'score.ly'], {
      cwd: scratch,
      env,
      timeout: 30000,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    if (!fs.existsSync(path.join(scratch, 'score.svg')))
      throw new Error('engine SVG smoke test produced no output');
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}
export async function refreshGoMtimes(dir) {
  const all = files(dir);
  const newest = Math.max(
    0,
    ...all.filter((f) => f.endsWith('.scm')).map((f) => fs.statSync(path.join(dir, f)).mtimeMs),
  );
  const stamp = new Date(Math.max(Date.now(), newest + 1000));
  const compiled = all.filter((f) => f.endsWith('.go'));
  if (!compiled.length) throw new Error('engine has no compiled Guile modules');
  for (const file of compiled) fs.utimesSync(path.join(dir, file), stamp, stamp);
  return compiled.length;
}
export function writeInventory(dir, lock) {
  const inventory = {
    schemaVersion: 1,
    target: lock.target,
    engineBuildId: lock.engineBuildId,
    archiveSha256: lock.archive.sha256,
    files: files(dir).map((file) => ({
      path: file,
      sha256: sha256(path.join(dir, file)),
      bytes: fs.statSync(path.join(dir, file)).size,
      executable: !!(fs.statSync(path.join(dir, file)).mode & 0o111),
    })),
  };
  fs.writeFileSync(path.join(dir, 'resources.json'), `${JSON.stringify(inventory, null, 2)}\n`);
}
async function ensureArchive(root, lock, opts) {
  if (!lock.archive) throw new Error(`${lock.target} has no locked binary archive`);
  const dir = path.join(root, 'vendor/dist');
  fs.mkdirSync(dir, { recursive: true });
  const archive = path.join(dir, lock.archive.file);
  if (!opts.refetch && fs.existsSync(archive) && sha256(archive) === lock.archive.sha256)
    return archive;
  if (!opts.allowDownload) throw new Error('verified engine archive missing; pass --download');
  const response = await fetch(lock.archive.url, { signal: AbortSignal.timeout(600000) });
  if (!response.ok) throw new Error(`engine download: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (crypto.createHash('sha256').update(bytes).digest('hex') !== lock.archive.sha256)
    throw new Error('engine archive checksum mismatch');
  const temp = `${archive}.${crypto.randomUUID()}.tmp`;
  try {
    fs.writeFileSync(temp, bytes);
    fs.renameSync(temp, archive);
  } finally {
    fs.rmSync(temp, { force: true });
  }
  return archive;
}
export async function install(root = ROOT, opts = {}) {
  const lock = readLock(root, opts.target ?? hostTarget());
  const archive = await ensureArchive(root, lock, opts);
  const parent = path.join(root, 'vendor/lilypond');
  fs.mkdirSync(parent, { recursive: true });
  const stage = fs.mkdtempSync(path.join(parent, '.prepare-'));
  const dir = path.join(parent, lock.tag);
  const previous = `${dir}.${crypto.randomUUID()}.old`;
  try {
    const { stdout } = await run('tar', ['-tf', archive], { maxBuffer: 16 << 20 });
    for (const item of stdout.trim().split(/\r?\n/)) {
      if (
        !item.startsWith(lock.archive.stripPrefix) &&
        item !== lock.archive.stripPrefix.slice(0, -1)
      )
        throw new Error('unexpected archive root');
      if (item.includes('\\') || item.split('/').includes('..') || item.startsWith('/'))
        throw new Error('unsafe archive path');
    }
    await run('tar', ['-xf', archive, '-C', stage, '--strip-components=1'], {
      maxBuffer: 16 << 20,
    });
    await refreshGoMtimes(stage);
    writeInventory(stage, lock);
    verifyTree(stage, lock, lock.target === hostTarget() && !opts.skipNative);
    if (fs.existsSync(dir)) fs.renameSync(dir, previous);
    try {
      fs.renameSync(stage, dir);
    } catch (error) {
      if (fs.existsSync(previous)) fs.renameSync(previous, dir);
      throw error;
    }
    fs.rmSync(previous, { recursive: true, force: true });
    return { dir, lock };
  } finally {
    fs.rmSync(stage, { recursive: true, force: true });
  }
}
export async function main(argv) {
  const cmd = argv[0] ?? 'verify';
  const target =
    argv.find((a) => a.startsWith('--target='))?.slice('--target='.length) ?? hostTarget();
  if (cmd === 'install') {
    const { dir } = await install(ROOT, {
      target,
      refetch: argv.includes('--refetch'),
      allowDownload: argv.includes('--download'),
      skipNative: argv.includes('--skip-native'),
    });
    console.log(`engine ready: ${dir}`);
  } else if (cmd === 'verify' || cmd === 'path') {
    const { roots } = candidateRoots(ROOT, process.env, target);
    const lock = readLock(ROOT, target);
    const dir = roots[0].dir;
    verifyTree(dir, lock, target === hostTarget());
    console.log(cmd === 'path' ? dir : `engine ok: ${lock.target} (${lock.engineBuildId})`);
  } else throw new Error(`unknown engine command: ${cmd}`);
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href)
  await main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
