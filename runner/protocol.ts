import type { JobResult, JsonValue } from './job.ts';
import { randomUUID } from 'node:crypto';
import { constants } from 'node:fs';
import { chmod, link, lstat, mkdtemp, open, rm, writeFile } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';
import * as v from 'valibot';

const jsonScalarSchema = v.union([
  v.string(),
  v.pipe(v.number(), v.finite()),
  v.boolean(),
  v.null(),
]);
const jsonContainerSchema = v.object({});

export const jsonValueSchema = v.custom<JsonValue>((input) => {
  const ancestors = new WeakSet<object>();
  const pending: { value: unknown; leaving: boolean }[] = [
    {
      value: input,
      leaving: false,
    },
  ];

  try {
    while (pending.length > 0) {
      const frame = pending.pop();
      if (!frame) {
        return false;
      }

      const { value, leaving } = frame;
      if (v.is(jsonScalarSchema, value)) {
        continue;
      }

      if (!v.is(jsonContainerSchema, value)) {
        return false;
      }

      if (leaving) {
        ancestors.delete(value);
        continue;
      }

      if (ancestors.has(value)) {
        return false;
      }

      const array = Array.isArray(value);
      const prototype: unknown = Object.getPrototypeOf(value);
      if (
        array ? prototype !== Array.prototype : prototype !== Object.prototype && prototype !== null
      ) {
        return false;
      }

      if (Object.getOwnPropertySymbols(value).length > 0) {
        return false;
      }

      const keys = Object.getOwnPropertyNames(value);
      if (array && keys.length !== value.length + 1) {
        return false;
      }

      ancestors.add(value);
      pending.push({
        value,
        leaving: true,
      });

      for (const key of keys) {
        if (array && key === 'length') {
          continue;
        }

        if (array && (!/^(0|[1-9]\d*)$/.test(key) || Number(key) >= value.length)) {
          return false;
        }

        const descriptor = Object.getOwnPropertyDescriptor(value, key);
        if (!descriptor || !('value' in descriptor) || !descriptor.enumerable) {
          return false;
        }

        const child: unknown = descriptor.value;

        pending.push({
          value: child,
          leaving: false,
        });
      }
    }

    JSON.stringify(input);
    return true;
  } catch {
    return false;
  }
}, 'Expected JSON data without cycles, accessors or non-JSON values');

export const jobResultSchema = v.custom<JobResult>(
  (input) =>
    v.is(jsonValueSchema, input) &&
    Array.isArray(input) &&
    input.every((record) => v.is(jsonContainerSchema, record) && !Array.isArray(record)),
);

const failureCodeSchema = v.picklist([
  'REQUEST_INVALID',
  'JOB_IMPORT',
  'JOB_EXPORT',
  'BROWSER_CONNECT',
  'JOB_EXECUTION',
  'JOB_RESULT',
  'BROWSER_CLEANUP',
]);

export const runFailureSchema = v.object({
  code: failureCodeSchema,
  message: v.string(),
});

export type RunFailureCode = v.InferOutput<typeof failureCodeSchema>;

const runRequestSchema = v.strictObject({
  version: v.literal(1),
  projectDirectory: v.pipe(v.string(), v.check(isAbsolute)),
  entryFile: v.pipe(v.string(), v.nonEmpty()),
  endpoint: v.pipe(v.string(), v.url(), v.regex(/^(?:http|ws):\/\/127\.0\.0\.1:\d+\/?$/)),
  input: jsonValueSchema,
});

const runOutcomeSchema = v.variant('status', [
  v.strictObject({
    version: v.literal(1),
    status: v.literal('success'),
    records: jobResultSchema,
  }),
  v.strictObject({
    version: v.literal(1),
    status: v.literal('failure'),
    error: v.strictObject({
      code: failureCodeSchema,
      message: v.string(),
    }),
  }),
]);

export type RunRequest = v.InferOutput<typeof runRequestSchema>;
export type RunOutcome = v.InferOutput<typeof runOutcomeSchema>;

export async function validateRunDirectory(directory: string): Promise<void> {
  const metadata = await lstat(directory);
  if (
    !metadata.isDirectory() ||
    (metadata.mode & 0o077) !== 0 ||
    metadata.uid !== process.getuid?.()
  ) {
    throw new Error('RUN_DIRECTORY: Run directory must be private and owned by the current user');
  }
}

async function readPrivateText(path: string): Promise<string> {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);

  try {
    const metadata = await file.stat();
    if (
      !metadata.isFile() ||
      (metadata.mode & 0o077) !== 0 ||
      metadata.uid !== process.getuid?.()
    ) {
      throw new Error('RUN_FILE: Protocol file must be private and owned by the current user');
    }

    return await file.readFile('utf8');
  } finally {
    await file.close();
  }
}

export async function createRunDirectory(parent: string, request: RunRequest): Promise<string> {
  const parsed = v.parse(runRequestSchema, request);
  const directory = await mkdtemp(join(resolve(parent), 'bamboo-run-'));

  try {
    await chmod(directory, 0o700);
    await writeFile(join(directory, 'request.json'), JSON.stringify(parsed), {
      flag: 'wx',
      mode: 0o600,
    });
    return directory;
  } catch (error) {
    await rm(directory, {
      recursive: true,
      force: true,
    });
    throw error;
  }
}

export async function readRunRequest(directory: string): Promise<RunRequest> {
  await validateRunDirectory(directory);
  const parsed: unknown = JSON.parse(await readPrivateText(join(directory, 'request.json')));
  return v.parse(runRequestSchema, parsed);
}

export async function publishRunOutcome(directory: string, outcome: RunOutcome): Promise<void> {
  await validateRunDirectory(directory);

  const parsed = v.parse(runOutcomeSchema, outcome);
  const temporary = join(directory, `.outcome-${randomUUID()}.tmp`);

  try {
    await writeFile(temporary, JSON.stringify(parsed), {
      flag: 'wx',
      mode: 0o600,
      flush: true,
    });

    await link(temporary, join(directory, 'outcome.json'));
  } finally {
    try {
      await rm(temporary, { force: true });
    } catch (error) {
      console.error(
        new Error('OUTCOME_CLEANUP: Could not remove temporary outcome', { cause: error }),
      );
    }
  }
}

export async function readRunOutcome(directory: string): Promise<RunOutcome> {
  try {
    await validateRunDirectory(directory);
    const parsed: unknown = JSON.parse(await readPrivateText(join(directory, 'outcome.json')));
    return v.parse(runOutcomeSchema, parsed);
  } catch (error) {
    if (v.is(v.object({ code: v.literal('ENOENT') }), error)) {
      throw new Error('OUTCOME_MISSING: Runner exited without a final outcome', { cause: error });
    }
    throw new Error('OUTCOME_INVALID: Final outcome is unreadable or invalid', { cause: error });
  }
}
