# 10. Prove installed-Chrome session transfer

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Prove installed-Chrome session transfer. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `tools/probes/session-transfer.ts`
- `tools/probes/auth-fixture.ts`
- `tools/probes/session-transfer.test.ts`

Launch installed Chrome with a dedicated temporary Bamboo profile, manually authenticate, export Playwright storage state and restore it into Lightpanda. Test cookies and local storage separately; probe IndexedDB without promising support. Never attach to the personal default Chrome profile. Stop profile implementation if the required round trip fails; report findings and revise the requirement with the user.

## Data structures

- SessionCompatibilityReport: tested versions and supported storage mechanisms.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Use a deterministic authenticated fixture to prove import, authenticated access and re-export into a fresh Lightpanda process. Exercise manual login with an actual chosen site before claiming site compatibility.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
