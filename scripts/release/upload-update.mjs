import { existsSync } from 'node:fs';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { cosClient, fileDigest, objectKey, putObject } from './cos.mjs';
import { TARGETS, mergeRelease } from './update-feed.mjs';
import { isPrereleaseTag, parseTag } from './version.mjs';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPT_DIR, '../..');
const CONTENT_TYPES = {
  '.deb': 'application/vnd.debian.binary-package',
  '.dmg': 'application/x-apple-diskimage',
  '.exe': 'application/vnd.microsoft.portable-executable',
  '.json': 'application/json',
  '.rpm': 'application/x-rpm',
  '.zip': 'application/zip',
};

function required(value, name) {
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function requiredEnv(env, name) {
  return required(env[name]?.trim(), name);
}

function contentType(file) {
  const extension = file.slice(file.lastIndexOf('.')).toLowerCase();
  return CONTENT_TYPES[extension] ?? 'application/octet-stream';
}

function parseArgs(argv) {
  const values = new Map();
  const repeated = new Set(['--extra']);
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (!key.startsWith('--')) throw new Error(`unexpected argument ${key}`);
    if (key === '--dry-run' || key === '--notes-only') {
      values.set(key, true);
      continue;
    }
    const value = argv[index + 1];
    if (!value) throw new Error(`${key} needs a value`);
    if (repeated.has(key)) {
      const entries = values.get(key) ?? [];
      entries.push(value);
      values.set(key, entries);
    } else {
      values.set(key, value);
    }
    index += 1;
  }
  return values;
}

function loadEnvironment(envFile) {
  const path = resolve(envFile);
  if (!existsSync(path)) {
    if (envFile !== join(SCRIPT_DIR, '.env')) {
      throw new Error(`environment file does not exist: ${path}`);
    }
    return;
  }
  process.loadEnvFile(path);
}

async function currentIndex(baseUrl, target, currentIndexFile) {
  if (currentIndexFile) {
    return JSON.parse(await readFile(resolve(currentIndexFile), 'utf8'));
  }
  const url = `${baseUrl}/${target}/index.json`;
  const response = await fetch(url, {
    cache: 'no-store',
    headers: { accept: 'application/json' },
    signal: AbortSignal.timeout(30_000),
  });
  if (response.status === 404) return { schemaVersion: 1, target, releases: [] };
  if (!response.ok) {
    throw new Error(`cannot read ${url}: HTTP ${response.status}: ${await response.text()}`);
  }
  return response.json();
}

function validateIndex(index, target) {
  if (index.schemaVersion !== 1 || index.target !== target || !Array.isArray(index.releases)) {
    throw new Error(`remote index is not a schemaVersion 1 ${target} manifest`);
  }
  return index;
}

/** Read the feed from object storage so CDN caching cannot drop releases. */
export async function readOriginIndex({ client, bucket, region, key, target }) {
  let response;
  try {
    response = await client.getObject({ Bucket: bucket, Region: region, Key: key });
  } catch (error) {
    if (
      error.statusCode === 404 &&
      (error.code === 'NoSuchKey' || error.error?.Code === 'NoSuchKey')
    ) {
      return { schemaVersion: 1, target, releases: [] };
    }
    throw error;
  }
  return validateIndex(JSON.parse(String(response.Body)), target);
}

async function stageRelease({ tag, target, asset, extras, body, stageDir, index, notesOnly }) {
  const version = parseTag(tag);
  const name = TARGETS[target];
  if (!name) throw new Error(`unsupported target ${target}`);
  const targetRoot = join(stageDir, target);
  const versionRoot = join(targetRoot, version);
  await mkdir(versionRoot, { recursive: true });

  const existing = index.releases.find((entry) => entry.version === version);
  let stagedAsset;
  let release;
  if (notesOnly) {
    if (!existing) throw new Error(`release ${version} is missing from the ${target} manifest`);
    release = { ...existing, body };
  } else {
    stagedAsset = join(versionRoot, name);
    await copyFile(resolve(asset), stagedAsset);
    const digest = await fileDigest(stagedAsset);
    if (existing) {
      const existingAsset = existing.assets?.find((entry) => entry.name === name);
      if (
        existingAsset?.size !== digest.size ||
        existingAsset?.digest !== `sha256:${digest.sha256}`
      ) {
        throw new Error(`release ${version} already exists with different asset bytes`);
      }
    }
    release = {
      version,
      prerelease: isPrereleaseTag(tag),
      body,
      assets: [
        {
          name,
          url: `${version}/${name}`,
          size: digest.size,
          digest: `sha256:${digest.sha256}`,
        },
      ],
    };
  }
  const next = mergeRelease(index, release);
  await writeFile(join(targetRoot, 'index.json'), `${JSON.stringify(next, null, 2)}\n`);

  const extrasByName = new Map();
  for (const extra of extras) {
    const extraName = basename(extra);
    if (extraName === name || extrasByName.has(extraName)) {
      throw new Error(`duplicate upload asset ${extraName}`);
    }
    extrasByName.set(extraName, resolve(extra));
    await copyFile(resolve(extra), join(versionRoot, extraName));
  }
  return {
    version,
    targetRoot,
    versionRoot,
    asset: stagedAsset,
    extras: [...extrasByName.keys()].map((extraName) => join(versionRoot, extraName)),
    index: next,
  };
}

