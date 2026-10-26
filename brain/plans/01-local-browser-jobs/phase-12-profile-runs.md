# 12. Apply profiles and signal expired authentication

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Apply profiles and signal expired authentication. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `runner/job.ts`
- `runner/main.ts`
- `src-tauri/src/runner.rs`

Allow an optional profile per job. Load state into the managed context and commit updated state atomically only after successful result validation and successful run publication. Failed/cancelled runs preserve prior saved state. Explicit authentication-required errors mark the profile as needing refresh. Do not infer expiry from generic navigation failures or claim rollback of server-side effects.

## Data structures

- AuthenticationRequiredError and ProfileRunOutcome: explicit script signal and candidate updated state.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Successful cookie rotation persists; ordinary failure and cancellation preserve saved state. Explicit auth-required failure marks refresh needed and publishes no dataset. Session and dataset publication have a tested crash-recovery strategy.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
