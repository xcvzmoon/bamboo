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

## Progress (2026-10-26)

- Added a shared Rust FIFO queue with one worker and immutable persisted job/input snapshots.
- Manual requests return immediately. The app shows active and waiting runs, supports cancellation of either, and refreshes history as queue state changes.
- Source and entry hashes use the file present at actual start. Saved setting changes after admission do not alter waiting runs.
- App exit closes admission and cancels pending requests before shutting down active work. Fresh queues never replay prior requests; startup recovery marks unfinished rows interrupted.
- Added process tests for FIFO/no overlap, input/settings snapshots, current-source hashing, cancelled requests never launching, and shutdown of active/pending work.
- Verification passed: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`, focused web lint and Cargo Clippy. Runtime-enabled verification passed 33 TypeScript and 27 Rust tests. Build succeeded after rerunning resource-producing steps sequentially.
- Desktop interaction with multiple queued requests still needs manual acceptance.
- Next implementation layer: [[plans/01-local-browser-jobs/phase-10-session-compatibility]].
