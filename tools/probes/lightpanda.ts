import type { Browser } from 'playwright-core';
import { execFile, spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';
import { chromium } from 'playwright-core';
import * as v from 'valibot';

const configSchema = v.object({
  executable: v.optional(v.pipe(v.string(), v.nonEmpty()), 'lightpanda'),
  url: v.optional(v.pipe(v.string(), v.url(), v.startsWith('https://'))),
});

export type RuntimeProbeResult = {
  node: string;
  lightpanda: string;
  playwright: string;
  platform: string;
  architecture: string;
  endpoint: string;
  title: string;
  heading: string;
  contextClosed: boolean;
  processTerminated: boolean;
};

async function allocatePort(): Promise<number> {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  await new Promise<void>((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
      } else {
        resolve();
      }
    });
  });
  if (!address || !v.is(v.object({ port: v.number() }), address)) {
    throw new Error('PROBE_PORT: Could not allocate a loopback port');
  }
  return address.port;
}

export async function probeRuntime(
  executable = 'lightpanda',
  url?: string,
): Promise<RuntimeProbeResult> {
  const config = v.parse(configSchema, { executable, url });
  const port = await allocatePort();
  const endpoint = `http://127.0.0.1:${port}`;
  const child = spawn(config.executable, ['serve', '--host', '127.0.0.1', '--port', String(port)], {
    stdio: ['ignore', 'ignore', 'pipe'],
    env: {
      ...process.env,
      LIGHTPANDA_DISABLE_TELEMETRY: 'true',
    },
  });
  let spawnError: Error | undefined;
  let diagnostics = '';
  child.on('error', (error) => {
    spawnError = error;
  });
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', (chunk: string) => {
    diagnostics = (diagnostics + chunk).slice(-4096);
  });
  const exited = new Promise<void>((resolve) => {
    child.once('close', () => {
      resolve();
    });
  });
  let browser: Browser | undefined;
  const fixture = createServer((_request, response) => {
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(
      '<!doctype html><title>Bamboo probe</title><h1 id="result">Loading</h1><script>document.querySelector("#result").textContent = "Runtime connected"</script>',
    );
  });
  const abort = new AbortController();
  function interrupt(): void {
    abort.abort();
    child.kill('SIGTERM');
  }
  const runTimer = setTimeout(() => {
    abort.abort();
    child.kill('SIGKILL');
  }, 35_000);
  process.once('SIGINT', interrupt);
  process.once('SIGTERM', interrupt);
  let result: Omit<RuntimeProbeResult, 'processTerminated'>;
  try {
    const deadline = Date.now() + 10_000;
    /* eslint-disable no-await-in-loop -- Readiness attempts must be sequential and bounded. */
    while (true) {
      abort.signal.throwIfAborted();
      if (spawnError) {
        throw new Error('PROBE_EXECUTABLE: Lightpanda could not be started', { cause: spawnError });
      }
      if (child.exitCode !== null || child.signalCode !== null) {
        throw new Error(`PROBE_STARTUP: Lightpanda exited before readiness: ${diagnostics}`);
      }
      try {
        const response = await fetch(`${endpoint}/json/version`, {
          signal: AbortSignal.timeout(300),
        });
        if (response.ok) {
          break;
        }
      } catch {
        abort.signal.throwIfAborted();
      }
      if (Date.now() >= deadline) {
        throw new Error(
          `PROBE_STARTUP_TIMEOUT: Lightpanda was not ready within 10 seconds: ${diagnostics}`,
        );
      }
      await delay(50, undefined, { signal: abort.signal });
    }
    /* eslint-enable no-await-in-loop */
    browser = await chromium.connectOverCDP(endpoint, { timeout: 5000 });
    const version = await promisify(execFile)(config.executable, ['version'], {
      timeout: 3000,
      killSignal: 'SIGKILL',
      maxBuffer: 4096,
    });
    const context = await browser.newContext();
    let contextClosed = false;
    context.once('close', () => {
      contextClosed = true;
    });
    let title: string;
    let heading: string;
    try {
      fixture.listen(0, '127.0.0.1');
      await once(fixture, 'listening');
      const address = fixture.address();
      if (!address || !v.is(v.object({ port: v.number() }), address)) {
        throw new Error('PROBE_FIXTURE: Local fixture did not bind');
      }
      const page = await context.newPage();
      const response = await page.goto(config.url ?? `http://127.0.0.1:${address.port}`, {
        timeout: 15_000,
        waitUntil: 'domcontentloaded',
      });
      if (!response?.ok()) {
        throw new Error(`PROBE_NAVIGATION: Unexpected HTTP status ${response?.status()}`);
      }
      title = await page.title();
      heading = await page.locator('h1').first().innerText({ timeout: 5000 });
      if (!config.url && (title !== 'Bamboo probe' || heading !== 'Runtime connected')) {
        throw new Error('PROBE_EXTRACTION: Fixture JavaScript or extraction failed');
      }
    } finally {
      await context.close();
    }
    abort.signal.throwIfAborted();
    const require = createRequire(import.meta.url);
    const playwright = v.parse(
      v.object({ version: v.string() }),
      require('playwright-core/package.json'),
    ).version;
    result = {
      node: process.version,
      lightpanda: version.stdout.trim(),
      playwright,
      platform: process.platform,
      architecture: process.arch,
      endpoint,
      title,
      heading,
      contextClosed,
    };
  } finally {
    clearTimeout(runTimer);
    process.removeListener('SIGINT', interrupt);
    process.removeListener('SIGTERM', interrupt);
    const killTimer = setTimeout(() => {
      child.kill('SIGKILL');
    }, 2000);
    try {
      child.kill('SIGTERM');
      await exited;
      await browser?.close();
    } finally {
      clearTimeout(killTimer);
      fixture.closeAllConnections();
      await new Promise<void>((resolve) => {
        fixture.close(() => {
          resolve();
        });
      });
    }
  }
  return {
    ...result,
    processTerminated: child.exitCode !== null || child.signalCode !== null,
  };
}

if (import.meta.main) {
  try {
    const result = await probeRuntime(process.env.LIGHTPANDA_EXECUTABLE, process.argv[2]);
    console.info(JSON.stringify(result, null, 2));
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  }
}
