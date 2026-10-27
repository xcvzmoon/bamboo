import { spawn } from 'node:child_process';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import * as v from 'valibot';

const [executable, directory, address] = v.parse(
  v.tuple([v.string(), v.string(), v.pipe(v.string(), v.url())]),
  process.argv.slice(2),
);
const lines = createInterface({ input: process.stdin });
const instruction = new Promise<string>((resolve) => {
  lines.once('line', resolve);
  lines.once('close', () => {
    resolve('cancel');
  });
});
const child = spawn(
  process.execPath,
  [
    fileURLToPath(new URL('./guard.ts', import.meta.url)),
    executable,
    `--user-data-dir=${join(directory, 'browser')}`,
    '--remote-debugging-port=0',
    '--remote-debugging-address=127.0.0.1',
    '--no-first-run',
    '--no-default-browser-check',
    ...(process.env.BAMBOO_CAPTURE_HEADLESS === '1' ? ['--headless=new'] : []),
    'about:blank',
  ],
  {
    detached: true,
    stdio: ['pipe', 'ignore', 'ignore'],
  },
);
child.stdin.on('error', () => undefined);
const exited = new Promise<void>((resolve) => {
  child.once('close', () => {
    resolve();
  });
  child.once('error', () => {
    resolve();
  });
});
const timeout = AbortSignal.timeout(600_000);
const ownerClosed = new Promise<never>((_resolve, reject) => {
  lines.once('close', () => {
    reject(new Error('PROFILE_CANCELLED'));
  });
  timeout.addEventListener('abort', () => {
    reject(new Error('PROFILE_TIMEOUT'));
  });
});
async function capture(): Promise<void> {
  let port: number | undefined;
  /* eslint-disable no-await-in-loop -- Wait for this browser's private CDP endpoint. */
  for (let attempt = 0; attempt < 150; attempt += 1) {
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error('PROFILE_BROWSER');
    }
    try {
      const content = await readFile(join(directory, 'browser', 'DevToolsActivePort'), 'utf8');
      port = v.parse(
        v.pipe(v.string(), v.transform(Number), v.integer(), v.minValue(1)),
        content.split('\n')[0],
      );
      break;
    } catch {
      await delay(50);
    }
  }
  /* eslint-enable no-await-in-loop */
  if (port === undefined) {
    throw new Error('PROFILE_BROWSER');
  }
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 5000 });
  const context = browser.contexts()[0];
  if (!context) {
    throw new Error('PROFILE_BROWSER');
  }
  const page = context.pages()[0] ?? (await context.newPage());
  await page.goto(address, {
    timeout: 20_000,
    waitUntil: 'domcontentloaded',
  });
  await writeFile(join(directory, 'ready'), '', {
    mode: 0o600,
    flag: 'wx',
  });
  const disconnected = new Promise<never>((_resolve, reject) => {
    browser.once('disconnected', () => {
      reject(new Error('PROFILE_BROWSER'));
    });
  });
  if ((await Promise.race([instruction, disconnected])) !== 'capture') {
    return;
  }
  const state = await context.storageState({ indexedDB: true });
  await writeFile(join(directory, 'state.json'), JSON.stringify(state), {
    mode: 0o600,
    flag: 'wx',
  });
}
try {
  await Promise.race([capture(), ownerClosed]);
} catch {
  process.exitCode = 1;
} finally {
  lines.close();
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
  await exited;
  clearTimeout(force);
  await rm(join(directory, 'browser'), {
    recursive: true,
    force: true,
  });
  if (process.exitCode === 1) {
    await rm(join(directory, 'state.json'), { force: true });
  }
}
