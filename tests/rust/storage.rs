use app_lib::storage::*;
use rusqlite::Connection;
use serde_json::json;
use serde_json::{Map, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use std::{
    io::Write,
    sync::{Arc, Barrier},
};
use tempfile::TempDir;
use uuid::Uuid;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    _temporary: TempDir,
    storage: Storage,
    root: PathBuf,
    project: PathBuf,
    entry: PathBuf,
}

impl Fixture {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let temporary = TempDir::new()?;
        let project = temporary.path().join("project with spaces");
        fs::create_dir(&project)?;
        let entry = project.join("job.ts");
        fs::write(&entry, "abc")?;
        let root = temporary.path().join("storage");
        let storage = Storage::open(&root)?;
        Ok(Self {
            _temporary: temporary,
            storage,
            root,
            project,
            entry,
        })
    }

    fn job(&self) -> std::result::Result<Job, StorageError> {
        self.storage.create_job(
            "Wikipedia",
            &self.project,
            &self.entry,
            &json!({"topic":"Bamboo"}),
            Duration::from_secs(30),
        )
    }
}

fn versions() -> RuntimeVersions {
    RuntimeVersions {
        node: "26.8.1".into(),
        lightpanda: "fixture-version".into(),
        playwright: "1.64.1".into(),
    }
}

fn records() -> Records {
    vec![Map::from_iter([
        ("title".into(), json!("Bamboo")),
        ("nested".into(), json!({"values":[true,null,1]})),
    ])]
}

