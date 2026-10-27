import type { Browser, BrowserContext } from 'playwright-core';
import type { StorageMechanism } from './auth-fixture.ts';
import { execFile } from 'node:child_process';
import { constants } from 'node:fs';
import { access } from 'node:fs/promises';
import { promisify } from 'node:util';
import * as v from 'valibot';
import { startAuthFixture } from './auth-fixture.ts';
import { probeDeadline, withCaptureContext, withProbeBrowser } from './browser-process.ts';

const configurationSchema = v.object({
  executable: v.pipe(v.string(), v.nonEmpty(), v.startsWith('/')),
  lightpanda: v.pipe(v.string(), v.nonEmpty()),
});
export type MechanismReport = {
  mechanism: StorageMechanism;
  captured: boolean;
  imported: boolean;
  reexported: boolean;
  freshProcess: boolean;
  sessionCookiePreserved: boolean | null;
  error: string | null;
};
export type SessionCompatibilityReport = {
  node: string;
  captureBrowser: string;
  captureVersion: string;
  lightpanda: string;
  playwright: string;
  platform: string;
  architecture: string;
  mechanisms: MechanismReport[];
  requiredRoundTrip: boolean;
  siteVerified: false;
};

async function assertCaptureExecutable(executable: string): Promise<void> {
  try {
    await access(executable, constants.X_OK);
  } catch {
    throw new Error('SESSION_CAPTURE_EXECUTABLE: Capture browser is not executable');
  }
}

async function withLightpanda<T>(
  executable: string,
  operation: (browser: Browser) => Promise<T>,
): Promise<T> {
  return await withProbeBrowser(
    executable,
    (port) => ['serve', '--host', '127.0.0.1', '--port', String(port)],
    operation,
  );
}

async function authenticated(context: BrowserContext, url: string): Promise<boolean> {
  const page = await context.newPage();
  try {
    await page.goto(url, {
      timeout: 8000,
      waitUntil: 'domcontentloaded',
    });
    await page.waitForFunction(
      () => document.querySelector('h1')?.textContent !== 'Checking session',
      undefined,
      { timeout: 5000 },
    );
    return (await page.locator('h1').innerText({ timeout: 5000 })) === 'Authenticated';
  } finally {
    await probeDeadline(page.close(), 2000);
  }
}

