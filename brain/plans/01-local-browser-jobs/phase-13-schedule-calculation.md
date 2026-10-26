# 13. Calculate simple local schedules

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Calculate simple local schedules. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/schedule.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/src/storage.rs`

Persist schedules and compute the next strictly future occurrence. Skip missed occurrences after quit/sleep; never backfill. Use the current system timezone for daily schedules, next valid instant for nonexistent DST times and one occurrence for repeated local times. Recompute when the system timezone changes.

## Data structures

- Schedule: disabled, interval in minutes/hours, daily local time.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Test intervals, restart after missed runs, daylight-saving gaps/repeats, timezone changes and wall-clock jumps using an injected clock.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