#[test]
fn jobs_runs_artifacts_and_snapshots_survive_reopen() -> TestResult {
    let fixture = Fixture::new()?;
    let job = fixture.job()?;
    assert_eq!(job.id.get_version_num(), 7);
    assert!(job.project_path.is_absolute());
    assert!(job.entry_path.is_absolute());
    assert_eq!(job.timeout_ms, 30_000);
    let queued = fixture
        .storage
        .queue_run(job.id, &json!({"topic":"Other"}))?;
    assert_eq!(queued.id.get_version_num(), 7);
    assert_eq!(queued.status, RunStatus::Queued);
    assert_eq!(queued.job.input, json!({"topic":"Other"}));
    assert_eq!(
        fixture.storage.get_job(job.id)?.input,
        json!({"topic":"Bamboo"})
    );
    {
        let mut stdout = fixture.storage.open_log(queued.id, LogStream::Stdout)?;
        stdout.write_all(b"job stdout\n")?;
        let mut stderr = fixture.storage.open_log(queued.id, LogStream::Stderr)?;
        stderr.write_all(&[0xff, 0])?;
    }
    let running = fixture.storage.start_run(queued.id, versions())?;
    assert_eq!(
        running.entry_hash.as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(running.runtime_versions, Some(versions()));
    let completed = fixture
        .storage
        .finish_run(queued.id, RunCompletion::Succeeded(records()))?;
    assert_eq!(completed.status, RunStatus::Succeeded);
    assert_eq!(completed.record_count, Some(1));
    assert!(completed.finished_at >= completed.started_at);
    let reopened = Storage::open(&fixture.root)?;
    assert_eq!(reopened.get_job(job.id)?, job);
    assert_eq!(reopened.list_jobs()?, vec![job.clone()]);
    assert_eq!(reopened.get_run(queued.id)?, completed);
    assert_eq!(reopened.list_runs(job.id, 10, 0)?, vec![completed]);
    assert!(reopened.list_runs(job.id, 10, 1)?.is_empty());
    assert_eq!(reopened.read_results(queued.id)?, records());
    assert_eq!(
        reopened.read_log(queued.id, LogStream::Stdout)?,
        b"job stdout\n"
    );
    assert_eq!(
        reopened.read_log(queued.id, LogStream::Stderr)?,
        vec![0xff, 0]
    );
    Ok(())
}

#[test]
fn terminal_states_are_final_and_never_publish_failure_datasets() -> TestResult {
    let fixture = Fixture::new()?;
    let job = fixture.job()?;
    for active in [false, true] {
        for completion in [
            RunCompletion::Failed(RunFailure {
                code: "JOB_EXECUTION".into(),
                message: "Job failed".into(),
            }),
            RunCompletion::Cancelled,
            RunCompletion::Interrupted,
        ] {
            let queued = fixture.storage.queue_run(job.id, &Value::Null)?;
            if active {
                fixture.storage.start_run(queued.id, versions())?;
            }
            let finished = fixture.storage.finish_run(queued.id, completion)?;
            assert!(finished.finished_at.is_some());
            assert!(finished.failure.is_some());
            assert_eq!(finished.record_count, None);
            assert_eq!(
                fixture
                    .storage
                    .read_results(queued.id)
                    .err()
                    .map(|error| error.code()),
                Some("STORAGE_TRANSITION")
            );
            assert_eq!(
                fixture
                    .storage
                    .start_run(queued.id, versions())
                    .err()
                    .map(|error| error.code()),
                Some("STORAGE_TRANSITION")
            );
            assert_eq!(
                fixture
                    .storage
                    .finish_run(queued.id, RunCompletion::Cancelled)
                    .err()
                    .map(|error| error.code()),
                Some("STORAGE_TRANSITION")
            );
            assert_eq!(
                fixture
                    .storage
                    .open_log(queued.id, LogStream::Stdout)
                    .err()
                    .map(|error| error.code()),
                Some("STORAGE_TRANSITION")
            );
            assert!(
                !fixture
                    .root
                    .join("runs")
                    .join(queued.id.to_string())
                    .join("results.json")
                    .exists()
            );
        }
    }
    let queued = fixture.storage.queue_run(job.id, &Value::Null)?;
    assert_eq!(
        fixture
            .storage
            .finish_run(queued.id, RunCompletion::Succeeded(vec![]))
            .err()
            .map(|error| error.code()),
        Some("STORAGE_TRANSITION")
    );
    fixture.storage.start_run(queued.id, versions())?;
    fixture
        .storage
        .finish_run(queued.id, RunCompletion::Succeeded(vec![]))?;
    assert!(fixture.storage.read_results(queued.id)?.is_empty());
    Ok(())
}

#[test]
fn concurrent_connections_allow_only_one_active_run() -> TestResult {
    let fixture = Fixture::new()?;
    let job = fixture.job()?;
    let first = fixture.storage.queue_run(job.id, &Value::Null)?;
    let second = fixture.storage.queue_run(job.id, &Value::Null)?;
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [first.id, second.id]
        .into_iter()
        .map(|id| {
            let storage = fixture.storage.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                storage.start_run(id, versions())
            })
        })
        .collect();
    let mut started = Vec::new();
    let mut busy = 0;
    for handle in handles {
        match handle.join().map_err(|_| "Storage worker panicked")? {
            Ok(run) => started.push(run),
            Err(StorageError::Busy) => busy += 1,
            Err(error) => return Err(error.into()),
        }
    }
    assert_eq!(started.len(), 1);
    assert_eq!(busy, 1);
    fixture
        .storage
        .finish_run(started[0].id, RunCompletion::Cancelled)?;
    let next = if started[0].id == first.id {
        second.id
    } else {
        first.id
    };
    fixture.storage.start_run(next, versions())?;
    Ok(())
}

