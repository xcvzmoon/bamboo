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

## Progress

- Implemented on 2026-10-27. An app-owned Rust task polls schedules once per second. The task starts with the app and is stopped before queue shutdown when the window closes. It installs no OS service or wake scheduling.
- Scheduled admission shares the manual queue mutex and supervisor reservation. Active work, queued manual work, or a waiting manual admission skips the occurrence. Accepted scheduled work takes the active slot immediately and uses saved job input/settings; it never builds a scheduled backlog.
- The schedule revision and consumed-through watermark are checked and updated in the same SQLite transaction that creates a scheduled run. A publication failure rolls back both; consumed occurrences cannot launch again after restart.
- Deadlines initialize strictly in the future. A tick gap over two seconds, a significant wall/monotonic clock mismatch, or a clock rewind rebases deadlines and advances the durable watermark past missed work. Normal dispatch requires a due time less than one second old. Timezone changes rebase daily schedules while intervals keep elapsed-time behavior. Clock errors invalidate deadlines; recovery starts from future occurrences.
- The saved `last_occurrence` field acts as a consumed-through watermark. Startup and wake observations advance it past skipped time, as well as recording launched or busy-skipped occurrences. This prevents a later clock rewind from replaying skipped slots.
- Eight new behavioral checks cover real child-process dispatch, saved inputs, manual active/pending priority, the waiting-lock race, atomic rollback and duplicate rejection, clock failure, sleep/restart/timezone/configuration changes, and shutdown of an active scheduled run and its owned browser.
- Schedule editing controls and history deletion are the next layer.
- Verification reached DMG packaging after all checks, tests, and Clippy passed. The initial DMG build hit Finder AppleScript error `-10006` while setting window status-bar visibility. The app had compiled successfully; the app and DMG then built successfully with Tauri's noninteractive CI option (`CI=true TAURI_BUNDLER_DMG_IGNORE_CI=false pnpm build`). [Upstream bundler option](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/macos/dmg/mod.rs).
- Completed verification: `pnpm check` → `pnpm typecheck` → `pnpm test` → build, with the build retried in the supported noninteractive mode above. All eight new dispatch checks and the default suite passed; all-target Clippy passed. Unrelated opt-in browser tests remained disabled.
