# Managed run supervisor

`RunSupervisor` owns admission, runtime preflight, Lightpanda and Node processes,
protocol files, captured logs, and final storage transitions. Tauri manages one
shared supervisor. A second request while it is busy fails with `RUN_BUSY` before
creating another queued run. SQLite also prevents two running rows.

## Runtime configuration

The `configure_runtimes` command accepts absolute `node` and `lightpanda` paths.
Configuration is written atomically to `runtimes.json` under Tauri's app config
directory, with a private directory and mode 0600 file. `runtime_paths` reads and
validates it. The paths must refer to executable files. Selected symlink paths are
preserved because launcher programs can depend on their invoked name.

The app does not depend on a terminal PATH to locate these executables. It neither
installs nor bundles Node or Lightpanda. Runtime preflight requires Node 26+ and
records the actual Node, Lightpanda, and private Playwright versions. Node preflight
and job launch use the same project working directory, including when a selected
launcher chooses a runtime based on that directory.

## Lifecycle

`run_job` accepts a job UUID and JSON input and asynchronously returns the final
run. It emits `run-updated` with the final run after persistence. Registration and
frontend run controls belong to the next phase.

Each accepted request persists a queued run and opens its private log files. The
supervisor checks runtime versions, starts Lightpanda bound to loopback on an
ephemeral port, and waits up to ten seconds for its CDP endpoint. HTTP readiness
requests use no proxy and have a 300 ms request timeout. Version commands have a
three-second timeout and 4096-byte output limit.

Before launching Node, storage marks the run running and records the entry hash
and versions. A private mode 0700 temporary directory holds the mode 0600 request
and final outcome. The runner entry comes from app resources; the Node process
starts in the job's project directory. Both runtime programs are launched using
argv execution without a shell. stdout and stderr go directly to the run's log
files rather than unbounded in-memory buffers.

The job's saved timeout bounds preflight, startup and execution after preparation.
Timeout, missing/invalid outcomes, unsuccessful exits, browser exit, and job
failures produce failed runs without a dataset. Only a version 1 success outcome
with record-shaped JSON and a zero Node exit status is accepted. Failure codes
from the Node protocol are validated against its defined variants.

Owned processes have separate Unix process groups. Cleanup signals Node before
Lightpanda, waits up to two seconds per process, then escalates to SIGKILL and
reaps direct children. It also signals their remaining process-group descendants,
including loader helpers. Drop handlers kill process groups if an async operation
is abandoned. Trusted scripts that deliberately escape their process group are
outside this ownership model; it is not a sandbox.

Final state is persisted after cleanup and log-handle closure. Cleanup failures
prevent success. Start-state database tasks remain owned across timeout/shutdown,
so finalization waits for an in-flight start transaction. Closing the window
requests app exit; Tauri defers exit until supervisor shutdown finishes. Repeated
exit requests cannot bypass that wait. Active runs become interrupted on normal
shutdown. Forced app termination cannot run shutdown handlers; crash reconciliation
belongs to the recovery phase.

## Packaging

`runner/package.json` defines the runner's private production dependencies.
`pnpm runner:package` uses frozen-lockfile `pnpm deploy` with a hoisted node linker to stage a self-contained
package at `src-tauri/resources/runner`. Dependencies use real directories because Tauri resource copying omits dependency
symlinks. Generated resources are ignored by Git.
The package contains TypeScript entry files, Playwright, Valibot, tsx, and its
native esbuild helper. It contains no Node executable or browser downloads.

Tauri maps that directory to `runner/` under its resource directory in development
and release. Build and development hooks prepare it before starting Tauri.
`pnpm test` prepares it before the suites; when running Cargo directly on a fresh
checkout, first run `pnpm runner:package`.

Both packaged dependencies and user project dependencies retain their own module
resolution roots. The tests copy a deployment outside the repository and run an
enum-containing TypeScript job with a minimal PATH to check private loader/native
helper resolution. The Rust real-runtime test can also use an app bundle's assets:

```sh
BAMBOO_RUNTIME_TEST=1 \
BAMBOO_NODE_EXECUTABLE=/absolute/path/to/node \
BAMBOO_LIGHTPANDA_EXECUTABLE=/absolute/path/to/lightpanda \
BAMBOO_RUNNER_ASSETS=/path/to/bamboo.app/Contents/Resources/runner \
cargo test --manifest-path src-tauri/Cargo.toml --test runner real_packaged_runner_executes_a_typescript_browser_job
```

`tests/rust/runner.rs` uses real child fixtures for startup failures, invalid
outcomes, missing outcomes, descendant cleanup, deadlines, busy admission and
shutdown. Set `BAMBOO_RUNTIME_TEST=1` to include a real Lightpanda job. The remaining
fixtures use Node to provide deterministic CLI and HTTP process behavior.