#[test]
fn missing_sources_and_artifacts_have_stable_errors() -> TestResult {
    let fixture = Fixture::new()?;
    let job = fixture.job()?;
    let queued = fixture.storage.queue_run(job.id, &Value::Null)?;
    fs::remove_file(&fixture.entry)?;
    assert_eq!(
        fixture
            .storage
            .start_run(queued.id, versions())
            .err()
            .map(|error| error.code()),
        Some("STORAGE_IO")
    );
    assert_eq!(
        fixture.storage.get_run(queued.id)?.status,
        RunStatus::Queued
    );
    fs::write(&fixture.entry, "changed source")?;
    fixture.storage.start_run(queued.id, versions())?;
    let path = fixture
        .root
        .join("runs")
        .join(queued.id.to_string())
        .join("stdout.log");
    fs::remove_file(&path)?;
    assert_eq!(
        fixture
            .storage
            .finish_run(queued.id, RunCompletion::Succeeded(records()))
            .err()
            .map(|error| error.code()),
        Some("STORAGE_ARTIFACT_MISSING")
    );
    fs::write(path, [])?;
    fixture
        .storage
        .finish_run(queued.id, RunCompletion::Succeeded(records()))?;
    let result = fixture
        .root
        .join("runs")
        .join(queued.id.to_string())
        .join("results.json");
    fs::write(&result, "[42]")?;
    assert_eq!(
        fixture
            .storage
            .read_results(queued.id)
            .err()
            .map(|error| error.code()),
        Some("STORAGE_ARTIFACT_INVALID")
    );
    fs::remove_file(result)?;
    assert_eq!(
        fixture
            .storage
            .read_results(queued.id)
            .err()
            .map(|error| error.code()),
        Some("STORAGE_ARTIFACT_MISSING")
    );
    assert_eq!(
        fixture
            .storage
            .get_job(Uuid::now_v7())
            .err()
            .map(|error| error.code()),
        Some("STORAGE_NOT_FOUND")
    );
    assert_eq!(
        fixture
            .storage
            .get_run(Uuid::now_v7())
            .err()
            .map(|error| error.code()),
        Some("STORAGE_NOT_FOUND")
    );
    Ok(())
}

#[test]
fn database_failures_roll_back_metadata_and_new_artifacts() -> TestResult {
    let fixture = Fixture::new()?;
    let job = fixture.job()?;
    let connection = Connection::open(fixture.root.join("bamboo.sqlite3"))?;
    connection.execute_batch("CREATE TRIGGER runs_reject_insert BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT, 'fixture insert failure'); END;")?;
    assert_eq!(
        fixture
            .storage
            .queue_run(job.id, &Value::Null)
            .err()
            .map(|error| error.code()),
        Some("STORAGE_DATABASE")
    );
    assert_eq!(fs::read_dir(fixture.root.join("runs"))?.count(), 0);
    assert!(fixture.storage.list_runs(job.id, 10, 0)?.is_empty());
    connection.execute_batch("DROP TRIGGER runs_reject_insert;")?;
    let run = fixture.storage.queue_run(job.id, &Value::Null)?;
    fixture.storage.start_run(run.id, versions())?;
    connection.execute_batch("CREATE TRIGGER runs_reject_success BEFORE UPDATE ON runs WHEN NEW.status='succeeded' BEGIN SELECT RAISE(ABORT, 'fixture update failure'); END;")?;
    assert_eq!(
        fixture
            .storage
            .finish_run(run.id, RunCompletion::Succeeded(records()))
            .err()
            .map(|error| error.code()),
        Some("STORAGE_DATABASE")
    );
    assert_eq!(fixture.storage.get_run(run.id)?.status, RunStatus::Running);
    let directory = fixture.root.join("runs").join(run.id.to_string());
    assert_eq!(fs::read_dir(&directory)?.count(), 2);
    assert_eq!(
        fixture
            .storage
            .read_results(run.id)
            .err()
            .map(|error| error.code()),
        Some("STORAGE_TRANSITION")
    );
    connection.execute_batch("DROP TRIGGER runs_reject_success;")?;
    fixture
        .storage
        .finish_run(run.id, RunCompletion::Succeeded(records()))?;
    Ok(())
}

