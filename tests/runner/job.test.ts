import { execFile } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import * as v from 'valibot';
import { expect } from 'vitest';
import { loadJob } from '../../runner/job.ts';
import { jsonValueSchema } from '../../runner/protocol.ts';
import { test } from '../fixtures.ts';

test('job loader reports missing entries and invalid default exports', async ({
  temporaryDirectory: directory,
}) => {
  await expect(loadJob(join(directory, 'missing.ts'))).rejects.toMatchObject({
    code: 'JOB_IMPORT',
  });
  await writeFile(join(directory, 'package.json'), '{"type":"module"}');
  const entry = join(directory, 'job.ts');
  await writeFile(entry, 'export default 42');
  await expect(loadJob(entry)).rejects.toMatchObject({ code: 'JOB_EXPORT' });
  await writeFile(entry, 'throw new Error("import failed")');
  await expect(loadJob(entry)).rejects.toMatchObject({ code: 'JOB_IMPORT' });
});

test('loader resolves project-only packages, relative TypeScript, enums and top-level await', async ({
  temporaryDirectory: directory,
}) => {
  await writeFile(join(directory, 'package.json'), '{"type":"module"}');
  const dependency = join(directory, 'node_modules', 'project-only');
  await mkdir(dependency, { recursive: true });
  await writeFile(
    join(dependency, 'package.json'),
    '{"name":"project-only","type":"module","exports":"./index.js"}',
  );
  await writeFile(join(dependency, 'index.js'), 'export const value = "project dependency"');
  await writeFile(join(directory, 'helper.ts'), 'export enum Status { Ready = "ready" }');
  const entry = join(directory, 'job.ts');
  await writeFile(
    entry,
    `
      import { value } from 'project-only'
      import { Status } from './helper.ts'
      await Promise.resolve()
      if (value !== 'project dependency' || Status.Ready !== 'ready') {
        throw new Error('Dependency resolution failed')
      }
      export default async function job() { return [] }
    `,
  );
  const job = await loadJob(entry);
  expect(v.is(v.function(), job)).toBeTruthy();
});

test('JSON input accepts nested values and rejects non-finite numbers', () => {
  const input = {
    topic: 'Bamboo',
    options: [true, null, { limit: 10 }],
  };
  expect(v.parse(jsonValueSchema, input)).toEqual(input);
  expect(() => v.parse(jsonValueSchema, { limit: Infinity })).toThrow();
});

test('CLI requires a private run-directory argument', async () => {
  const run = promisify(execFile);
  await expect(run(process.execPath, [resolve('runner/main.ts')])).rejects.toThrow(/JOB_ARGUMENTS/);
});
