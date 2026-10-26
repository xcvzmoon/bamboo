# 3. Separate results from user logs

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Separate results from user logs. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `runner/main.ts`
- `runner/protocol.ts`
- `runner/protocol.test.ts`

Use per-run private request/outcome files with atomic final outcome publication; stdout/stderr remain logs. Validate JSON-compatible results at the boundary, rejecting undefined values, cycles, non-finite numbers, and non-record entries. Empty arrays are valid. No dataset is published on failure.

## Data structures

- RunRequest and RunOutcome: versioned JSON messages with stable failure codes.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Verify malformed inputs, invalid results, logs mixed with output, empty results, and a process exiting without an outcome. Use real child processes without module mocking.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