#[test]
fn artifact_failures_do_not_mark_success_or_overwrite_results() -> TestResult {
    let fixture = Fixture::new()?;
    let run = fixture.storage.queue_run(fixture.job()?.id, &Value::Null)?;
    fixture.storage.start_run(run.id, versions())?;
    let path = fixture
        .root
        .join("runs")
        .join(run.id.to_string())
        .join("results.json");
    fs::write(&path, "preserve this file")?;
    assert_eq!(
        fixture
            .storage
            .finish_run(run.id, RunCompletion::Succeeded(records()))
            .err()
            .map(|error| error.code()),
        Some("STORAGE_IO")
    );
    assert_eq!(fs::read_to_string(&path)?, "preserve this file");
    assert_eq!(fixture.storage.get_run(run.id)?.status, RunStatus::Running);
    assert_eq!(
        fixture
            .storage
            .read_results(run.id)
            .err()
            .map(|error| error.code()),
        Some("STORAGE_TRANSITION")
    );
    Ok(())
}

#[test]
fn invalid_inputs_schemas_and_corrupt_databases_fail_without_resetting() -> TestResult {
    let fixture = Fixture::new()?;
    assert_eq!(
        fixture
            .storage
            .create_job(
                "",
                &fixture.project,
                &fixture.entry,
                &Value::Null,
                Duration::from_secs(1)
            )
            .err()
            .map(|error| error.code()),
        Some("STORAGE_INPUT")
    );
    assert_eq!(
        fixture
            .storage
            .create_job(
                "job",
                &fixture.project,
                &fixture.entry,
                &Value::Null,
                Duration::ZERO
            )
            .err()
            .map(|error| error.code()),
        Some("STORAGE_INPUT")
    );
    assert_eq!(
        fixture
            .storage
            .create_job(
                "job",
                Path::new("relative"),
                &fixture.entry,
                &Value::Null,
                Duration::from_secs(1)
            )
            .err()
            .map(|error| error.code()),
        Some("STORAGE_INPUT")
    );
    Connection::open(fixture.root.join("bamboo.sqlite3"))?.pragma_update(
        None,
        "user_version",
        2,
    )?;
    assert_eq!(
        Storage::open(&fixture.root).err().map(|error| error.code()),
        Some("STORAGE_SCHEMA")
    );
    let corrupt = fixture._temporary.path().join("corrupt");
    fs::create_dir(&corrupt)?;
    fs::write(corrupt.join("bamboo.sqlite3"), "not sqlite")?;
    assert_eq!(
        Storage::open(&corrupt).err().map(|error| error.code()),
        Some("STORAGE_DATABASE")
    );
    let blocked = fixture._temporary.path().join("blocked");
    fs::write(&blocked, "file")?;
    assert_eq!(
        Storage::open(&blocked).err().map(|error| error.code()),
        Some("STORAGE_IO")
    );
    Ok(())
}

