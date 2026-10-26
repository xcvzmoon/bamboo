import type { JsonValue } from '../../runner/job.ts';
import type { RunRequest } from '../../runner/protocol.ts';
import { execFile } from 'node:child_process';
import { chmod, lstat, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import * as v from 'valibot';
import { expect } from 'vitest';
import {
  createRunDirectory,
  jobResultSchema,
  jsonValueSchema,
  publishRunOutcome,
  readRunOutcome,
  readRunRequest,
  validateRunDirectory,
} from '../../runner/protocol.ts';
import { test as baseTest } from '../fixtures.ts';

function requestFixture(): RunRequest {
  return {
    version: 1,
    projectDirectory: resolve('.'),
    entryFile: 'missing.ts',
    endpoint: 'http://127.0.0.1:1',
    input: { nested: [true, null, 1] },
  };
}

const test = baseTest.extend('runDirectory', ({ temporaryDirectory }) =>
  createRunDirectory(temporaryDirectory, requestFixture()),
);

async function invokeRunner(directory: string): Promise<{ stdout: string; stderr: string }> {
  return await promisify(execFile)(process.execPath, [resolve('runner/main.ts'), directory], {
    timeout: 15_000,
  });
}

test('result validation rejects lossy JSON and accepts shared references', () => {
  const cycle: JsonValue[] = [];
  cycle.push(cycle);
  const sparse: JsonValue[] = [];
  sparse.length = 1;
  const invalid: unknown[] = [
    undefined,
    null,
    42,
    {},
    [42],
    [null],
    [[]],
    [undefined],
    [{ value: undefined }],
    [{ value: NaN }],
    [{ value: Infinity }],
    [{ value: 1n }],
    [{ value: Symbol('value') }],
    [{ value: () => 1 }],
    [new Date()],
    [{ value: new Map() }],
    [{ value: cycle }],
    [{ value: sparse }],
    [{ [Symbol('key')]: true }],
    [Object.defineProperty({}, 'hidden', { value: true })],
  ];
  for (const value of invalid) {
    expect(v.is(jobResultSchema, value)).toBe(false);
  }
  const shared = { value: [null, true, 'text', 1] };
  const valid = [
    {
      first: shared,
      second: shared,
    },
  ];
  expect(v.is(jobResultSchema, valid)).toBe(true);
  expect(v.is(jobResultSchema, [])).toBe(true);
  let accessed = false;
  const accessor = Object.defineProperty({}, 'value', {
    enumerable: true,
    get() {
      accessed = true;
      return 'not data';
    },
  });
  expect(v.is(jobResultSchema, [accessor])).toBe(false);
  expect(accessed).toBe(false);
  const properties: JsonValue[] = [];
  Object.defineProperty(properties, 'extra', {
    value: true,
    enumerable: true,
  });
  expect(v.is(jsonValueSchema, properties)).toBe(false);
});

test('private protocol files preserve JSON keys and publish a complete outcome once', async ({
  runDirectory: directory,
}) => {
  expect((await lstat(directory)).mode & 0o777).toBe(0o700);
  expect((await lstat(join(directory, 'request.json'))).mode & 0o777).toBe(0o600);
  await validateRunDirectory(directory);
  expect(await readRunRequest(directory)).toEqual(requestFixture());
  await expect(readRunOutcome(directory)).rejects.toThrow(/OUTCOME_MISSING/);
  const records = v.parse(
    jobResultSchema,
    JSON.parse('[{"__proto__":{"safe":true},"constructor":"data","prototype":null}]'),
  );
  for (let index = 0; index < 2000; index += 1) {
    records.push({
      index,
      text: 'record data'.repeat(8),
    });
  }
  const outcomes = await Promise.allSettled([
    publishRunOutcome(directory, {
      version: 1,
      status: 'success',
      records,
    }),
    publishRunOutcome(directory, {
      version: 1,
      status: 'success',
      records,
    }),
  ]);
  expect(outcomes[0]?.status === 'fulfilled' || outcomes[1]?.status === 'fulfilled').toBe(true);
  expect(outcomes[0]?.status === 'rejected' || outcomes[1]?.status === 'rejected').toBe(true);
  expect(await readRunOutcome(directory)).toEqual({
    version: 1,
    status: 'success',
    records,
  });
  expect((await lstat(join(directory, 'outcome.json'))).mode & 0o777).toBe(0o600);
  expect((await readdir(directory)).toSorted()).toEqual(['outcome.json', 'request.json']);
});

test('protocol rejects unknown versions, malformed JSON, and failure records', async ({
  runDirectory: directory,
}) => {
  await writeFile(join(directory, 'request.json'), '{');
  await expect(readRunRequest(directory)).rejects.toThrow();
  await writeFile(
    join(directory, 'request.json'),
    JSON.stringify({
      ...requestFixture(),
      version: 2,
    }),
  );
  await expect(readRunRequest(directory)).rejects.toThrow();
  await writeFile(
    join(directory, 'outcome.json'),
    JSON.stringify({
      version: 1,
      status: 'failure',
      error: {
        code: 'JOB_EXECUTION',
        message: 'failed',
      },
      records: [],
    }),
    { mode: 0o600 },
  );
  await expect(readRunOutcome(directory)).rejects.toThrow(/OUTCOME_INVALID/);
});

test('insecure files and symlinks are rejected', async ({ runDirectory: directory }) => {
  await chmod(directory, 0o755);
  await expect(validateRunDirectory(directory)).rejects.toThrow(/RUN_DIRECTORY/);
  await chmod(directory, 0o700);
  const requestPath = join(directory, 'request.json');
  await chmod(requestPath, 0o644);
  await expect(readRunRequest(directory)).rejects.toThrow(/RUN_FILE/);
  await rm(requestPath);
  await symlink(join(directory, 'target.json'), requestPath);
  await expect(readRunRequest(directory)).rejects.toThrow();
});

test('malformed requests publish a failure without records', async ({
  runDirectory: directory,
}) => {
  await writeFile(join(directory, 'request.json'), '{');
  await expect(invokeRunner(directory)).rejects.toThrow();
  const outcome = await readRunOutcome(directory);
  expect(outcome.status).toBe('failure');
  if (outcome.status === 'failure') {
    expect(outcome.error.code).toBe('REQUEST_INVALID');
  }
  expect('records' in outcome).toBe(false);
  expect((await readdir(directory)).toSorted()).toEqual(['outcome.json', 'request.json']);
});

test('an abrupt zero exit and forged stdout cannot substitute for an outcome', async ({
  runDirectory: directory,
}) => {
  await writeFile(join(directory, 'package.json'), '{"type":"module"}');
  await writeFile(
    join(directory, 'exit.ts'),
    `console.info('{"version":1,"status":"success","records":[]}'); process.exit(0)`,
  );
  await writeFile(
    join(directory, 'request.json'),
    JSON.stringify({
      ...requestFixture(),
      projectDirectory: directory,
      entryFile: 'exit.ts',
    }),
  );
  const { stdout } = await invokeRunner(directory);
  expect(stdout).toMatch(/"success"/);
  await expect(readRunOutcome(directory)).rejects.toThrow(/OUTCOME_MISSING/);
  await expect(readFile(join(directory, 'outcome.json'))).rejects.toThrow();
});
