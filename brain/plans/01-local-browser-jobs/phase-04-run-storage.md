# 4. Persist jobs and run history

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Persist jobs and run history. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/storage.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/src/lib.rs`

Use SQLite through a maintained Rust library for job/run metadata and app-data files for logs/results. Jobs store absolute project and entry paths, JSON input and timeout. Record timestamps, entry-file hash and runtime versions. Initialize a new schema, without legacy compatibility paths.

## Data structures

- Job, Run, RunStatus: queued, running, succeeded, failed, cancelled, interrupted.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Round-trip a job and completed run in a temporary database; verify missing files and filesystem/database errors surface with stable codes.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
