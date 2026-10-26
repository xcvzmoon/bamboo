import type { Browser, BrowserContext } from 'playwright-core';
import { realpath } from 'node:fs/promises';
import { extname } from 'node:path';
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

export async function executeJob(job: Job, browser: Browser, input: JsonValue): Promise<JobResult> {
  const context = await browser.newContext();

  try {
    return await job({ context, input });
  } finally {
    await context.close();
  }
}
