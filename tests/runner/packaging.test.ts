import { execFile } from 'node:child_process';
import { cp, lstat, mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { expect } from 'vitest';
import { createRunDirectory, readRunOutcome } from '../../runner/protocol.ts';
import { test } from './fixtures.ts';

test(
  'deployed runner executes outside the repository with a minimal PATH',
  {
    skip: process.env.BAMBOO_RUNTIME_TEST !== '1',
    timeout: 30_000,
  },
  async ({ temporaryDirectory: directory, runtime: { endpoint } }) => {
    await Promise.all(
      ['playwright-core', 'tsx', 'valibot', 'esbuild'].map(async (name) => {
        const metadata = await lstat(resolve('src-tauri/resources/runner/node_modules', name));
        expect(metadata.isDirectory(), name).toBe(true);
        expect(metadata.isSymbolicLink(), name).toBe(false);
      }),
    );
    const assets = join(directory, 'packaged runner');
    await cp(resolve('src-tauri/resources/runner'), assets, {
      recursive: true,
      verbatimSymlinks: true,
    });
    const project = join(directory, 'external project');
    await mkdir(project);
    await writeFile(join(project, 'package.json'), '{"type":"module"}');
    await writeFile(
      join(project, 'helper.ts'),
      'export enum Value { Ready = "private loader works" }',
    );
    await writeFile(
      join(project, 'job.ts'),
      `import { Value } from './helper.ts'
    export default async function job({ context, input }) {
      const page = await context.newPage()
      await page.setContent('<h1>Packaged job</h1>')
      return [{ heading: await page.locator('h1').innerText(), value: Value.Ready, input }]
    }`,
    );
    const run = await createRunDirectory(directory, {
      version: 1,
      projectDirectory: project,
      entryFile: 'job.ts',
      endpoint,
      input: { topic: 'Bamboo' },
    });
    await promisify(execFile)(process.execPath, [join(assets, 'main.ts'), run], {
      cwd: project,
      env: {
        ...process.env,
        PATH: '/usr/bin:/bin',
      },
      timeout: 20_000,
    });
    await expect(readRunOutcome(run)).resolves.toEqual({
      version: 1,
      status: 'success',
      records: [
        {
          heading: 'Packaged job',
          value: 'private loader works',
          input: { topic: 'Bamboo' },
        },
      ],
    });
  },
);
