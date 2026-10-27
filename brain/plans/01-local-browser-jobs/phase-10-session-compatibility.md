# 10. Prove installed-browser session transfer

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Prove installed-browser session transfer. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `tools/probes/session-transfer.ts`
- `tools/probes/auth-fixture.ts`
- `tools/probes/session-transfer.test.ts`

Launch the owner-selected installed Helium browser with a dedicated temporary Bamboo profile, manually authenticate, export Playwright storage state and restore it into Lightpanda. Test cookies and local storage separately; probe IndexedDB without promising support. Never attach to the personal default browser profile. Stop profile implementation if the required round trip fails; report findings and revise the requirement with the user.

## Data structures

- SessionCompatibilityReport: tested versions and supported storage mechanisms.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Use a deterministic authenticated fixture to prove import, authenticated access and re-export into a fresh Lightpanda process. Exercise manual login with an actual chosen site before claiming site compatibility.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.

## Progress (2026-10-26)

- The owner uses Helium; capture was tested with Helium 0.20.1.1 / Chromium 156.0.8078.47 rather than installing Chrome.
- Added local cookie/local-storage/IndexedDB fixtures, isolated capture profiles, bounded guarded browser processes, fresh-process restoration and redacted reports.
- The original Lightpanda 1.1.0-nightly.10071+3be033135 dropped session cookies; manual Quotes to Scrape capture succeeded but import failed.
- Upstream fixed `expires: -1` handling in 1.0.1. Verified the official 1.0.2 Apple Silicon binary against its SHA-256, tested it in isolation, then replaced the installed runtime while retaining a backup.
- All four fixture mechanisms now pass. Session cookies remain non-persistent after re-export and fresh-process restoration. The public demo round trip also passes with automated dummy login.
- Removed the unsuccessful alternate-cookie API diagnostic and made the runtime regression test require authentication and session-cookie lifetime preservation.
- Required gate now passes. Next implementation layer: [[plans/01-local-browser-jobs/phase-11-saved-profiles]].
- Full findings and reproduction commands: `tools/probes/session-compatibility.md`.
