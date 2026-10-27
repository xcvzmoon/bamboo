CREATE TABLE IF NOT EXISTS schedules (
    job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    definition TEXT NOT NULL CHECK(json_valid(definition)),
    anchor_at INTEGER NOT NULL,
    last_occurrence INTEGER CHECK(last_occurrence IS NULL OR last_occurrence > anchor_at)
) STRICT;
