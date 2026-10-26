import { execFile } from 'node:child_process';
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { describe, expect } from 'vitest';
import { executeJob, loadJob } from '../../runner/job.ts';
import { createRunDirectory, readRunOutcome } from '../../runner/protocol.ts';
import { test } from './fixtures.ts';

describe.runIf(process.env.BAMBOO_RUNTIME_TEST === '1')('Lightpanda jobs', () => {
  test.beforeEach(async ({ temporaryDirectory }) => {
    await writeFile(join(temporaryDirectory, 'package.json'), '{"type":"module"}');
  });

  test('extracts JavaScript content and isolates each job context', async ({
    temporaryDirectory: directory,
    runtime: { browser },
  }) => {
    const entry = join(directory, 'extract.ts');
    await writeFile(
      entry,
      `export default async function job({ context, input }) {
      const page = await context.newPage()
      await page.setContent('<h1>Loading</h1><script>document.querySelector("h1").textContent="Ready"</script>')
      return [{ heading: await page.locator('h1').innerText(), input, pages: context.pages().length }]
    }`,
    );
    const job = await loadJob(entry);
    const input = {
      nested: [true, null, 3],
      topic: 'Bamboo',
    };
    const before = browser.contexts().length;
    await expect(executeJob(job, browser, input)).resolves.toEqual([
      {
        heading: 'Ready',
        input,
        pages: 1,
      },
    ]);
    await expect(executeJob(job, browser, null)).resolves.toEqual([
      {
        heading: 'Ready',
        input: null,
        pages: 1,
      },
    ]);
    expect(browser.contexts()).toHaveLength(before);
  });

  test('closes a throwing job context and publishes a failure without records', async ({
    temporaryDirectory: directory,
    runtime: { browser, endpoint },
  }) => {
    const entry = join(directory, 'throw.ts');
    await writeFile(
      entry,
      'export default async function job() { throw new Error("fixture failure") }',
    );
    const before = browser.contexts().length;
    await expect(executeJob(await loadJob(entry), browser, null)).rejects.toThrow(
      'fixture failure',
    );
    expect(browser.contexts()).toHaveLength(before);
    const run = await createRunDirectory(directory, {
      version: 1,
      projectDirectory: directory,
      entryFile: 'throw.ts',
      endpoint,
      input: null,
    });
    await expect(
      promisify(execFile)(process.execPath, [resolve('runner/main.ts'), run], { timeout: 15_000 }),
    ).rejects.toThrow('fixture failure');
    const outcome = await readRunOutcome(run);
    expect(outcome).toMatchObject({
      status: 'failure',
      error: { code: 'JOB_EXECUTION' },
    });
    expect(outcome).not.toHaveProperty('records');
  });

  test('uses the project working directory and keeps logs out of results', async ({
    temporaryDirectory: directory,
    runtime: { endpoint },
  }) => {
    await writeFile(join(directory, 'project.json'), '{"value":"project cwd"}');
    await writeFile(
      join(directory, 'cwd.ts'),
      `import { readFile } from 'node:fs/promises'
      export default async function job({ input }) {
        console.info('job stdout')
        console.error('job stderr')
        return [{ text: await readFile('project.json', 'utf8'), input }]
      }`,
    );
    const run = await createRunDirectory(directory, {
      version: 1,
      projectDirectory: directory,
      entryFile: 'cwd.ts',
      endpoint,
      input: { topic: 'Bamboo' },
    });
    const { stdout, stderr } = await promisify(execFile)(
      process.execPath,
      [resolve('runner/main.ts'), run],
      { timeout: 15_000 },
    );
    expect(stdout).toContain('job stdout');
    expect(stderr).toContain('job stderr');
    await expect(readRunOutcome(run)).resolves.toEqual({
      version: 1,
      status: 'success',
      records: [
        {
          text: '{"value":"project cwd"}',
          input: { topic: 'Bamboo' },
        },
      ],
    });
  });

  test.for([
    {
      name: 'undefined',
      body: 'return [{ value: undefined }]',
      code: 'JOB_RESULT',
    },
    {
      name: 'cycle',
      body: 'const item = {}; item.self = item; return [item]',
      code: 'JOB_RESULT',
    },
    {
      name: 'infinity',
      body: 'return [{ value: Infinity }]',
      code: 'JOB_RESULT',
    },
    {
      name: 'non-record',
      body: 'return [42]',
      code: 'JOB_RESULT',
    },
    {
      name: 'empty',
      body: 'return []',
      code: null,
    },
  ])(
    'publishes the correct outcome for $name results',
    async ({ name, body, code }, { temporaryDirectory: directory, runtime: { endpoint } }) => {
      await writeFile(
        join(directory, `${name}.ts`),
        `export default async function job() { ${body} }`,
      );
      const run = await createRunDirectory(directory, {
        version: 1,
        projectDirectory: directory,
        entryFile: `${name}.ts`,
        endpoint,
        input: null,
      });
      const execution = promisify(execFile)(process.execPath, [resolve('runner/main.ts'), run], {
        timeout: 15_000,
      });
      if (code) {
        await expect(execution).rejects.toThrow();
        const outcome = await readRunOutcome(run);
        expect(outcome).toMatchObject({
          status: 'failure',
          error: { code },
        });
        expect(outcome).not.toHaveProperty('records');
      } else {
        await execution;
        await expect(readRunOutcome(run)).resolves.toEqual({
          version: 1,
          status: 'success',
          records: [],
        });
      }
    },
  );

  test(
    'returns Wikipedia article records',
    {
      skip: process.env.BAMBOO_WIKIPEDIA_TEST !== '1',
      timeout: 45_000,
    },
    async ({ runtime: { browser } }) => {
      const job = await loadJob(resolve('examples/wikipedia/job.ts'));
      const records = await executeJob(job, browser, { topic: 'Bamboo' });
      expect(records.length).toBeGreaterThan(0);
      expect(records[0]?.title).toBe('Bamboo');
    },
  );
});
