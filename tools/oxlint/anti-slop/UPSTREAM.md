# Vendored anti-slop plugin

Source: bundled `install-anti-slop` skill asset. Exact upstream source commit is unknown; the bundle is the recoverable pristine snapshot used for this update.

Installed paths:

- `tools/oxlint/anti-slop/index.ts`
- `tools/oxlint/anti-slop/rules/`
- `tools/oxlint/anti-slop/shared/`
- `tools/oxlint/anti-slop/effect/`
- `tools/oxlint/anti-slop/vendor/`

## Update record

- Updated the existing installation from the staged bundled snapshot.
- Added `no-array-filter-map`, `no-reduce-accumulator-copy`, and `require-readable-spacing`, plus their shared helpers and the bundled ESLint Stylistic adapter.
- Adopted the incoming implementations for existing generic rules and the existing opt-in Effect rule path.
- Preserved the repository's existing Vite+ registration, rule severities, ignores, and disabled Effect-plugin configuration. Effect rules remain unregistered because `effect` is not a direct package dependency.
- The pre-update installation is recoverable at `/tmp/buildr-anti-slop-backup.KVqYXC` for this working session.

Verification: pending repository checks.
