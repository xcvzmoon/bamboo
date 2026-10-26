import { writeFile, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from 'vitest';
import { probeRuntime } from '../../tools/probes/lightpanda.ts';
import { test } from '../fixtures.ts';

test('missing executable has a stable error code', async () => {
  await expect(probeRuntime('/bamboo-missing-lightpanda')).rejects.toThrow(/PROBE_EXECUTABLE/);
});

test('early process exit has a stable error code', async () => {
  await expect(probeRuntime(process.execPath)).rejects.toThrow(/PROBE_STARTUP:/);
});

test(
  'stalled startup is bounded and SIGTERM-resistant process is killed',
  { timeout: 15_000 },
  async ({ temporaryDirectory: directory }) => {
    const executable = join(directory, 'stalled-runtime');
    const pidFile = join(directory, 'pid');

    await writeFile(
      executable,
      `#!/bin/sh\ntrap '' TERM\necho $$ > '${pidFile}'\nexec sleep 60\n`,
      {
        mode: 0o700,
      },
    );

    await expect(probeRuntime(executable)).rejects.toThrow(/PROBE_STARTUP_TIMEOUT/);

    const pid = Number(await readFile(pidFile, 'utf8'));

    expect(() => {
      process.kill(pid, 0);
    }).toThrow(expect.objectContaining({ code: 'ESRCH' }));
  },
);

test('invalid navigation URL fails before launching a process', async () => {
  await expect(probeRuntime('/bamboo-missing-lightpanda', 'file:///etc/passwd')).rejects.toThrow();
});

test(
  'real Lightpanda navigation and cleanup',
  { skip: process.env.BAMBOO_RUNTIME_TEST !== '1' },
  async () => {
    const result = await probeRuntime();
    expect(result.title).toBe('Bamboo probe');
    expect(result.heading).toBe('Runtime connected');
    expect(result.contextClosed).toBe(true);
    expect(result.processTerminated).toBe(true);
  },
);
