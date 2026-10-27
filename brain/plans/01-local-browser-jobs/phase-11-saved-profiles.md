# 11. Store and refresh named login profiles

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Store and refresh named login profiles. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `src-tauri/src/profiles.rs`
- `src-tauri/src/commands.rs`
- `app/components/ProfilesPanel.vue`

Let users create/refresh named profiles through the selected installed browser (Helium) in a dedicated app-owned directory. User confirms login completion before capture. Encrypt captured state at rest using a key held in macOS Keychain via a maintained library; keep secrets out of logs and run records. Restrict temporary state file permissions and clean them on all paths and startup recovery.

## Data structures

- SessionProfile: name, captured state reference, refresh status and timestamps.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Create and refresh a profile, restart the app, and restore its state. Cancelling setup preserves the prior state. A missing capture browser and Keychain denial fail clearly without exposing session contents.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.

## Progress

- Implemented on 2026-10-27.

- Implemented named profiles, login capture, refresh, and cancellation in the app. The form defaults to installed Helium and the Quotes demo login URL.
- Rust starts the packaged Node capture helper with an app-owned temporary browser directory. Login completion requires the user to click Save profile. The helper uses a separate CDP connection and never opens the personal browser profile.
- Session state is encrypted with AES-256-GCM and a random nonce. The encryption key is held in macOS Keychain through keyring 4.2.0. Profile metadata contains no captured cookies or storage values; decryption stays in Rust.
- Profile replacement is atomic. Cancelled captures, browser failures, and Keychain denial preserve the prior profile. Temporary directories are private; state files use mode 0600. Owner-pipe closure, browser disconnection, app shutdown, and startup recovery clean capture resources. The helper removes browser data before exit, and the panel detects closed captures.
- Six focused Rust tests cover encryption, tampering, restart recovery, selected-profile refresh, failure boundaries, native Keychain, and real Helium creation/refresh/cancellation. Native checks use synthetic sessions and a separate test Keychain entry, which is deleted afterward.
- Runtime reproduction after packaging: `BAMBOO_CAPTURE_HEADLESS=1 BAMBOO_PROFILE_TEST=1 BAMBOO_KEYCHAIN_TEST=1 BAMBOO_NODE_EXECUTABLE=/absolute/path/to/node cargo test --manifest-path src-tauri/Cargo.toml --lib profiles::tests`.
- Verification passed in repository order: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`. The suite passed with real Helium and native Keychain checks enabled; unrelated opt-in browser tests remained disabled. Focused Oxlint and all-target Clippy also passed. The macOS app and DMG were built.
- Applying saved profiles to jobs and marking expired authentication remain phase 12.

## Sources

- [keyring credential storage](https://docs.rs/keyring/4.2.0/keyring/v1/index.html).
- [RustCrypto AES-GCM](https://docs.rs/aes-gcm/0.11.1/aes_gcm/).
