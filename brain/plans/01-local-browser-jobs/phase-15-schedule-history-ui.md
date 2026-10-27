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

## Progress

- Implemented on 2026-10-27. Each saved job has schedule controls for disabled, minute/hour intervals, and daily local time, with a live next-occurrence display. Polling preserves unsaved form edits. Schedules use saved input/profile settings; manual runs keep editable JSON overrides.
- Single-run and whole-job history deletion require confirmation in the app. Queue admission is locked during deletion, including the finalization window after success metadata is written. Queued/running work rejects the entire deletion request before any files are removed.
- Pending session publication is completed before removing terminal run metadata, so history deletion does not orphan profile recovery journals. Jobs, settings, schedules, profiles, and already exported copies are preserved.
- Filesystem or per-run database failures retain that run's metadata and report stable failure codes, alongside IDs deleted successfully. Missing artifacts can be retried from the delete controls even when run details cannot load. A commit failure explicitly warns that files may already be gone and deletion should be retried. Symlinked run directories are rejected.
- Behavioral checks cover malformed schedule forms, active/finalizing work, mixed deletion success/failure, retry after database and commit failures, symlinks, and pending-profile recovery. The opt-in acceptance test captures a synthetic login through installed Helium, uses a native Keychain key, runs an authenticated Lightpanda job on schedule, verifies manual input overrides, restarts storage, exports JSON/CSV, and deletes single/all history while preserving settings/profiles and external exports.
- The HTTP acceptance fixture reads complete request headers and changes login cookies only on the protected page. No personal browser profile or real account is used.
- Acceptance reproduction after packaging: `BAMBOO_ACCEPTANCE_TEST=1 BAMBOO_CAPTURE_HEADLESS=1 BAMBOO_NODE_EXECUTABLE=/absolute/path/to/node BAMBOO_LIGHTPANDA_EXECUTABLE=/absolute/path/to/lightpanda cargo test --manifest-path src-tauri/Cargo.toml --lib authenticated_helium_profile`.
- Window close uses the app-wide scheduler/queue shutdown path tested in phase 14; the acceptance test also stops dispatch before reopening saved data.
- Verification passed in order: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`. The test run enabled `BAMBOO_ACCEPTANCE_TEST=1` with the installed Node/Lightpanda executables and isolated headless Helium. Other unrelated opt-in runtime tests stayed disabled. Focused Oxlint and all-target Clippy passed. The macOS app and DMG built with the supported noninteractive packaging mode recorded in the verification guide.
- The backlog item is complete and archived after all 15 layers.
