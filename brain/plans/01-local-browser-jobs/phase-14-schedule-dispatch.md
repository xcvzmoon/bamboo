# 14. Dispatch schedules while the app runs

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Dispatch schedules while the app runs. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/scheduler.rs`
- `src-tauri/src/queue.rs`
- `src-tauri/src/lib.rs`

Use the Rust supervisor for atomic global admission. Scheduled occurrences are skipped whenever any run is active or manual work is queued. Manual requests take precedence. A timer delayed by sleep advances to the next future occurrence without dispatching missed work. No daemon, tray service, login item or OS wake scheduling.

## Data structures

- ScheduleDispatchDecision: launch or skip-busy.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Use a controlled clock to prove busy skipping, manual priority, wake skipping and no duplicate dispatch. Quit the app and confirm no schedules execute.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
