# Session compatibility probe

## Result on 2026-10-26

The required session-transfer gate passes with Lightpanda 1.0.2. Saved login
profile implementation can proceed.

| Mechanism                           | Capture in Helium | Import into Lightpanda | Re-export and fresh Lightpanda process |
| ----------------------------------- | ----------------- | ---------------------- | -------------------------------------- |
| Session cookie (no expiry)          | Pass              | Pass                   | Pass                                   |
| Persistent cookie (one-hour expiry) | Pass              | Pass                   | Pass                                   |
| Local storage                       | Pass              | Pass                   | Pass                                   |
| IndexedDB                           | Pass              | Pass                   | Pass                                   |

Tested on macOS Apple Silicon with Node v26.12.1, Helium 0.20.1.1
(Chromium 156.0.8078.47), Lightpanda 1.0.2, and playwright-core 1.64.1.
Chrome was not installed; the owner selected Helium. IndexedDB passing this
fixture does not establish compatibility with arbitrary sites or schemas.

Session cookies remain session cookies: the probe checks `expires: -1` after
re-export and again in the fresh process. No expiry rewriting or alternate
restoration path is used. The runtime-enabled regression test now requires the
authentication gate to pass instead of merely checking report consistency.

## Runtime fix

The prior installed binary, `1.1.0-nightly.10071+3be033135`, dropped session
cookies on import. [Lightpanda 1.0.1 release notes](https://github.com/lightpanda-io/browser/releases/tag/1.0.1)
identify the upstream fix for retaining cookies with `expires: -1`.

The official [1.0.2 Apple Silicon release](https://github.com/lightpanda-io/browser/releases/tag/1.0.2)
was tested in isolation before replacing the installed executable at
`~/.local/bin/lightpanda`. Its SHA-256 matches the release asset metadata:

```text
9f3ee749a55d1abc5f8fdda0d9559c3b2abcec5bff5bfe902bc06c0a89538ec4
```

The previous binary is preserved beside it as
`lightpanda.backup-20261026T140132559515Z`. Existing runtime configuration keeps
using the same executable path. This was a one-time development environment
update; Bamboo and the probe do not install or download browsers.

The owner previously completed manual login on Quotes to Scrape in a temporary
Helium profile; capture succeeded but the old nightly lost its session cookie.
An automated dummy login reproduced that failure. Repeating the public-site
round trip with 1.0.2 restored the cookie unchanged and stayed authenticated
after re-export into a fresh Lightpanda process. The successful repeat was
explicitly reported as an automated login, not another manual login.

## Reproduce

No browser binaries are downloaded by these commands. Use installed Helium (or
provide an explicit Chromium-compatible executable) and Lightpanda.

```sh
pnpm probe:session /Applications/Helium.app/Contents/MacOS/Helium
```

The command reports each mechanism and exits with status 1 when the required
cookie/local-storage round trip or session-cookie lifetime check fails.
`LIGHTPANDA_EXECUTABLE` can select an installed Lightpanda executable.

Manual login against the public demo site:

```sh
pnpm probe:session --manual /Applications/Helium.app/Contents/MacOS/Helium
```

Enter dummy values in the temporary browser window. Any username/password works
on [Quotes to Scrape](https://toscrape.com/). `--site-fixture` repeats that demo
login automatically for diagnosis; its report identifies that login as automated.
Neither mode attaches to the personal browser profile.

The local fixture requires a real server-side session token and checks an
anonymous context before importing state. Storage mechanisms are isolated in
separate capture profiles. Successful state must survive re-export and a second
fresh Lightpanda process. Reports never include cookie values or session contents.

Owned browser processes use Bamboo's process guard and bounded operations.
Temporary profiles and servers are removed in cleanup. Exported Playwright state
stays in memory; reports contain only versions, stages, counts and boolean outcomes.

## Tests

```sh
BAMBOO_SESSION_TEST=1 pnpm exec vitest run tests/probes/session-transfer.test.ts
```

Tests verify fixture boundaries, missing-executable behavior, authentication
preservation and unchanged session-cookie lifetime. IndexedDB remains an optional
probe rather than a requirement of the authentication gate.

The [Playwright storage-state API](https://playwright.dev/docs/auth) defines the
capture/restore contract. The compatibility result above comes from executing the
probe against the listed binaries, rather than assuming CDP support is complete.
