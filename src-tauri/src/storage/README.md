# Local storage

Tauri initializes `Storage` at `<app_data_dir>/storage` during setup. Initialization
errors abort startup with a stable `STORAGE_*` code. The database and artifacts
belong to the current app user; storage does not depend on the working directory.

The module holds a root path and opens a SQLite connection per operation. Its
handles can be cloned across worker threads. Call its synchronous operations from
blocking workers when adding Tauri commands; filesystem access, SQLite lock waits,
serialization, and hashing can block.

## Metadata

`bamboo.sqlite3` contains two strict tables, `jobs` and `runs`. Schema version 1 and
an application identifier distinguish a new Bamboo database from unrelated or
unsupported databases. Opening an unsupported schema never resets it. There are
no legacy migrations.

Jobs store their name, canonical absolute project and entry paths, default JSON
input, timeout in milliseconds, and creation time. Registration requires a local
project directory, an existing .ts or .mts entry, UTF-8 paths, and a positive
millisecond timeout.

Queuing copies the job configuration and the selected JSON input into an immutable
run snapshot. Later changes to a job do not rewrite its run history. Run IDs are
UUID v7 values; artifact paths are derived from those IDs rather than caller-supplied paths.
History queries are ordered and paginated.

Starting a queued run records a SHA-256 hash of the current entry file, supplied
Node/Lightpanda/Playwright versions, and its start time. It does not capture source
or dependency snapshots. Timestamps are Unix milliseconds; start and finish times
are bounded by the preceding timestamp if the system clock moves backwards.

The state transitions are:

- Queued to running, failed, cancelled, or interrupted.
- Running to succeeded, failed, cancelled, or interrupted.
- Terminal states cannot transition or open new log writers.

An immediate transaction serializes state changes. A partial unique index also
prevents two running rows, including across separate storage handles. Foreign keys
protect referenced jobs. SQLite uses WAL, full synchronization, and a five-second
busy timeout. Mutations obtain their return value before committing, so they do
not depend on a second connection after the write has committed.

## Artifacts

Each run has `runs/<uuid>/stdout.log` and `stderr.log`. The caller owns handles
returned by `open_log` and closes writers after child-process exit, before calling
`finish_run`. Logs remain bytes so invalid UTF-8 does not destroy captured output.
Directories use mode 0700; the database, log files, and result files use mode 0600.
Symlinked artifact files are rejected.

Successful completion writes `results.json` using a temporary file in the run
directory, syncs it, and publishes without replacing an existing file. The directory
is synced before SQLite commits success and its record count. Log files are synced
before finalizing any terminal state. A failed database update removes the new
result file; failed queue insertion removes its new artifact directory.

SQLite is authoritative across the filesystem/database boundary. `read_results`
requires a committed success row and verifies the dataset shape and record count.
An interrupted write can leave an orphan directory or result file; it cannot make
a queued or running row readable as a successful dataset. Crash reconciliation and
cleanup are part of the later recovery phase. History and artifacts are retained;
this phase does not add automatic deletion.

`StorageError::code()` exposes stable codes for input errors, missing metadata,
invalid transitions, a busy runner, unsupported schemas, filesystem/database/JSON
errors, missing or invalid artifacts, and clock failures. Operational errors retain
their source errors. Normal failure paths clean up newly created artifacts; cleanup
failures are logged as `STORAGE_CLEANUP` and preserve the original operation error.

## Verification

The Cargo integration tests in `tests/rust/storage.rs` use temporary on-disk databases and real files. They cover reopen,
job snapshots, hashes and versions, all terminal states, concurrent starts,
independent datasets, missing/corrupt files, filesystem conflicts, database-trigger
failures, rollback cleanup, unsupported databases, private permissions and SQL
constraints. Run them with:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test storage
```

The module is managed by Tauri but has no frontend commands yet. The process supervisor consumes this API; frontend registration controls remain
a later phase.
