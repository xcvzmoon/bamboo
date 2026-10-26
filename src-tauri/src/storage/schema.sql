CREATE TABLE jobs (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    project_path TEXT NOT NULL,
    entry_path TEXT NOT NULL,
    input TEXT NOT NULL CHECK(json_valid(input)),
    timeout_ms INTEGER NOT NULL CHECK(timeout_ms > 0),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE runs (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    job_snapshot TEXT NOT NULL CHECK(json_valid(job_snapshot)),
    status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'interrupted')),
    queued_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    entry_hash TEXT CHECK(entry_hash IS NULL OR length(entry_hash) = 64),
    runtime_versions TEXT CHECK(runtime_versions IS NULL OR json_valid(runtime_versions)),
    failure TEXT CHECK(failure IS NULL OR json_valid(failure)),
    record_count INTEGER CHECK(record_count IS NULL OR record_count >= 0),
    CHECK(started_at IS NULL OR started_at >= queued_at),
    CHECK(finished_at IS NULL OR finished_at >= coalesce(started_at, queued_at)),
    CHECK((started_at IS NULL AND entry_hash IS NULL AND runtime_versions IS NULL)
       OR (started_at IS NOT NULL AND entry_hash IS NOT NULL AND runtime_versions IS NOT NULL)),
    CHECK((status = 'queued' AND started_at IS NULL AND finished_at IS NULL AND failure IS NULL AND record_count IS NULL)
       OR (status = 'running' AND started_at IS NOT NULL AND finished_at IS NULL AND failure IS NULL AND record_count IS NULL)
       OR (status = 'succeeded' AND started_at IS NOT NULL AND finished_at IS NOT NULL AND failure IS NULL AND record_count IS NOT NULL)
       OR (status IN ('failed', 'cancelled', 'interrupted') AND finished_at IS NOT NULL AND failure IS NOT NULL AND record_count IS NULL))
) STRICT;

CREATE INDEX runs_history ON runs(job_id, queued_at DESC, id DESC);
CREATE UNIQUE INDEX runs_one_running ON runs((1)) WHERE status = 'running';
PRAGMA application_id = 1111575874;
PRAGMA user_version = 1;
