# 5. Launch and finish one managed run

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Launch and finish one managed run. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/runner.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml`

Launch a fresh Lightpanda per run, wait for readiness, launch the Node runner in the job project, capture logs and validate the outcome. Package runner assets with the app, not Node or browsers. Use argv execution without a shell. Make the supervisor the only owner of run state.

## Data structures

- RunSupervisor and ActiveRun: single global ownership of browser and Node processes.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: A real fixture run saves records and logs. Startup failures and unexpected child exit mark failure and clean up owned children. Test a packaged app launched from Finder, including executable path configuration.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
