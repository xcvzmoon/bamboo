# 9. Queue manual requests globally

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Queue manual requests globally. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/queue.rs`
- `src-tauri/src/runner.rs`
- `src-tauri/src/commands.rs`

Run one job globally; queue manual requests FIFO. Snapshot settings on enqueue, but execute current source at actual start and hash it then. Allow cancelling queued work. App exit cancels pending requests; crashes mark them interrupted. Do not automatically replay pending work after restart.

## Data structures

- QueuedRun: immutable snapshot of job settings and input at enqueue time.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Submit two jobs and prove no overlap and FIFO execution. Cancel the queued job and confirm it never launches. Edit inputs after enqueue and verify the captured input is used.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
