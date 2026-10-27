import type { Browser, BrowserContext } from 'playwright-core';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import * as v from 'valibot';

export async function probeDeadline<T>(operation: Promise<T>, milliseconds = 20_000): Promise<T> {
  const abort = new AbortController();
  try {
    return await Promise.race([
      operation,
      delay(milliseconds, undefined, {
        signal: abort.signal,
        ref: false,
      }).then(() => {
        throw new Error('SESSION_DEADLINE: Browser operation exceeded its deadline');
      }),
    ]);
  } finally {
    abort.abort();
  }
}

export async function withProbeBrowser<T>(
  executable: string,
  argumentsForPort: (port: number) => string[],
  operation: (browser: Browser) => Promise<T>,
  milliseconds = 30_000,
): Promise<T> {
  const server = createServer();
  let port: number;
  try {
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    port = v.parse(v.object({ port: v.number() }), server.address()).port;
  } finally {
    await new Promise<void>((resolve) => {
      server.close(() => {
        resolve();
      });
    });
  }
  const child = spawn(
    process.execPath,
    [
      fileURLToPath(new URL('../../runner/guard.ts', import.meta.url)),
      executable,
      ...argumentsForPort(port),
    ],
    {
      detached: true,
      stdio: ['pipe', 'ignore', 'ignore'],
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
  child.stdin.on('error', () => undefined);
  const exited = new Promise<void>((resolve) => {
    child.once('close', () => {
      resolve();
    });
  });
  function interrupt(): void {
    child.stdin.end();
  }
  process.once('SIGINT', interrupt);
  process.once('SIGTERM', interrupt);
  try {
    let browser: Browser | undefined;
    const deadline = Date.now() + 10000;
    /* eslint-disable no-await-in-loop -- Readiness attempts are sequential and bounded. */
    while (!browser) {
      if (
        spawnError ||
        child.exitCode !== null ||
        child.signalCode !== null ||
        Date.now() >= deadline
      ) {
        throw new Error('SESSION_STARTUP: Browser did not become ready');
      }
      try {
        browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 500 });
      } catch {
        await delay(50);
      }
    }
    /* eslint-enable no-await-in-loop */
    return await probeDeadline(operation(browser), milliseconds);
  } finally {
    process.removeListener('SIGINT', interrupt);
    process.removeListener('SIGTERM', interrupt);
    child.stdin.end();
    const force = setTimeout(() => {
      if (child.pid !== undefined) {
        try {
          process.kill(-child.pid, 'SIGKILL');
        } catch {
          child.kill('SIGKILL');
        }
      }
    }, 2000);
    try {
      await exited;
    } finally {
      clearTimeout(force);
    }
  }
}

export async function withCaptureContext<T>(
  executable: string,
  headless: boolean,
  operation: (context: BrowserContext) => Promise<T>,
  milliseconds = 30000,
): Promise<T> {
  const profile = await mkdtemp(join(tmpdir(), 'bamboo-session-capture-'));
  try {
    return await withProbeBrowser(
      executable,
      (port) => [
        `--user-data-dir=${profile}`,
        `--remote-debugging-port=${port}`,
        '--remote-debugging-address=127.0.0.1',
        '--no-first-run',
        '--no-default-browser-check',
        ...(headless ? ['--headless=new'] : []),
        'about:blank',
      ],
      async (browser) => {
        const context = browser.contexts()[0];
        if (!context) {
          throw new Error('SESSION_CAPTURE: Missing dedicated browser context');
        }
        context.setDefaultTimeout(8000);
        context.setDefaultNavigationTimeout(8000);
        return await operation(context);
      },
      milliseconds,
    );
  } finally {
    await rm(profile, {
      recursive: true,
      force: true,
    });
  }
}
