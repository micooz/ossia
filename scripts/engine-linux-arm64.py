#!/usr/bin/env python3
"""Prepare locked upstream sources, then build a native Linux ARM64 engine archive."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def digest(file):
    with file.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def fetch(entry, directory):
    file = directory / entry['file']
    if file.exists() and digest(file) == entry['sha256']:
        return file
    temporary = file.with_suffix(file.suffix + '.partial')
    try:
        with urllib.request.urlopen(entry['url'], timeout=120) as response, temporary.open('wb') as out:
            shutil.copyfileobj(response, out)
        if digest(temporary) != entry['sha256']:
            raise RuntimeError(f'checksum mismatch: {entry["file"]}')
        temporary.replace(file)
    finally:
        temporary.unlink(missing_ok=True)
    return file


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--fetch-only', action='store_true')
    parser.add_argument('--jobs', type=int, default=os.cpu_count() or 1)
    args = parser.parse_args()
    if not args.fetch_only and (platform.system() != 'Linux' or platform.machine() not in ('aarch64', 'arm64')):
        parser.error('native Linux ARM64 required; --fetch-only never starts a build')
    if sys.version_info < (3, 12):
        parser.error('Python 3.12+ required for safe source extraction')
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    lock = json.loads((ROOT / 'vendor/ENGINE.lock').read_text())['targets']['linux-arm64']
    deps = json.loads((ROOT / 'vendor/ENGINE.dependencies.json').read_text())
    if lock['source']['sha256'] != deps['sourceArchiveSha256']:
        raise RuntimeError('dependency lock does not match the engine source archive')
    downloads = work / 'downloads'
    downloads.mkdir(exist_ok=True)
    source = fetch(lock['source'], downloads)
    for entry in deps['packages']:
        fetch(entry, downloads)
    source_dir = work / 'source'
    source_dir.mkdir(exist_ok=True)
    with tarfile.open(source) as archive:
        archive.extractall(source_dir, filter='data')
    builder = source_dir / 'lilypond-2.26.0/release/binaries'
    for entry in deps['buildScripts']:
        if digest(builder / entry['path']) != entry['sha256']:
            raise RuntimeError(f'changed upstream build script: {entry["path"]}')
    if args.fetch_only:
        print('Verified source, packaging scripts, and all dependency archives; no native build performed.')
        return
    # Upstream uses a relative downloads directory; all inputs are checked before it runs.
    upstream_downloads = builder / 'downloads'
    upstream_downloads.mkdir(exist_ok=True)
    for entry in deps['packages']:
        shutil.copy2(downloads / entry['file'], upstream_downloads / entry['file'])
    env = {**os.environ, 'LC_ALL': 'C', 'PYTHONDONTWRITEBYTECODE': '1'}
    commands = [
        [sys.executable, 'build-dependencies', '--jobs', str(args.jobs)],
        [sys.executable, 'build-lilypond', '--jobs', str(args.jobs), str(source)],
    ]
    for number, command in enumerate(commands, 1):
        with (work / f'build-{number}.log').open('w') as log:
            subprocess.run(command, cwd=builder, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    archives = list(builder.glob('lilypond-2.26.0-linux-aarch64.tar.gz'))
    if len(archives) != 1:
        raise RuntimeError('expected exactly one native upstream aarch64 archive')
    artifact = work / 'lilypond-2.26.0-linux-arm64.tar.gz'
    shutil.copy2(archives[0], artifact)
    report = {'status': 'built-not-validated', 'target': 'linux-arm64', 'file': artifact.name,
              'sha256': digest(artifact), 'machine': platform.machine(), 'commands': commands,
              'next': 'Audit shared dependencies and run conversion, SVG/PDF/MIDI and relocation tests before locking or distributing this archive.'}
    (work / 'build-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
