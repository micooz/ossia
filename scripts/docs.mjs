import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';

import { parse as parseHtml } from 'parse5';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const HTML_ROOT = 'share/doc/lilypond/html';
const SECTION_CLASSES = new Set([
  'section',
  'subsection',
  'subsubsection',
  'chapter',
  'appendix',
  'unnumbered',
  'unnumberedsec',
]);
const SKIPPED_CLASSES = new Set(['nav_table', 'menu', 'contents']);
const ASSET_EXTENSIONS = new Set([
  '.png',
  '.svg',
  '.css',
  '.js',
  '.ly',
  '.woff',
  '.woff2',
  '.ttf',
  '.otf',
]);

export const sha256 = (file) =>
  crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

function attributes(node) {
  return Object.fromEntries((node.attrs ?? []).map(({ name, value }) => [name, value]));
}

function classes(node) {
  return new Set((attributes(node).class ?? '').split(/\s+/).filter(Boolean));
}

function findElements(node, predicate, output = []) {
  if (predicate(node)) output.push(node);
  for (const child of node.childNodes ?? []) findElements(child, predicate, output);
  return output;
}

function firstElement(node, predicate) {
  if (predicate(node)) return node;
  for (const child of node.childNodes ?? []) {
    const found = firstElement(child, predicate);
    if (found) return found;
  }
  return null;
}

function rawText(node) {
  if (node.nodeName === '#text') return node.value;
  return (node.childNodes ?? []).map(rawText).join('');
}

function titleOf(node) {
  const heading = firstElement(
    node,
    (child) =>
      /^h[1-6]$/.test(child.tagName ?? '') &&
      [...classes(child)].some((name) => SECTION_CLASSES.has(name)),
  );
  return heading ? rawText(heading).replace(/\s+/g, ' ').trim() : '';
}

function markdownOf(node) {
  if (node.nodeName === '#text') return node.value.replace(/\s+/g, ' ');
  if (!node.tagName) return (node.childNodes ?? []).map(markdownOf).join('');
  if (
    node.tagName === 'script' ||
    node.tagName === 'style' ||
    [...classes(node)].some((name) => SKIPPED_CLASSES.has(name))
  )
    return '';
  const children = () => (node.childNodes ?? []).map(markdownOf).join('');
  if (node.tagName === 'pre') return `\n\n\`\`\`lilypond\n${rawText(node).trim()}\n\`\`\`\n\n`;
  if (node.tagName === 'code') return `\`${rawText(node)}\``;
  if (/^h[1-6]$/.test(node.tagName))
    return `\n${'#'.repeat(Number(node.tagName[1]) + 1)} ${rawText(node).trim()}\n`;
  if (node.tagName === 'li') return `\n- ${children().trim()}\n`;
  if (node.tagName === 'br' || node.tagName === 'hr') return '\n';
  if (['p', 'tr', 'blockquote', 'div', 'table'].includes(node.tagName)) return `\n${children()}\n`;
  return children();
}

function localReference(page, value) {
  if (
    !value ||
    /^[a-z][a-z0-9+.-]*:/i.test(value) ||
    value.startsWith('#') ||
    value.startsWith('/')
  )
    return null;
  const pathname = decodeURIComponent(value.split(/[?#]/, 1)[0]);
  const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(page), pathname));
  return resolved.startsWith('../') ? null : resolved;
}

function parseChapter(relative, data) {
  const document = parseHtml(data.toString('utf8'));
  const main = findElements(document, (node) => attributes(node).id === 'main')[0];
  if (!main) return { sections: [], references: [] };
  const references = new Set();
  findElements(main, (node) => {
    const attrs = attributes(node);
    for (const value of [attrs.href, attrs.src]) {
      const reference = localReference(relative, value);
      if (reference) references.add(reference);
    }
    return false;
  });
  const sections = [];
  for (const node of findElements(
    main,
    (candidate) =>
      candidate.tagName === 'div' &&
      Boolean(attributes(candidate).id) &&
      [...classes(candidate)].some((name) => SECTION_CLASSES.has(name)),
  )) {
    const title = titleOf(node);
    if (!title) continue;
    sections.push({
      anchor: attributes(node).id,
      title,
      body: markdownOf(node)
        .replace(/\n{3,}/g, '\n\n')
        .trim(),
    });
  }
  return { sections, references };
}

function tar(args, options = {}) {
  return execFileSync('tar', args, {
    cwd: options.cwd,
    env: { ...process.env, XZ_OPT: '-3' },
    maxBuffer: 64 << 20,
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : ['ignore', 'inherit', 'inherit'],
    encoding: options.capture ? 'utf8' : undefined,
  });
}

function copyIfFile(source, destination) {
  if (!fs.statSync(source, { throwIfNoEntry: false })?.isFile()) return;
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
}

function fileIdentity(stage) {
  const files = [];
  const visit = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const absolute = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(absolute);
      if (!entry.isFile()) continue;
      files.push({
        path: path.relative(stage, absolute).split(path.sep).join('/'),
        sha256: sha256(absolute),
        bytes: fs.statSync(absolute).size,
      });
    }
  };
  visit(stage);
  files.sort((left, right) => left.path.localeCompare(right.path));
  const identity = files.map((file) => `${file.path}\0${file.sha256}\0${file.bytes}\n`).join('');
  return { files, contentHash: crypto.createHash('sha256').update(identity).digest('hex') };
}