export async function probeSessionTransfer(
  executable: string,
  lightpanda = 'lightpanda',
  progress: (stage: string) => void = () => undefined,
): Promise<SessionCompatibilityReport> {
  const config = v.parse(configurationSchema, { executable, lightpanda });
  await assertCaptureExecutable(config.executable);
  const fixture = await startAuthFixture();
  let captureVersion = 'unavailable';
  try {
    const mechanisms: MechanismReport[] = [];
    /* eslint-disable no-await-in-loop -- Each isolated storage mechanism must complete before resetting capture state. */
    for (const mechanism of ['cookie', 'persistentCookie', 'localStorage', 'indexedDB'] as const) {
      const report: MechanismReport = {
        mechanism,
        captured: false,
        imported: false,
        reexported: false,
        freshProcess: false,
        sessionCookiePreserved: null,
        error: null,
      };
      progress(`${mechanism}:capture`);
      let stage = 'CAPTURE';
      try {
        await withCaptureContext(
          config.executable,
          true,
          async (context) => {
            captureVersion = context.browser()?.version() ?? 'unavailable';
            const capture = await context.newPage();
            try {
              progress(`${mechanism}:clear`);
              await context.clearCookies();
              progress(`${mechanism}:navigate`);
              await capture.goto(`${fixture.origin}/login?mechanism=${mechanism}`);
              await capture.evaluate(() => {
                localStorage.clear();
              });
              progress(`${mechanism}:login`);
              await capture.locator('#login').click();
              await capture.waitForURL(`**/protected?mechanism=${mechanism}`, { timeout: 8000 });
              await capture.waitForFunction(
                () => document.querySelector('h1')?.textContent === 'Authenticated',
                undefined,
                { timeout: 5000 },
              );
              progress(`${mechanism}:snapshot`);
              const state = await context.storageState({ indexedDB: mechanism === 'indexedDB' });
              report.captured = true;
              const protectedUrl = `${fixture.origin}/protected?mechanism=${mechanism}`;
              stage = 'IMPORT';
              progress(`${mechanism}:import`);
              const next = await withLightpanda(config.lightpanda, async (browser) => {
                const anonymous = await browser.newContext();
                try {
                  if (await authenticated(anonymous, protectedUrl)) {
                    throw new Error('SESSION_FIXTURE: Anonymous context was authenticated');
                  }
                } finally {
                  await probeDeadline(anonymous.close(), 2000);
                }
                const restored = await browser.newContext({ storageState: state });
                try {
                  report.imported = await authenticated(restored, protectedUrl);
                  if (!report.imported) {
                    report.error = 'SESSION_AUTHENTICATION_LOST';
                    return null;
                  }
                  stage = 'REEXPORT';
                  const exported = await restored.storageState({
                    indexedDB: mechanism === 'indexedDB',
                  });
                  report.reexported = true;
                  if (mechanism === 'cookie') {
                    report.sessionCookiePreserved = exported.cookies.some(
                      (cookie) => cookie.name === 'bamboo_fixture' && cookie.expires === -1,
                    );
                  }
                  return exported;
                } finally {
                  await probeDeadline(restored.close(), 2000);
                }
              });
              if (next !== null) {
                stage = 'FRESH_PROCESS';
                progress(`${mechanism}:fresh-process`);
                report.freshProcess = await withLightpanda(config.lightpanda, async (browser) => {
                  const restored = await browser.newContext({ storageState: next });
                  try {
                    const authenticatedSession = await authenticated(restored, protectedUrl);
                    if (mechanism === 'cookie') {
                      const cookies = await restored.cookies(protectedUrl);
                      report.sessionCookiePreserved =
                        report.sessionCookiePreserved === true &&
                        cookies.some(
                          (cookie) => cookie.name === 'bamboo_fixture' && cookie.expires === -1,
                        );
                    }
                    return authenticatedSession;
                  } finally {
                    await probeDeadline(restored.close(), 2000);
                  }
                });
              }
            } finally {
              await probeDeadline(capture.close(), 2000);
            }
          },
          90000,
        );
      } catch {
        report.error = `SESSION_${stage}_FAILED`;
      }
      progress(`${mechanism}:complete:${report.error ?? String(report.freshProcess)}`);
      mechanisms.push(report);
    }
    /* eslint-enable no-await-in-loop */
    const { stdout } = await promisify(execFile)(config.lightpanda, ['version'], {
      timeout: 5000,
    });
    const playwright = await import('playwright-core/package.json', { with: { type: 'json' } });
    return {
      node: process.version,
      captureBrowser: config.executable,
      captureVersion,
      lightpanda: stdout.trim(),
      playwright: v.parse(v.object({ version: v.string() }), playwright.default).version,
      platform: process.platform,
      architecture: process.arch,
      mechanisms,
      requiredRoundTrip: mechanisms.every(
        (entry) =>
          entry.mechanism === 'indexedDB' ||
          (entry.freshProcess &&
            (entry.mechanism !== 'cookie' || entry.sessionCookiePreserved === true)),
      ),
      siteVerified: false,
    };
  } finally {
    await fixture.close();
  }
}

export type SiteCompatibilityReport = {
  capturedCookies: number;
  restoredCookies: number;
  cookiesPreserved: boolean;
  manualLogin: boolean;
  error: string | null;
  httpStatus: number | null;
  site: string;
  captureVersion: string;
  captured: boolean;
  imported: boolean;
  reexported: boolean;
  freshProcess: boolean;
};

