import type { Browser } from 'playwright-core';
import type { JobResult } from './job.ts';
import type { RunFailureCode, RunOutcome } from './protocol.ts';
import { realpath } from 'node:fs/promises';
import { resolve } from 'node:path';
import { chromium } from 'playwright-core';
import * as v from 'valibot';
import { executeJob, loadJob } from './job.ts';
import { publishRunOutcome, readRunRequest, runFailureSchema } from './protocol.ts';

async function disconnectBrowser(browser: Browser): Promise<void> {
  try {
    await browser.close();
  } catch (error) {
    throw Object.assign(new Error('Could not disconnect the browser', { cause: error }), {
      code: 'BROWSER_CLEANUP' as const,
    });
  }
}

async function main(): Promise<void> {
  const args = v.safeParse(v.tuple([v.pipe(v.string(), v.nonEmpty())]), process.argv.slice(2));

  if (!args.success || Number(process.versions.node.split('.')[0]) < 26) {
    throw new Error('JOB_ARGUMENTS: Requires Node 26+ and <private-run-directory>');
  }

  const directory = resolve(args.output[0]);
  let stage: RunFailureCode = 'REQUEST_INVALID';
  let outcome: RunOutcome;

  try {
    const request = await readRunRequest(directory);
    const project = await realpath(request.projectDirectory);
    const entry = resolve(project, request.entryFile);

    process.chdir(project);
    stage = 'JOB_IMPORT';

    const job = await loadJob(entry);

    stage = 'BROWSER_CONNECT';

    const browser = await chromium.connectOverCDP(request.endpoint, { timeout: 5000 });
    let records: JobResult;

    try {
      stage = 'JOB_EXECUTION';
      records = await executeJob(
        job,
        browser,
        request.input,
        request.profile ? directory : undefined,
      );
    } finally {
      await disconnectBrowser(browser);
    }

    outcome = {
      version: 1,
      status: 'success',
      records,
    };
  } catch (error) {
    const failure = v.safeParse(runFailureSchema, error);

    outcome = {
      version: 1,
      status: 'failure',
      error: {
        code: failure.success ? failure.output.code : stage,
        message: error instanceof Error ? error.message : 'Job threw a non-Error value',
      },
    };

    console.error(error);
    process.exitCode = 1;
  }

  await publishRunOutcome(directory, outcome);
}

if (import.meta.main) {
  try {
    await main();
  } catch (error) {
    console.error(
      new Error('RUNNER_FAILED: Could not complete the run protocol', { cause: error }),
    );

    process.exitCode = 1;
  }
}
