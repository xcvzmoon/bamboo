# 15. Finish schedule and history management

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Finish schedule and history management. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `app/components/JobsPanel.vue`
- `app/components/RunDetails.vue`
- `src-tauri/src/commands.rs`

Expose schedule editing, next-run time, queued/running status and profile selection. Retain logs/results/metadata until explicit deletion. Delete a run or all history for a job, rejecting deletion of active work and preserving job settings and profiles. Handle partially failed filesystem deletion clearly.

## Data structures

- ScheduleForm and DeleteRunResult: scheduling settings and history deletion.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Exercise schedule editing, restart persistence, JSON input overrides for manual runs, profile selection and single/all-history deletion. Verify window-close exit and the complete authenticated scheduled export flow.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
