# 6. Register and start jobs from the app

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Register and start jobs from the app. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/commands.rs`
- `app/components/JobsPanel.vue`
- `app/app.vue`

Expose narrow Rust commands for selecting/registering a project and entry, naming a job, saving JSON inputs and timeout, listing jobs, and starting a manual run. Use a native picker dependency when needed; do not expose a generic frontend shell. Keep SQLite authoritative.

## Data structures

- JobSummary and RegisterJobInput: validated command-boundary data.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Register the Wikipedia example, restart the app, and run the saved job. Invalid JSON, absent entry files, and paths containing spaces receive actionable errors. Follow repository Vue and TypeScript skills.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.

## Progress (2026-10-26)

- Added job listing, registration, saved settings and native project/entry selection commands.
- Added runtime path setup, registration and saved-job controls to the app. Manual runs accept edited JSON input; saved timeout changes require saving settings first.
- Added JSON/timeout boundary tests and a persistence test covering restart and queued snapshot preservation.
- Verification passed: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`, focused web lint and Cargo Clippy. Real Lightpanda/Wikipedia checks passed (14 tests), plus the supervisor fixture using packaged app runner assets.
- Desktop picker interaction and registering/running Wikipedia through the UI still need manual acceptance.
- Next implementation layer: [[plans/01-local-browser-jobs/phase-07-results-export]].
