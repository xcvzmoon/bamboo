use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("STORAGE_IO: {0}")]
    Io(#[from] io::Error),
    #[error("STORAGE_DATABASE: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("STORAGE_JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("STORAGE_INPUT: {0}")]
    InvalidInput(&'static str),
    #[error("STORAGE_NOT_FOUND: Job or run does not exist")]
    NotFound,
    #[error("STORAGE_TRANSITION: Run cannot make this state transition")]
    InvalidTransition,
    #[error("STORAGE_BUSY: Another run is already active")]
    Busy,
    #[error("STORAGE_SCHEMA: Database is not a supported Bamboo schema")]
    UnsupportedSchema,
    #[error("STORAGE_ARTIFACT_MISSING: {0}")]
    MissingArtifact(PathBuf),
    #[error("STORAGE_ARTIFACT_INVALID: Artifact is not a regular file or valid dataset")]
    InvalidArtifact,
    #[error("HISTORY_ACTIVE: Wait for queued, running, or finalizing work before deleting history")]
    HistoryActive,
    #[error(
        "HISTORY_COMMIT: Some files may have been removed, but history changes could not be committed. Retry deletion"
    )]
    DeletionCommit(#[source] rusqlite::Error),
    #[error("STORAGE_CLOCK: System time cannot be represented as Unix milliseconds")]
    Clock,
}

impl StorageError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "STORAGE_IO",
            Self::Database(_) => "STORAGE_DATABASE",
            Self::Json(_) => "STORAGE_JSON",
            Self::InvalidInput(_) => "STORAGE_INPUT",
            Self::NotFound => "STORAGE_NOT_FOUND",
            Self::InvalidTransition => "STORAGE_TRANSITION",
            Self::Busy => "STORAGE_BUSY",
            Self::UnsupportedSchema => "STORAGE_SCHEMA",
            Self::MissingArtifact(_) => "STORAGE_ARTIFACT_MISSING",
            Self::InvalidArtifact => "STORAGE_ARTIFACT_INVALID",
            Self::Clock => "STORAGE_CLOCK",
            Self::HistoryActive => "HISTORY_ACTIVE",
            Self::DeletionCommit(_) => "HISTORY_COMMIT",
        }
    }
}

type Result<T> = std::result::Result<T, StorageError>;
pub type Records = Vec<Map<String, Value>>;

#[derive(Clone, Copy)]
pub enum HistoryTarget {
    Run(Uuid),
    Job(Uuid),
}

#[derive(Debug, Serialize)]
pub struct DeleteRunFailure {
    pub run_id: Uuid,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct DeleteRunResult {
    pub deleted: Vec<Uuid>,
    pub failed: Vec<DeleteRunFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub name: String,
    pub profile_id: Option<Uuid>,
    pub project_path: PathBuf,
    pub entry_path: PathBuf,
    pub input: Value,
    pub timeout_ms: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeVersions {
    pub node: String,
    pub lightpanda: String,
    pub playwright: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFailure {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub id: Uuid,
    pub job: Job,
    pub status: RunStatus,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub entry_hash: Option<String>,
    pub runtime_versions: Option<RuntimeVersions>,
    pub failure: Option<RunFailure>,
    pub record_count: Option<i64>,
}

pub enum RunCompletion {
    Succeeded(Records),
    Failed(RunFailure),
    Cancelled,
    AppExit,
    Interrupted,
}

#[derive(Clone, Copy)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_absolute() {
            return Err(StorageError::InvalidInput("Storage path must be absolute"));
        }
        private_directory(root)?;
        let storage = Self {
            root: fs::canonicalize(root)?,
        };
        private_directory(&storage.root.join("runs"))?;
        let database = storage.root.join("bamboo.sqlite3");
        match fs::symlink_metadata(&database) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(StorageError::InvalidInput(
                    "Database must be a regular file",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&database)?;
        fs::set_permissions(&database, fs::Permissions::from_mode(0o600))?;
        let mut connection = storage.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 =
            transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let application: i64 =
            transaction.pragma_query_value(None, "application_id", |row| row.get(0))?;
        match (version, application) {
            (0, 0) => {
                let tables: i64 = transaction.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )?;
                if tables != 0 {
                    return Err(StorageError::UnsupportedSchema);
                }
                transaction.execute_batch(include_str!("storage/schema.sql"))?;
            }
            (1, 1111575874) => {}
            _ => return Err(StorageError::UnsupportedSchema),
        }
        transaction.execute_batch(include_str!("storage/job-profiles.sql"))?;
        transaction.execute_batch(include_str!("storage/schedules.sql"))?;
        transaction.commit()?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(storage)
    }