export function prepare(archive, output, lock) {
  if (sha256(archive) !== lock.sha256) throw new Error('documentation archive checksum mismatch');
  fs.mkdirSync(path.dirname(output), { recursive: true });
  const temporary = fs.mkdtempSync(path.join(path.dirname(output), 'docs-'));
  try {
    const extracted = path.join(temporary, 'extracted');
    const archiveStage = path.join(temporary, 'archive');
    const stage = path.join(temporary, 'pack');
    fs.mkdirSync(extracted);
    fs.mkdirSync(archiveStage);
    fs.mkdirSync(stage);
    const archiveName = `lilypond-${lock.version}-en.tar.xz`;
    const manuals = new Set(lock.manuals);
    const selected = tar(['-tf', archive], { capture: true })
      .split('\n')
      .map((name) => name.replace(/^\.\//, ''))
      .filter(Boolean)
      .filter((name) => {
        const match = name.match(
          /^share\/doc\/lilypond\/html\/Documentation\/([^/]+)\/([^/]+\.html)$/,
        );
        if (match && manuals.has(match[1]) && !match[2].slice(0, -'.html'.length).includes('.'))
          return true;
        return /^share\/doc\/lilypond\/html\/(?:COPYING(?:\.FDL)?|LICENSE(?:\.[A-Z]+)?|AUTHORS\.txt|VERSION)$/.test(
          name,
        );
      });
    if (!selected.length) throw new Error('documentation archive contained no indexed chapters');
    const fileList = path.join(temporary, 'files.txt');
    fs.writeFileSync(fileList, `${selected.map((name) => `./${name}`).join('\n')}\n`);
    tar(['-xJf', archive, '-C', extracted, '-T', fileList]);

    const references = new Set();
    const chapters = [];
    for (const name of selected) {
      const relative = name.slice(`${HTML_ROOT}/`.length);
      const source = path.join(extracted, name);
      if (!relative.startsWith('Documentation/')) {
        copyIfFile(source, path.join(archiveStage, relative));
        continue;
      }
      copyIfFile(source, path.join(archiveStage, relative));
      const parsed = parseChapter(relative, fs.readFileSync(source));
      for (const reference of parsed.references) {
        if (ASSET_EXTENSIONS.has(path.extname(reference).toLowerCase())) references.add(reference);
      }
      const [manual, filename] = relative.slice('Documentation/'.length).split('/');
      const stem = filename.slice(0, -'.html'.length);
      for (const section of parsed.sections) {
        const identity = `${manual}/${stem}#${section.anchor}`;
        const url = `https://lilypond.org/doc/v${lock.version
          .split('.')
          .slice(0, 2)
          .join('.')}/${relative}#${section.anchor}`;
        const commands = [...section.body.matchAll(/\\([A-Za-z][A-Za-z-]*)/g)]
          .map((match) => match[1])
          .filter((value, index, all) => all.indexOf(value) === index)
          .sort((left, right) => left.localeCompare(right))
          .join(' ');
        chapters.push([
          identity,
          manual,
          section.title,
          section.body,
          url,
          archiveName,
          section.anchor,
          commands,
        ]);
      }
    }
    if (references.size) {
      const assetList = path.join(temporary, 'assets.txt');
      fs.writeFileSync(
        assetList,
        `${[...references].map((name) => `./${HTML_ROOT}/${name}`).join('\n')}\n`,
      );
      tar(['-xJf', archive, '-C', extracted, '-T', assetList]);
      for (const reference of references)
        copyIfFile(path.join(extracted, HTML_ROOT, reference), path.join(archiveStage, reference));
    }
    if (!chapters.length) throw new Error('documentation archive contained no indexed chapters');
    chapters.sort((left, right) => left[0].localeCompare(right[0]));
    tar(['-cJf', path.join(stage, archiveName), '-C', archiveStage, '.']);
    const notices = path.join(stage, 'notices');
    fs.mkdirSync(notices);
    for (const name of selected) {
      if (name.startsWith(`${HTML_ROOT}/Documentation/`)) continue;
      copyIfFile(path.join(extracted, name), path.join(notices, path.posix.basename(name)));
    }
    const database = new DatabaseSync(path.join(stage, 'index.sqlite'));
    database.exec(
      'CREATE TABLE chapters (id TEXT PRIMARY KEY, manual TEXT, title TEXT, body TEXT, url TEXT, path TEXT, anchor TEXT, commands TEXT);' +
        'CREATE VIRTUAL TABLE search USING fts5(id UNINDEXED, title, commands, body);' +
        'BEGIN',
    );
    const insertChapter = database.prepare('INSERT INTO chapters VALUES (?,?,?,?,?,?,?,?)');
    const insertSearch = database.prepare('INSERT INTO search VALUES (?,?,?,?)');
    for (const chapter of chapters) {
      insertChapter.run(...chapter);
      insertSearch.run(chapter[0], chapter[2], chapter[7], chapter[3]);
    }
    database.exec('COMMIT; VACUUM;');
    database.close();
    fs.writeFileSync(path.join(stage, 'source-lock.json'), `${JSON.stringify(lock, null, 2)}\n`);
    const identity = fileIdentity(stage);
    const manifest = {
      version: lock.version,
      formatVersion: lock.formatVersion,
      archiveSha256: lock.sha256,
      contentHash: identity.contentHash,
      chapters: chapters.length,
      files: identity.files,
    };
    fs.writeFileSync(path.join(stage, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
    if (fs.existsSync(output)) fs.rmSync(output, { recursive: true, force: true });
    fs.renameSync(stage, output);
    return manifest;
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

async function download(url, destination) {
  const response = await fetch(url);
  if (!response.ok || !response.body)
    throw new Error(`documentation download failed: ${response.status}`);
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  await pipeline(Readable.fromWeb(response.body), fs.createWriteStream(destination));
}

export async function main(argv = process.argv.slice(2)) {
  const options = new Set(argv);
  const value = (flag) => {
    const index = argv.indexOf(flag);
    return index === -1 ? undefined : argv[index + 1];
  };
  const lock = JSON.parse(fs.readFileSync(path.join(ROOT, 'vendor/DOCS.lock'), 'utf8'));
  const archive = path.resolve(value('--archive') ?? path.join(ROOT, 'vendor/dist', lock.file));
  const output = path.resolve(
    value('--output') ?? path.join(ROOT, 'vendor/docs/lilypond', lock.version),
  );
  const manifestPath = path.join(output, 'manifest.json');
  if (!options.has('--refresh') && fs.existsSync(manifestPath)) {
    const existing = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    if (
      existing.version === lock.version &&
      existing.formatVersion === lock.formatVersion &&
      existing.archiveSha256 === lock.sha256
    ) {
      const { files: _files, ...summary } = existing;
      process.stdout.write(`${JSON.stringify(summary)}\n`);
      return existing;
    }
  }
  if (!fs.existsSync(archive)) {
    if (!options.has('--download'))
      throw new Error('documentation archive missing; use --download');
    await download(lock.url, archive);
  }
  const manifest = prepare(archive, output, lock);
  const { files: _files, ...summary } = manifest;
  process.stdout.write(`${JSON.stringify(summary)}\n`);
  return manifest;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  await main();
