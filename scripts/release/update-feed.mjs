import { isNewerTag } from './version.mjs';

export const TARGETS = {
  'darwin-arm64': 'ossia-darwin-arm64.zip',
  'darwin-amd64': 'ossia-darwin-amd64.zip',
  'windows-amd64': 'ossia-windows-amd64.exe',
};

function compareVersions(left, right) {
  if (left === right) return 0;
  return isNewerTag(`v${left}`, `v${right}`) ? -1 : 1;
}

export function mergeRelease(index, release) {
  const releases = (index.releases ?? []).filter((entry) => entry.version !== release.version);
  releases.push(release);
  releases.sort((left, right) => compareVersions(left.version, right.version));
  return {
    schemaVersion: 1,
    target: index.target,
    releases,
  };
}
