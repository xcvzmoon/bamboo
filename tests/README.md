# Tests

All application tests live in this directory:

- `probes/` checks runtime startup, deadlines and cleanup with Vitest.
- `runner/` checks job loading, the file protocol and browser execution with Vitest.
- `rust/runner.rs` checks managed process ownership and outcome handling.
- `rust/storage.rs` checks the public storage API as a Cargo integration suite.

`pnpm test` stages the private runner package, then runs Vitest followed by Cargo. `pnpm test:unit` runs Vitest once;
`pnpm test:watch` starts its watch mode. Filter by directory, for example
`pnpm test:unit tests/runner`.

Browser tests are opt-in. To run the full TypeScript suite against installed
Lightpanda, including live Wikipedia extraction:

```sh
BAMBOO_RUNTIME_TEST=1 BAMBOO_WIKIPEDIA_TEST=1 pnpm test:unit
```

Vitest fixtures own temporary directories and browser processes. Each browser test
gets its own Lightpanda instance, and fixture cleanup runs after failures. Tests
use real files and child processes without module mocking.

Rust tests run through Cargo because they execute Rust code directly. Their target
is declared in `src-tauri/Cargo.toml`, so `cargo test` discovers them despite their
location outside the crate directory. Run storage tests alone with:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test storage
```

The storage suite verifies UUID v7 IDs for jobs and runs, creation ordering,
persistence, state transitions, concurrent starts, and filesystem/database failure
handling. It uses public storage methods and separate SQLite connections for
constraint and fault-injection checks.

Run `pnpm runner:package` before invoking Cargo tests directly on a fresh checkout.
