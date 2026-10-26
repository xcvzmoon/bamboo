# 11. Store and refresh named login profiles

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Store and refresh named login profiles. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/profiles.rs`
- `src-tauri/src/commands.rs`
- `app/components/ProfilesPanel.vue`

Let users create/refresh named profiles through installed Chrome in a dedicated app-owned directory. User confirms login completion before capture. Encrypt captured state at rest using a key held in macOS Keychain via a maintained library; keep secrets out of logs and run records. Restrict temporary state file permissions and clean them on all paths and startup recovery.

## Data structures

- SessionProfile: name, captured state reference, refresh status and timestamps.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Create and refresh a profile, restart the app, and restore its state. Cancelling setup preserves the prior state. Missing Chrome and Keychain denial fail clearly without exposing session contents.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
