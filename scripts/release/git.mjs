import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const exec = promisify(execFile);

export async function runGit(args, root = process.cwd()) {
  const { stdout } = await exec('git', args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 << 20,
  });
  return stdout;
}