#[test]
fn private_permissions_and_database_constraints_are_enforced() -> TestResult {
    let fixture = Fixture::new()?;
    let run = fixture.storage.queue_run(fixture.job()?.id, &Value::Null)?;
    let directory = fixture.root.join("runs").join(run.id.to_string());
    for path in [&fixture.root, &fixture.root.join("runs"), &directory] {
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o700);
    }
    for path in [
        fixture.root.join("bamboo.sqlite3"),
        directory.join("stdout.log"),
        directory.join("stderr.log"),
    ] {
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    }
    let connection = Connection::open(fixture.root.join("bamboo.sqlite3"))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    assert!(
        connection
            .execute(
                "UPDATE runs SET status='succeeded' WHERE id=?1",
                [run.id.to_string()]
            )
            .is_err()
    );
    assert!(
        connection
            .execute("DELETE FROM jobs WHERE id=?1", [run.job.id.to_string()])
            .is_err()
    );
    fixture.storage.start_run(run.id, versions())?;
    fixture
        .storage
        .finish_run(run.id, RunCompletion::Succeeded(records()))?;
    assert_eq!(
        fs::metadata(directory.join("results.json"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    Ok(())
}

#[test]
fn successful_runs_keep_independent_datasets_and_job_history() -> TestResult {
    let fixture = Fixture::new()?;
    let job = fixture.job()?;
    let first = fixture.storage.queue_run(job.id, &job.input)?;
    Connection::open(fixture.root.join("bamboo.sqlite3"))?.execute(
        "UPDATE jobs SET name='Edited job',timeout_ms=1000 WHERE id=?1",
        [job.id.to_string()],
    )?;
    let second = fixture.storage.queue_run(job.id, &Value::Null)?;
    assert_eq!(fixture.storage.get_run(first.id)?.job.name, "Wikipedia");
    assert_eq!(fixture.storage.get_run(first.id)?.job.timeout_ms, 30_000);
    assert_eq!(second.job.name, "Edited job");
    assert_eq!(second.job.timeout_ms, 1000);
    fixture.storage.start_run(first.id, versions())?;
    let dataset: Records = (0..2000)
        .map(|index| Map::from_iter([("index".into(), json!(index))]))
        .collect();
    fixture
        .storage
        .finish_run(first.id, RunCompletion::Succeeded(dataset.clone()))?;
    fixture.storage.start_run(second.id, versions())?;
    fixture
        .storage
        .finish_run(second.id, RunCompletion::Succeeded(vec![]))?;
    assert_eq!(fixture.storage.read_results(first.id)?, dataset);
    assert!(fixture.storage.read_results(second.id)?.is_empty());
    assert_eq!(fixture.storage.list_runs(job.id, 10, 0)?.len(), 2);
    assert_eq!(fixture.storage.list_runs(job.id, 1, 1)?.len(), 1);
    Ok(())
}

#[test]
fn unrelated_databases_and_symlinked_storage_are_rejected() -> TestResult {
    let temporary = TempDir::new()?;
    let root = temporary.path().join("unrelated");
    fs::create_dir(&root)?;
    let connection = Connection::open(root.join("bamboo.sqlite3"))?;
    connection.execute("CREATE TABLE fixtures (id INTEGER PRIMARY KEY)", [])?;
    assert_eq!(
        Storage::open(&root).err().map(|error| error.code()),
        Some("STORAGE_SCHEMA")
    );
    assert_eq!(
        connection.query_row("SELECT count(*) FROM fixtures", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    let linked = temporary.path().join("linked");
    fs::create_dir(&linked)?;
    let target = temporary.path().join("outside.sqlite3");
    std::os::unix::fs::symlink(&target, linked.join("bamboo.sqlite3"))?;
    assert_eq!(
        Storage::open(&linked).err().map(|error| error.code()),
        Some("STORAGE_INPUT")
    );
    assert!(!target.exists());
    let directory_link = temporary.path().join("linked-directory");
    std::os::unix::fs::symlink(&root, &directory_link)?;
    assert_eq!(
        Storage::open(&directory_link)
            .err()
            .map(|error| error.code()),
        Some("STORAGE_INPUT")
    );
    Ok(())
}

#[test]
fn database_ids_are_uuid_v7_and_follow_creation_order() -> TestResult {
    let fixture = Fixture::new()?;
    let first_job = fixture.job()?;
    let second_job = fixture.job()?;
    let first_run = fixture.storage.queue_run(first_job.id, &Value::Null)?;
    let second_run = fixture.storage.queue_run(first_job.id, &Value::Null)?;
    for id in [first_job.id, second_job.id, first_run.id, second_run.id] {
        assert_eq!(id.get_version_num(), 7);
    }
    assert!(first_job.id < second_job.id);
    assert!(first_run.id < second_run.id);
    let reopened = Storage::open(&fixture.root)?;
    assert_eq!(reopened.get_job(first_job.id)?.id, first_job.id);
    assert_eq!(reopened.get_run(first_run.id)?.id, first_run.id);
    Ok(())
}

#[test]
fn json_null_and_primitive_job_inputs_round_trip_as_json() -> TestResult {
    let fixture = Fixture::new()?;
    for input in [
        Value::Null,
        json!(42),
        json!(true),
        json!("text"),
        json!([null, 1]),
    ] {
        let job = fixture.storage.create_job(
            "JSON input",
            &fixture.project,
            &fixture.entry,
            &input,
            Duration::from_secs(10),
        )?;
        assert_eq!(fixture.storage.get_job(job.id)?.input, input);
    }
    Ok(())
}
