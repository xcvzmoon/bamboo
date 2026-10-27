# 8. Bound execution and recover after interruption

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Bound execution and recover after interruption. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/runner.rs`
- `src-tauri/src/storage.rs`
- `app/components/RunDetails.vue`

Enforce configurable timeout and cancellation, terminate the managed process groups with bounded graceful cleanup then force termination. Closing the main window quits and cancels active work. On restart mark unfinished runs interrupted; do not resume them. Hide unpublished partial output. Arbitrary trusted code is not sandboxed, so detached children and external side effects cannot be rolled back.

## Data structures

- RunTerminationReason: timeout, user cancellation, app exit, crash recovery.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Cancel an active fixture, trigger timeout, quit during execution and restart after forced app termination. Confirm terminal state, no owned children, and no published failed dataset.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.

## Progress (2026-10-26)

- Added cancellation by active run ID and live run details with a Cancel run control.
- App exit cancels owned work; timeout, user cancellation, app exit and restart interruption have distinct stable codes.
- Added a packaged Node guard that kills owned process groups when the Rust owner pipe closes, including forced termination.
- Startup recovery interrupts unfinished rows and removes abandoned protocol directories and unpublished results without resuming work.
- Tests cover cancellation after a candidate success outcome, stale/duplicate cancellation, owned-child cleanup on pipe loss, timeout, shutdown, and restart recovery.
- Native window-close and forced application termination still need desktop acceptance; automated tests exercise supervisor shutdown and owner-pipe loss.
- Verification passed: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`, focused web lint and Cargo Clippy. Runtime-enabled verification passed 33 TypeScript and 25 Rust tests. A temporary DMG unmount failure was resolved by releasing the generated image and rerunning the build.
- Next implementation layer: [[plans/01-local-browser-jobs/phase-09-manual-queue]].
