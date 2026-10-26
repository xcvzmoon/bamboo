import type { Browser } from 'playwright-core';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright-core';
import * as v from 'valibot';
import { test as baseTest } from '../fixtures.ts';

async function allocatePort(): Promise<number> {
  const server = createServer();
  try {
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    return v.parse(v.object({ port: v.number() }), server.address()).port;
  } finally {
    await new Promise<void>((resolve) => {
      server.close(() => {
        resolve();
      });
    });
  }
}

export const test = baseTest.extend('runtime', async ({ task: _task }, { onCleanup }) => {
  const port = await allocatePort();
  const endpoint = `http://127.0.0.1:${port}`;
  const child = spawn(
    process.env.LIGHTPANDA_EXECUTABLE ?? 'lightpanda',
    ['serve', '--host', '127.0.0.1', '--port', String(port)],
    {
      stdio: 'ignore',
      env: {
        ...process.env,
        LIGHTPANDA_DISABLE_TELEMETRY: 'true',
      },
    },
  );
  let spawnError: Error | undefined;
  child.on('error', (error) => {
    spawnError = error;
  });
  const exited = new Promise<void>((resolve) => {
    child.once('close', () => {
      resolve();
    });
  });
  let browser: Browser | undefined;
  onCleanup(async () => {
    const killTimer = setTimeout(() => {
      child.kill('SIGKILL');
    }, 2000);
    try {
      child.kill('SIGTERM');
      await exited;
      await browser?.close();
    } finally {
      clearTimeout(killTimer);
    }
  });
  const deadline = Date.now() + 10_000;
  /* eslint-disable no-await-in-loop -- Readiness depends on bounded, sequential connection attempts. */
  while (!browser) {
    if (spawnError) {
      throw spawnError;
    }
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error('TEST_BROWSER_EXIT: Lightpanda exited before readiness');
    }
    try {
      browser = await chromium.connectOverCDP(endpoint, { timeout: 500 });
    } catch (error) {
      if (Date.now() >= deadline) {
        throw error;
      }
      await delay(50);
    }
  }
  /* eslint-enable no-await-in-loop */
  return { browser, endpoint };
});