    fn connection(&self) -> Result<Connection> {
        let connection = Connection::open_with_flags(
            self.root.join("bamboo.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(connection)
    }

    pub fn create_job(
        &self,
        name: &str,
        project: &Path,
        entry: &Path,
        input: &Value,
        timeout: Duration,
    ) -> Result<Job> {
        if name.trim().is_empty() || !project.is_absolute() || !entry.is_absolute() {
            return Err(StorageError::InvalidInput(
                "Job needs a name and absolute project/entry paths",
            ));
        }
        let timeout_ms = i64::try_from(timeout.as_millis())
            .map_err(|_| StorageError::InvalidInput("Timeout is too large"))?;
        if timeout_ms == 0 {
            return Err(StorageError::InvalidInput(
                "Timeout must be at least one millisecond",
            ));
        }
        let project = fs::canonicalize(project)?;
        let entry = fs::canonicalize(entry)?;
        if !project.is_dir()
            || !entry.is_file()
            || !matches!(
                entry.extension().and_then(|value| value.to_str()),
                Some("ts" | "mts")
            )
        {
            return Err(StorageError::InvalidInput(
                "Project must be a directory and entry must be a TypeScript file",
            ));
        }
        let project_text = project
            .to_str()
            .ok_or(StorageError::InvalidInput("Project path must be UTF-8"))?;
        let entry_text = entry
            .to_str()
            .ok_or(StorageError::InvalidInput("Entry path must be UTF-8"))?;
        let id = Uuid::now_v7();
        let created_at = timestamp()?;
        self.connection()?.execute("INSERT INTO jobs (id,name,project_path,entry_path,input,timeout_ms,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![id.to_string(), name.trim(), project_text, entry_text, serde_json::to_string(input)?, timeout_ms, created_at])?;
        Ok(Job {
            id,
            name: name.trim().into(),
            profile_id: None,
            project_path: project,
            entry_path: entry,
            input: input.clone(),
            timeout_ms,
            created_at,
        })
    }

    pub fn create_protocol_directory(&self, id: Uuid) -> Result<tempfile::TempDir> {
        let run = self.get_run(id)?;
        if !matches!(run.status, RunStatus::Queued | RunStatus::Running) {
            return Err(StorageError::InvalidTransition);
        }
        Ok(tempfile::Builder::new()
            .prefix("protocol-")
            .tempdir_in(self.root.join("runs").join(id.to_string()))?)
    }

    pub fn recover_unfinished_runs(&self) -> Result<usize> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids = {
            let mut statement =
                transaction.prepare("SELECT id FROM runs WHERE status IN ('queued','running')")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &ids {
            let directory = self.root.join("runs").join(id);
            for entry in fs::read_dir(&directory)? {
                let entry = entry?;
                if entry.file_name().to_string_lossy().starts_with("protocol-")
                    && entry.file_type()?.is_dir()
                {
                    fs::remove_dir_all(entry.path())?;
                }
            }
            let result = directory.join("results.json");
            match fs::remove_file(result) {
                Ok(()) => File::open(&directory)?.sync_all()?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let failure = serde_json::to_string(&RunFailure {
            code: "RUN_INTERRUPTED".into(),
            message: "App stopped before the run finished; it will not be resumed".into(),
        })?;
        let count = transaction.execute(
            "UPDATE runs SET status='interrupted',finished_at=MAX(?1,COALESCE(started_at,queued_at)),failure=?2,record_count=NULL WHERE status IN ('queued','running')",
            params![timestamp()?, failure],
        )?;
        transaction.commit()?;
        Ok(count)
    }

    pub fn get_job(&self, id: Uuid) -> Result<Job> {
        self.connection()?
            .query_row("SELECT jobs.*,job_profiles.profile_id FROM jobs LEFT JOIN job_profiles ON jobs.id=job_profiles.job_id WHERE jobs.id=?1", [id.to_string()], read_job)
            .optional()?
            .ok_or(StorageError::NotFound)
    }

    pub fn list_jobs(&self) -> Result<Vec<Job>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT jobs.*,job_profiles.profile_id FROM jobs LEFT JOIN job_profiles ON jobs.id=job_profiles.job_id ORDER BY created_at,jobs.id")?;
        Ok(statement
            .query_map([], read_job)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn save_job_settings(
        &self,
        id: Uuid,
        name: &str,
        input: &Value,
        timeout_ms: i64,
    ) -> Result<Job> {
        if name.trim().is_empty() || timeout_ms <= 0 {
            return Err(StorageError::InvalidInput(
                "Job needs a name and a positive timeout in milliseconds",
            ));
        }
        let connection = self.connection()?;
        if connection.execute(
            "UPDATE jobs SET name=?2,input=?3,timeout_ms=?4 WHERE id=?1",
            params![
                id.to_string(),
                name.trim(),
                serde_json::to_string(input)?,
                timeout_ms
            ],
        )? == 0
        {
            return Err(StorageError::NotFound);
        }
        self.get_job(id)
    }

    pub fn save_schedule(
        &self,
        job_id: Uuid,
        schedule: crate::schedule::Schedule,
        clock: &impl crate::schedule::Clock,
    ) -> std::result::Result<crate::schedule::ScheduleDetails, crate::schedule::ScheduleError> {
        self.get_job(job_id)?;
        schedule.validate()?;
        let reading = clock.read()?;
        let saved = crate::schedule::SavedSchedule {
            job_id,
            revision: Uuid::now_v7(),
            schedule,
            anchor_at: reading.now.as_millisecond(),
            last_occurrence: None,
        };
        let details = saved.details(&reading)?;
        self.connection()?.execute("INSERT INTO schedules(job_id,revision,definition,anchor_at,last_occurrence) VALUES (?1,?2,?3,?4,NULL) ON CONFLICT(job_id) DO UPDATE SET revision=excluded.revision,definition=excluded.definition,anchor_at=excluded.anchor_at,last_occurrence=NULL", params![job_id.to_string(), saved.revision.to_string(), serde_json::to_string(&saved.schedule).map_err(StorageError::from)?, saved.anchor_at]).map_err(StorageError::from)?;
        Ok(details)
    }

    pub fn list_schedules(&self) -> Result<Vec<crate::schedule::SavedSchedule>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT * FROM schedules ORDER BY job_id")?;
        Ok(statement
            .query_map([], read_schedule)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_schedule(&self, job_id: Uuid) -> Result<Option<crate::schedule::SavedSchedule>> {
        self.get_job(job_id)?;
        Ok(self
            .connection()?
            .query_row(
                "SELECT * FROM schedules WHERE job_id=?1",
                [job_id.to_string()],
                read_schedule,
            )
            .optional()?)
    }

    pub fn record_schedule_occurrence(
        &self,
        job_id: Uuid,
        revision: Uuid,
        occurrence: i64,
    ) -> Result<bool> {
        jiff::Timestamp::from_millisecond(occurrence).map_err(|_| {
            StorageError::InvalidInput("Schedule occurrence exceeds the supported timestamp range")
        })?;
        Ok(self.connection()?.execute("UPDATE schedules SET last_occurrence=?3 WHERE job_id=?1 AND revision=?2 AND json_extract(definition,'$.kind')<>'disabled' AND ?3>anchor_at AND (last_occurrence IS NULL OR last_occurrence<?3)", params![job_id.to_string(), revision.to_string(), occurrence])? == 1)
    }

    pub fn set_job_profile(&self, job_id: Uuid, profile_id: Option<Uuid>) -> Result<Job> {
        self.get_job(job_id)?;
        let connection = self.connection()?;
        if let Some(profile_id) = profile_id {
            connection.execute("INSERT INTO job_profiles(job_id,profile_id) VALUES (?1,?2) ON CONFLICT(job_id) DO UPDATE SET profile_id=excluded.profile_id", params![job_id.to_string(),profile_id.to_string()])?;
        } else {
            connection.execute(
                "DELETE FROM job_profiles WHERE job_id=?1",
                [job_id.to_string()],
            )?;
        }
        self.get_job(job_id)
    }

    pub fn queue_run(&self, job_id: Uuid, input: &Value) -> Result<Run> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut job = transaction
            .query_row(
                "SELECT jobs.*,job_profiles.profile_id FROM jobs LEFT JOIN job_profiles ON jobs.id=job_profiles.job_id WHERE jobs.id=?1",
                [job_id.to_string()],
                read_job,
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        job.input = input.clone();
        self.insert_queued_run(transaction, job)
    }

    pub fn queue_scheduled_run(
        &self,
        job_id: Uuid,
        revision: Uuid,
        occurrence: i64,
    ) -> Result<Option<Run>> {
        jiff::Timestamp::from_millisecond(occurrence).map_err(|_| {
            StorageError::InvalidInput("Schedule occurrence exceeds the supported timestamp range")
        })?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if transaction.execute("UPDATE schedules SET last_occurrence=?3 WHERE job_id=?1 AND revision=?2 AND json_extract(definition,'$.kind')<>'disabled' AND ?3>anchor_at AND (last_occurrence IS NULL OR last_occurrence<?3)", params![job_id.to_string(), revision.to_string(), occurrence])? == 0 {
            return Ok(None);
        }
        let job = transaction.query_row("SELECT jobs.*,job_profiles.profile_id FROM jobs LEFT JOIN job_profiles ON jobs.id=job_profiles.job_id WHERE jobs.id=?1", [job_id.to_string()], read_job)?;
        self.insert_queued_run(transaction, job).map(Some)
    }

    fn insert_queued_run(&self, transaction: Transaction<'_>, job: Job) -> Result<Run> {
        let job_id = job.id;
        let id = Uuid::now_v7();
        let directory = self.root.join("runs").join(id.to_string());
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let queued = (|| -> Result<Run> {
            for name in ["stdout.log", "stderr.log"] {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(directory.join(name))?
                    .sync_all()?;
            }
            File::open(&directory)?.sync_all()?;
            File::open(self.root.join("runs"))?.sync_all()?;
            transaction.execute("INSERT INTO runs (id,job_id,job_snapshot,status,queued_at) VALUES (?1,?2,?3,'queued',?4)", params![id.to_string(), job_id.to_string(), serde_json::to_string(&job)?, timestamp()?])?;
            let queued = get_run(&transaction, id)?;
            transaction.commit()?;
            Ok(queued)
        })();
        match queued {
            Ok(run) => Ok(run),
            Err(error) => {
                if let Err(cleanup) = fs::remove_dir_all(&directory) {
                    log::error!(
                        "STORAGE_CLEANUP: Failed to remove uncommitted run directory: {cleanup}"
                    );
                }
                Err(error)
            }
        }
    }

    pub(crate) fn deletion_candidates(&self, target: HistoryTarget) -> Result<Vec<Uuid>> {
        history_candidates(&self.connection()?, target)
    }

    pub fn delete_history(&self, target: HistoryTarget) -> Result<DeleteRunResult> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids = history_candidates(&transaction, target)?;
        let mut result = DeleteRunResult::default();
        for id in ids {
            let directory = self.root.join("runs").join(id.to_string());
            match fs::symlink_metadata(&directory) {
                Ok(metadata) if !metadata.is_dir() => {
                    result.failed.push(DeleteRunFailure {
                        run_id: id,
                        code: "HISTORY_FILESYSTEM",
                        message: "Run directory is not a regular directory. Metadata was kept."
                            .into(),
                    });
                    continue;
                }
                Ok(_) => {
                    if let Err(error) = fs::remove_dir_all(&directory) {
                        result.failed.push(DeleteRunFailure { run_id: id, code: "HISTORY_FILESYSTEM", message: format!("{error}. Some files may already be removed; metadata was kept. Retry deletion.") });
                        continue;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    result.failed.push(DeleteRunFailure {
                        run_id: id,
                        code: "HISTORY_FILESYSTEM",
                        message: format!("{error}. Metadata was kept. Retry deletion."),
                    });
                    continue;
                }
            }
            if let Err(error) = File::open(self.root.join("runs")).and_then(|file| file.sync_all())
            {
                result.failed.push(DeleteRunFailure {
                    run_id: id,
                    code: "HISTORY_FILESYSTEM",
                    message: format!(
                        "{error}. Files may be removed; metadata was kept. Retry deletion."
                    ),
                });
                continue;
            }
            match transaction.execute("DELETE FROM runs WHERE id=?1", [id.to_string()]) {
                Ok(_) => result.deleted.push(id),
                Err(error) => result.failed.push(DeleteRunFailure {
                    run_id: id,
                    code: "HISTORY_DATABASE",
                    message: format!(
                        "{error}. Files were removed; metadata was kept. Retry deletion."
                    ),
                }),
            }
        }
        transaction.commit().map_err(StorageError::DeletionCommit)?;
        Ok(result)
    }

    pub fn get_run(&self, id: Uuid) -> Result<Run> {
        get_run(&self.connection()?, id)
    }

    pub fn list_runs(&self, job_id: Uuid, limit: u32, offset: u32) -> Result<Vec<Run>> {
        self.get_job(job_id)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT * FROM runs WHERE job_id=?1 ORDER BY queued_at DESC,id DESC LIMIT ?2 OFFSET ?3",
        )?;
        Ok(statement
            .query_map(params![job_id.to_string(), limit, offset], read_run)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn start_run(&self, id: Uuid, versions: RuntimeVersions) -> Result<Run> {
        if versions.node.trim().is_empty()
            || versions.lightpanda.trim().is_empty()
            || versions.playwright.trim().is_empty()
        {
            return Err(StorageError::InvalidInput(
                "Runtime versions cannot be empty",
            ));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let run = get_run(&transaction, id)?;
        if run.status != RunStatus::Queued {
            return Err(StorageError::InvalidTransition);
        }
        let active: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE status='running')",
            [],
            |row| row.get(0),
        )?;
        if active {
            return Err(StorageError::Busy);
        }
        let hash = hex::encode(Sha256::digest(fs::read(&run.job.entry_path)?));
        transaction.execute("UPDATE runs SET status='running',started_at=?2,entry_hash=?3,runtime_versions=?4 WHERE id=?1", params![id.to_string(), timestamp()?.max(run.queued_at), hash, serde_json::to_string(&versions)?])?;
        let started = get_run(&transaction, id)?;
        transaction.commit()?;
        Ok(started)
    }

    pub fn finish_run(&self, id: Uuid, completion: RunCompletion) -> Result<Run> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let run = get_run(&transaction, id)?;
        if !matches!(run.status, RunStatus::Queued | RunStatus::Running)
            || (run.status == RunStatus::Queued
                && matches!(completion, RunCompletion::Succeeded(_)))
        {
            return Err(StorageError::InvalidTransition);
        }
        let directory = self.root.join("runs").join(id.to_string());
        for name in ["stdout.log", "stderr.log"] {
            read_artifact(&directory.join(name))?.sync_all()?;
        }
        let (status, failure, count, records) = match completion {
            RunCompletion::Succeeded(records) => (
                "succeeded",
                None,
                Some(i64::try_from(records.len()).map_err(|_| StorageError::InvalidArtifact)?),
                Some(records),
            ),
            RunCompletion::Failed(failure) => {
                if failure.code.trim().is_empty() || failure.message.trim().is_empty() {
                    return Err(StorageError::InvalidInput(
                        "Failure needs a code and message",
                    ));
                }
                ("failed", Some(failure), None, None)
            }
            RunCompletion::Cancelled => (
                "cancelled",
                Some(RunFailure {
                    code: "RUN_CANCELLED".into(),
                    message: "Run cancelled".into(),
                }),
                None,
                None,
            ),
            RunCompletion::AppExit => (
                "cancelled",
                Some(RunFailure {
                    code: "RUN_SHUTDOWN".into(),
                    message: "Run cancelled because the app closed".into(),
                }),
                None,
                None,
            ),
            RunCompletion::Interrupted => (
                "interrupted",
                Some(RunFailure {
                    code: "RUN_INTERRUPTED".into(),
                    message: "Run interrupted".into(),
                }),
                None,
                None,
            ),
        };
        let result_path = directory.join("results.json");
        if let Some(records) = &records {
            let mut temporary = NamedTempFile::new_in(&directory)?;
            serde_json::to_writer(&mut temporary, records)?;
            temporary.as_file().sync_all()?;
            temporary
                .persist_noclobber(&result_path)
                .map_err(|error| error.error)?;
            if let Err(error) = File::open(&directory).and_then(|file| file.sync_all()) {
                cleanup_result(&result_path);
                return Err(error.into());
            }
        }
        let saved = (|| -> Result<Run> {
            transaction.execute(
                "UPDATE runs SET status=?2,finished_at=?3,failure=?4,record_count=?5 WHERE id=?1",
                params![
                    id.to_string(),
                    status,
                    timestamp()?.max(run.started_at.unwrap_or(run.queued_at)),
                    failure.as_ref().map(serde_json::to_string).transpose()?,
                    count
                ],
            )?;
            let finished = get_run(&transaction, id)?;
            transaction.commit()?;
            Ok(finished)
        })();
        match saved {
            Ok(run) => Ok(run),
            Err(error) => {
                if records.is_some() {
                    cleanup_result(&result_path);
                }
                Err(error)
            }
        }
    }

    /// Opens an append-only log handle. The caller closes writers before finishing the run.
    pub fn open_log(&self, id: Uuid, stream: LogStream) -> Result<File> {
        let run = self.get_run(id)?;
        if !matches!(run.status, RunStatus::Queued | RunStatus::Running) {
            return Err(StorageError::InvalidTransition);
        }
        let path = self.log_path(id, stream);
        read_artifact(&path)?;
        Ok(OpenOptions::new().append(true).open(path)?)
    }

    pub fn read_log(&self, id: Uuid, stream: LogStream) -> Result<Vec<u8>> {
        self.get_run(id)?;
        let mut file = read_artifact(&self.log_path(id, stream))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes)?;
        Ok(bytes)
    }

    pub fn read_log_excerpt(&self, id: Uuid, stream: LogStream) -> Result<(String, bool)> {
        self.get_run(id)?;
        let file = read_artifact(&self.log_path(id, stream))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, 65_537), &mut bytes)?;
        let truncated = bytes.len() > 65_536;
        bytes.truncate(65_536);
        Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
    }

    /// Reads only datasets whose success state has committed in SQLite.
    pub fn read_results(&self, id: Uuid) -> Result<Records> {
        let run = self.get_run(id)?;
        if run.status != RunStatus::Succeeded {
            return Err(StorageError::InvalidTransition);
        }
        let file = read_artifact(
            &self
                .root
                .join("runs")
                .join(id.to_string())
                .join("results.json"),
        )?;
        let records: Records =
            serde_json::from_reader(file).map_err(|_| StorageError::InvalidArtifact)?;
        if i64::try_from(records.len()).ok() != run.record_count {
            return Err(StorageError::InvalidArtifact);
        }
        Ok(records)
    }

    fn log_path(&self, id: Uuid, stream: LogStream) -> PathBuf {
        self.root
            .join("runs")
            .join(id.to_string())
            .join(match stream {
                LogStream::Stdout => "stdout.log",
                LogStream::Stderr => "stderr.log",
            })
    }
}

fn history_candidates(connection: &Connection, target: HistoryTarget) -> Result<Vec<Uuid>> {
    let (sql, id) = match target {
        HistoryTarget::Run(id) => ("SELECT id,status FROM runs WHERE id=?1", id),
        HistoryTarget::Job(id) => {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1)",
                [id.to_string()],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(StorageError::NotFound);
            }
            (
                "SELECT id,status FROM runs WHERE job_id=?1 ORDER BY queued_at,id",
                id,
            )
        }
    };
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query([id.to_string()])?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next()? {
        let status: String = row.get("status")?;
        if matches!(status.as_str(), "queued" | "running") {
            return Err(StorageError::HistoryActive);
        }
        ids.push(read_id(row, "id")?);
    }
    if ids.is_empty() && matches!(target, HistoryTarget::Run(_)) {
        return Err(StorageError::NotFound);
    }
    Ok(ids)
}

fn timestamp() -> Result<i64> {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StorageError::Clock)?
        .as_millis();
    i64::try_from(milliseconds).map_err(|_| StorageError::Clock)
}

fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(StorageError::InvalidInput(
            "Storage directory cannot be a symlink",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn read_artifact(path: &Path) -> Result<File> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            StorageError::MissingArtifact(path.to_path_buf())
        } else {
            StorageError::Io(error)
        }
    })?;
    if !metadata.is_file() {
        return Err(StorageError::InvalidArtifact);
    }
    Ok(File::open(path)?)
}

fn cleanup_result(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        log::error!("STORAGE_CLEANUP: Failed to remove uncommitted result: {error}");
    }
}

fn read_id(row: &Row<'_>, column: &str) -> rusqlite::Result<Uuid> {
    let index = row.as_ref().column_index(column)?;
    let value: String = row.get(index)?;
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn read_json<T: DeserializeOwned>(row: &Row<'_>, column: &str) -> rusqlite::Result<T> {
    let index = row.as_ref().column_index(column)?;
    let value: String = row.get(index)?;
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn read_schedule(row: &Row<'_>) -> rusqlite::Result<crate::schedule::SavedSchedule> {
    Ok(crate::schedule::SavedSchedule {
        job_id: read_id(row, "job_id")?,
        revision: read_id(row, "revision")?,
        schedule: serde_json::from_str(&row.get::<_, String>("definition")?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        anchor_at: row.get("anchor_at")?,
        last_occurrence: row.get("last_occurrence")?,
    })
}

fn read_job(row: &Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: read_id(row, "id")?,
        name: row.get("name")?,
        profile_id: row
            .get::<_, Option<String>>("profile_id")?
            .map(|value| {
                Uuid::parse_str(&value).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?,
        project_path: PathBuf::from(row.get::<_, String>("project_path")?),
        entry_path: PathBuf::from(row.get::<_, String>("entry_path")?),
        input: row.get("input")?,
        timeout_ms: row.get("timeout_ms")?,
        created_at: row.get("created_at")?,
    })
}

fn read_run(row: &Row<'_>) -> rusqlite::Result<Run> {
    let status: String = row.get("status")?;
    let status = serde_json::from_value(Value::String(status)).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Run {
        id: read_id(row, "id")?,
        job: read_json(row, "job_snapshot")?,
        status,
        queued_at: row.get("queued_at")?,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        entry_hash: row.get("entry_hash")?,
        runtime_versions: if row.get::<_, Option<String>>("runtime_versions")?.is_some() {
            Some(read_json(row, "runtime_versions")?)
        } else {
            None
        },
        failure: if row.get::<_, Option<String>>("failure")?.is_some() {
            Some(read_json(row, "failure")?)
        } else {
            None
        },
        record_count: row.get("record_count")?,
    })
}

fn get_run(connection: &Connection, id: Uuid) -> Result<Run> {
    connection
        .query_row("SELECT * FROM runs WHERE id=?1", [id.to_string()], read_run)
        .optional()?
        .ok_or(StorageError::NotFound)
}
