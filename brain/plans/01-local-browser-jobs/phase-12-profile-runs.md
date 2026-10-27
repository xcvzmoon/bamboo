# 12. Apply profiles and signal expired authentication

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Apply profiles and signal expired authentication. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `runner/job.ts`
- `runner/main.ts`
- `src-tauri/src/runner.rs`

Allow an optional profile per job. Load state into the managed context and commit updated state atomically only after successful result validation and successful run publication. Failed/cancelled runs preserve prior saved state. Explicit authentication-required errors mark the profile as needing refresh. Do not infer expiry from generic navigation failures or claim rollback of server-side effects.

## Data structures

- AuthenticationRequiredError and ProfileRunOutcome: explicit script signal and candidate updated state.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Successful cookie rotation persists; ordinary failure and cancellation preserve saved state. Explicit auth-required failure marks refresh needed and publishes no dataset. Session and dataset publication have a tested crash-recovery strategy.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.

## Progress

- Implemented on 2026-10-27. Saved jobs can select an optional login profile; queue snapshots retain the selected profile ID. No profile means a fresh context.
- Rust decrypts state into a private run protocol file. The runner imports it into the managed context and exports a candidate only after result validation. Existing process cleanup removes plaintext state after every run; startup recovery removes interrupted protocol directories.
- `AuthenticationRequiredError` exports the stable `AUTHENTICATION_REQUIRED` signal. It publishes no dataset and marks that profile revision as needing refresh. Ordinary failures do not infer expired authentication. Invalid imported state reports `PROFILE_STATE` without exposing session values.
- Before result publication, Rust writes an encrypted candidate journal with the run ID and original profile revision. Only a committed successful run can replace the profile. Cancellation, app shutdown, failed results, and interrupted runs discard their candidates. Recovery replays committed updates and discards interrupted ones; replay is idempotent.
- A manual refresh takes priority over older run candidates and authentication signals. Revision hashes prevent an in-flight run from overwriting newer saved state. Publication errors retain the encrypted journal for startup or the next run to retry.
- Focused tests cover queue selection snapshots, persistence, publication failure, restart recovery, manual refresh conflicts, native Lightpanda cookie rotation, ordinary errors, invalid results, cancellation, explicit authentication failure, and invalid imported state without secret logging.
- Runtime reproduction after packaging: `BAMBOO_PROFILE_RUN_TEST=1 BAMBOO_NODE_EXECUTABLE=/absolute/path/to/node BAMBOO_LIGHTPANDA_EXECUTABLE=/absolute/path/to/lightpanda cargo test --manifest-path src-tauri/Cargo.toml --lib profiles::tests`.
- Authoring instructions and the authentication signal example are in `runner/README.md`. Schedule calculation is the next layer.
- Verification passed in order: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`. The test suite included real profile-backed Lightpanda execution, Helium capture, and native Keychain checks. Unrelated opt-in browser tests stayed disabled. Focused Oxlint, all-target Clippy, and the macOS app/DMG build passed.
