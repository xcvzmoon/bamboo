# 7. Show results and export JSON or CSV

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Show results and export JSON or CSV. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/results.rs`
- `src-tauri/src/commands.rs`
- `app/components/RunDetails.vue`

Display run status, logs and a bounded record preview. Save JSON automatically. Export CSV only when every value is a scalar or null, using a maintained CSV writer and union of record keys. Reject nested values for CSV; missing/null fields become empty cells. Preserve record order and handle empty datasets explicitly.

## Data structures

- RunDetails and CsvExportError: persisted logs, status, records, export outcome.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Complete the first milestone: run Wikipedia from the app and export both formats. Round-trip quotes, commas, newlines, Unicode and uneven keys; reject nested CSV without damaging JSON.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
