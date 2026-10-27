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

## Progress

- Implemented on 2026-10-27. The model supports disabled schedules, positive minute/hour intervals, and daily local hour/minute values. Invalid times and unsupported date ranges return stable error codes.
- Interval slots keep the saved anchor and advance directly past missed slots. Daily slots use the current timezone. Gaps select the transition's first valid instant, including half-hour and skipped-date transitions; folds select only the earlier instant.
- Saved definitions, revision IDs, anchors, and occurrence watermarks live in SQLite. Updates reset the anchor and invalidate older revisions. Occurrence recording is atomic and monotonic, and ignores disabled schedules or stale revisions. A backward clock adjustment does not repeat an already recorded slot or generate occurrences before activation.
- `Clock` supplies the timestamp and timezone together for calculation and storage tests. `SystemClock` reads macOS `/etc/localtime` on every request and parses it with Jiff 0.2.38, avoiding Jiff's cached system-zone lookup. Calculated next times are not persisted, so the next query reflects a timezone change without stale cached deadlines.
- Tauri commands expose schedule saving and current next-occurrence previews. Nine Rust tests cover intervals, strict boundaries, missed days, restart persistence, DST gaps/folds, skipped dates, timezone changes, clock rewinds, stale revisions, invalid input, clock failure, and the actual system clock.
- Automatic dispatch and schedule controls remain the following phases. No timer or background job execution has been introduced.

- Verification passed in order: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`. All nine scheduling tests passed, as did all-target Clippy. The macOS app and DMG were built. Unrelated opt-in browser tests remained disabled.

## Sources

- [Jiff timezone conversion and transition APIs](https://docs.rs/jiff/0.2.38/jiff/tz/struct.TimeZone.html).
- [Jiff system timezone behavior](https://docs.rs/jiff/0.2.38/jiff/_documentation/platform/index.html).