export async function probeManualQuotes(
  executable: string,
  lightpanda = 'lightpanda',
  manualLogin = true,
): Promise<SiteCompatibilityReport> {
  const config = v.parse(configurationSchema, { executable, lightpanda });
  await assertCaptureExecutable(config.executable);
  let stage = 'CAPTURE';
  const report: SiteCompatibilityReport = {
    capturedCookies: 0,
    restoredCookies: 0,
    cookiesPreserved: false,
    manualLogin,
    error: null,
    httpStatus: null,
    site: 'https://quotes.toscrape.com/',
    captureVersion: 'unavailable',
    captured: false,
    imported: false,
    reexported: false,
    freshProcess: false,
  };
  try {
    return await withCaptureContext(
      config.executable,
      !manualLogin,
      async (context) => {
        const page = await context.newPage();
        await page.goto('https://quotes.toscrape.com/login', { waitUntil: 'domcontentloaded' });
        if (manualLogin) {
          console.info(
            'Complete the dummy login in the temporary Helium window. Any username/password works. Waiting up to ten minutes.',
          );
          await page.bringToFront();
        } else {
          await page.locator('#username').fill('bamboo');
          await page.locator('#password').fill('bamboo');
          await page.locator('input[type="submit"]').click();
        }
        await page.locator('a[href="/logout"]').waitFor({ timeout: manualLogin ? 600000 : 8000 });
        report.captureVersion = context.browser()?.version() ?? 'unavailable';
        report.captured = true;
        console.info('site:capture-complete');
        stage = 'IMPORT';
        const state = await context.storageState({ indexedDB: true });
        report.capturedCookies = state.cookies.length;
        console.info('site:import');
        const next = await withLightpanda(config.lightpanda, async (browser) => {
          const anonymous = await browser.newContext();
          try {
            const anonymousPage = await anonymous.newPage();
            await anonymousPage.goto(report.site, {
              timeout: 8000,
              waitUntil: 'domcontentloaded',
            });
            if ((await anonymousPage.locator('a[href="/logout"]').count()) !== 0) {
              throw new Error('SESSION_SITE_BASELINE: Anonymous context was authenticated');
            }
          } finally {
            await probeDeadline(anonymous.close(), 2000);
          }
          const restored = await browser.newContext({ storageState: state });
          try {
            const cookies = await restored.cookies(report.site);
            report.restoredCookies = cookies.length;
            report.cookiesPreserved = state.cookies.every((cookie) =>
              cookies.some((entry) => entry.name === cookie.name && entry.value === cookie.value),
            );
            const restoredPage = await restored.newPage();
            const response = await restoredPage.goto(report.site, {
              timeout: 8000,
              waitUntil: 'domcontentloaded',
            });
            report.httpStatus = response?.status() ?? null;
            await restoredPage.locator('a[href="/logout"]').waitFor({ timeout: 8000 });
            report.imported = true;
            stage = 'REEXPORT';
            console.info('site:reexport');
            const exported = await restored.storageState({ indexedDB: true });
            report.reexported = true;
            return exported;
          } finally {
            await probeDeadline(restored.close(), 2000);
          }
        });
        stage = 'FRESH_PROCESS';
        console.info('site:fresh-process');
        report.freshProcess = await withLightpanda(config.lightpanda, async (browser) => {
          const restored = await browser.newContext({ storageState: next });
          try {
            const restoredPage = await restored.newPage();
            const response = await restoredPage.goto(report.site, {
              timeout: 8000,
              waitUntil: 'domcontentloaded',
            });
            report.httpStatus = response?.status() ?? null;
            await restoredPage.locator('a[href="/logout"]').waitFor({ timeout: 8000 });
            return true;
          } finally {
            await probeDeadline(restored.close(), 2000);
          }
        });
        return report;
      },
      manualLogin ? 660000 : 90000,
    );
  } catch {
    report.error = `SESSION_SITE_${stage}_FAILED`;
    return report;
  }
}

if (import.meta.main) {
  try {
    if (process.argv[2] === '--manual' || process.argv[2] === '--site-fixture') {
      const report = await probeManualQuotes(
        process.argv[3] ?? '/Applications/Helium.app/Contents/MacOS/Helium',
        process.env.LIGHTPANDA_EXECUTABLE,
        process.argv[2] === '--manual',
      );
      console.info(JSON.stringify(report, null, 2));
      if (!report.freshProcess) {
        process.exitCode = 1;
      }
    } else {
      const report = await probeSessionTransfer(
        process.argv[2] ?? '/Applications/Helium.app/Contents/MacOS/Helium',
        process.env.LIGHTPANDA_EXECUTABLE,
        (stage) => {
          console.error(stage);
        },
      );
      console.info(JSON.stringify(report, null, 2));
      if (!report.requiredRoundTrip) {
        process.exitCode = 1;
      }
    }
  } catch {
    console.error('SESSION_PROBE_FAILED: Could not complete session compatibility probe');
    process.exitCode = 1;
  }
}
