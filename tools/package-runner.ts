import { execFile } from 'node:child_process';
import { copyFile, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const root = fileURLToPath(new URL('../', import.meta.url));
const output = fileURLToPath(new URL('../src-tauri/resources/runner', import.meta.url));
await mkdir(new URL('../src-tauri/resources/', import.meta.url), { recursive: true });
await rm(output, {
  recursive: true,
  force: true,
});
const { stdout, stderr } = await promisify(execFile)(
  'pnpm',
  [
    '--filter',
    '@bamboo/runner',
    'deploy',
    '--prod',
    '--frozen-lockfile',
    '--node-linker=hoisted',
    output,
  ],
  { cwd: root },
);
process.stdout.write(stdout);
process.stderr.write(stderr);
await copyFile(new URL('../examples/wikipedia/job.ts', import.meta.url), `${output}/example.ts`);
