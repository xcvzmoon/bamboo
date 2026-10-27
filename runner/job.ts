import type { Browser, BrowserContext } from 'playwright-core';
import { realpath, writeFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { tsImport } from 'tsx/esm/api';
import * as v from 'valibot';
import { jobResultSchema } from './protocol.ts';

export type JsonValue = string | number | boolean | null | JsonValue[] | JsonRecord;
export type JsonRecord = { [key: string]: JsonValue };
export type JobResult = JsonRecord[];
export type JobContext = {
  context: BrowserContext;
  input: JsonValue;
};
export class AuthenticationRequiredError extends Error {
  readonly code = 'AUTHENTICATION_REQUIRED';

  constructor() {
    super('Saved login requires refresh');
    this.name = 'AuthenticationRequiredError';
  }
}

export type Job = (context: JobContext) => Promise<JobResult>;

const jobModuleSchema = v.object({ default: v.function() });

export async function loadJob(entryPath: string): Promise<Job> {
  let loaded: unknown;

  try {
    const path = await realpath(entryPath);

    v.parse(v.picklist(['.ts', '.mts']), extname(path));

    loaded = await tsImport(pathToFileURL(path).href, {
      parentURL: import.meta.url,
      tsconfig: false,
    });
  } catch (error) {
    throw Object.assign(new Error('Could not load the TypeScript job', { cause: error }), {
      code: 'JOB_IMPORT' as const,
    });
  }

  const parsed = v.safeParse(jobModuleSchema, loaded);

  if (!parsed.success) {
    throw Object.assign(new Error('The job must export a default function'), {
      code: 'JOB_EXPORT' as const,
    });
  }

  const job = parsed.output.default;

  return async (context) => {
    const result: unknown = await job(context);

    try {
      return v.parse(jobResultSchema, result);
    } catch (error) {
      throw Object.assign(
        new Error('The job must return an array of JSON records', { cause: error }),
        { code: 'JOB_RESULT' as const },
      );
    }
  };
}

export async function executeJob(
  job: Job,
  browser: Browser,
  input: JsonValue,
  profileDirectory?: string,
): Promise<JobResult> {
  let context: BrowserContext;
  try {
    context = await browser.newContext(
      profileDirectory === undefined
        ? {}
        : {
            storageState: join(profileDirectory, 'profile.json'),
          },
    );
  } catch (error) {
    if (profileDirectory === undefined) {
      throw error;
    }
    throw Object.assign(new Error('Could not restore saved login profile'), {
      code: 'PROFILE_STATE' as const,
    });
  }

  try {
    const records = await job({ context, input });
    if (profileDirectory !== undefined) {
      const state = await context.storageState({ indexedDB: true });
      await writeFile(join(profileDirectory, 'candidate.json'), JSON.stringify(state), {
        mode: 0o600,
        flag: 'wx',
        flush: true,
      });
    }
    return records;
  } finally {
    await context.close();
  }
}
