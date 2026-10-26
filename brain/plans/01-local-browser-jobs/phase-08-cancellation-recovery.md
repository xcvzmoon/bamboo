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
