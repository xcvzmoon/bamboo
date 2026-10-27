import { expect, test } from 'vitest';
import { startAuthFixture } from '../../tools/probes/auth-fixture.ts';
import { probeSessionTransfer } from '../../tools/probes/session-transfer.ts';

test('authentication fixture denies anonymous requests and isolates storage mechanisms', async () => {
  const fixture = await startAuthFixture();
  try {
    const denied = await fetch(`${fixture.origin}/check?mechanism=cookie`);
    expect(denied.status).toBe(401);
    const login = await fetch(`${fixture.origin}/issue?mechanism=cookie`, { method: 'POST' });
    const cookie = login.headers.get('set-cookie')?.split(';')[0];
    expect(cookie).toBeDefined();
    const authenticated = await fetch(`${fixture.origin}/check?mechanism=cookie`, {
      headers: { Cookie: cookie ?? '' },
    });
    expect(authenticated.status).toBe(200);
    const isolated = await fetch(`${fixture.origin}/check?mechanism=localStorage`, {
      headers: { Cookie: cookie ?? '' },
    });
    expect(isolated.status).toBe(401);
    expect((await fetch(`${fixture.origin}/check?mechanism=invalid`)).status).toBe(400);
    expect((await fetch(`${fixture.origin}/issue?mechanism=cookie`)).status).toBe(404);
  } finally {
    await fixture.close();
  }
});

test('missing capture browser fails without using a personal or downloaded browser', async () => {
  await expect(probeSessionTransfer('/missing/bamboo-browser')).rejects.toThrow();
});

test(
  'installed browser session transfer preserves authentication and session-cookie lifetime',
  {
    skip: process.env.BAMBOO_SESSION_TEST !== '1',
    timeout: 120_000,
  },
  async () => {
    const report = await probeSessionTransfer(
      process.env.BAMBOO_CAPTURE_BROWSER ?? '/Applications/Helium.app/Contents/MacOS/Helium',
      process.env.LIGHTPANDA_EXECUTABLE,
    );
    expect(report.mechanisms.map(({ mechanism }) => mechanism)).toEqual([
      'cookie',
      'persistentCookie',
      'localStorage',
      'indexedDB',
    ]);
    expect(report.mechanisms.every(({ captured }) => captured)).toBe(true);
    expect(report.requiredRoundTrip).toBe(true);
    const sessionCookie = report.mechanisms.find(({ mechanism }) => mechanism === 'cookie');
    expect(sessionCookie?.sessionCookiePreserved).toBe(true);
    expect(report.siteVerified).toBe(false);
  },
);