async function main(argv) {
  const values = parseArgs(argv);
  loadEnvironment(values.get('--env-file') ?? join(SCRIPT_DIR, '.env'));
  const baseUrl = requiredEnv(process.env, 'OSSIA_RELEASES_URL').replace(/\/+$/, '');
  const target = required(values.get('--target'), '--target');
  const tag = required(values.get('--tag'), '--tag');
  const notesOnly = Boolean(values.get('--notes-only'));
  const asset = notesOnly ? undefined : required(values.get('--asset'), '--asset');
  const notesFile = required(values.get('--notes-file'), '--notes-file');
  const body = (await readFile(resolve(notesFile), 'utf8')).trim();
  if (!body) throw new Error('--notes-file is empty');
  const extras = values.get('--extra') ?? [];
  if (notesOnly && extras.length) throw new Error('--notes-only cannot upload extra assets');
  const dryRun = Boolean(values.get('--dry-run'));
  const currentIndexFile = values.get('--current-index');
  if (currentIndexFile && !dryRun) throw new Error('--current-index requires --dry-run');
  const prefix = process.env.OSSIA_RELEASES_PREFIX?.trim() ?? '';
  const indexKey = objectKey(prefix, target, 'index.json');
  let client;
  let bucket;
  let region;
  let index;
  if (dryRun) {
    index = validateIndex(await currentIndex(baseUrl, target, currentIndexFile), target);
  } else {
    bucket = requiredEnv(process.env, 'OSSIA_RELEASES_BUCKET');
    region = requiredEnv(process.env, 'OSSIA_RELEASES_REGION');
    client = cosClient({
      secretId: requiredEnv(process.env, 'OSSIA_RELEASES_SECRET_ID'),
      secretKey: requiredEnv(process.env, 'OSSIA_RELEASES_SECRET_KEY'),
      securityToken: process.env.OSSIA_RELEASES_SECURITY_TOKEN?.trim() || undefined,
    });
    index = await readOriginIndex({ client, bucket, region, key: indexKey, target });
  }
  const stageDir = resolve(values.get('--stage-dir') ?? join(ROOT, 'dist/ossia-update'));
  const stage = await stageRelease({
    tag,
    target,
    asset,
    extras,
    body,
    stageDir,
    index,
    notesOnly,
  });
  const uploads = notesOnly
    ? []
    : [
        ...stage.extras.map((file) => ({
          file,
          key: objectKey(prefix, target, stage.version, basename(file)),
          cacheControl: 'public, max-age=31536000, immutable',
        })),
        {
          file: stage.asset,
          key: objectKey(prefix, target, stage.version, basename(stage.asset)),
          cacheControl: 'public, max-age=31536000, immutable',
        },
      ];
  if (dryRun) {
    console.log(
      JSON.stringify(
        {
          baseUrl,
          uploads: uploads.map(({ key }) => key),
          index: indexKey,
          stageDir: stage.targetRoot,
        },
        null,
        2,
      ),
    );
    return;
  }

  const acl = process.env.OSSIA_RELEASES_ACL?.trim() || 'public-read';
  for (const upload of uploads) {
    await putObject({
      client,
      bucket,
      region,
      key: upload.key,
      file: upload.file,
      contentType: contentType(upload.file),
      cacheControl: upload.cacheControl,
      acl,
    });
  }
  await putObject({
    client,
    bucket,
    region,
    key: indexKey,
    body: Buffer.from(`${JSON.stringify(stage.index, null, 2)}\n`),
    contentType: 'application/json',
    cacheControl: 'public, max-age=60',
    acl,
  });
  console.log(
    JSON.stringify(
      {
        target,
        version: stage.version,
        manifest: `${baseUrl}/${target}/index.json`,
        asset: stage.asset
          ? `${baseUrl}/${target}/${stage.version}/${basename(stage.asset)}`
          : undefined,
        extras: stage.extras.map(
          (file) => `${baseUrl}/${target}/${stage.version}/${basename(file)}`,
        ),
      },
      null,
      2,
    ),
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`update upload: ${error.message}`);
    process.exitCode = 1;
  });
}
