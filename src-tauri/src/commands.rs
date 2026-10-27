use std::{path::PathBuf, sync::Arc, time::Duration};

use serde::Serialize;
use serde_json::Value;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use crate::{
    queue::{QueueSnapshot, RunQueue},
    results::{self, ExportFormat, ResultsError, RunDetails},
    runner::{RunSupervisor, RuntimePaths, SupervisorError},
    storage::{Job, Run, Storage, StorageError},
};

#[derive(Serialize)]
pub struct CommandError {
    code: String,
    message: String,
}

#[derive(Serialize)]
pub struct ExampleDefaults {
    project_path: PathBuf,
    entry_path: PathBuf,
    node: Option<PathBuf>,
    lightpanda: Option<PathBuf>,
}

#[tauri::command]
pub async fn example_defaults(app: tauri::AppHandle) -> Result<ExampleDefaults, CommandError> {
    let resources = app.path().resource_dir().map_err(|error| CommandError {
        code: "EXAMPLE_PATH".into(),
        message: error.to_string(),
    })?;
    let project_path = resources.join("runner");
    let entry_path = project_path.join("example.ts");
    let mut directories: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    if let Ok(home) = app.path().home_dir() {
        directories.push(home.join(".local/bin"));
        directories.push(home.join(".local/share/vite-plus/bin"));
    }
    directories.push(PathBuf::from("/opt/homebrew/bin"));
    directories.push(PathBuf::from("/usr/local/bin"));
    fn find_executable(name: &str, directories: &[PathBuf]) -> Option<PathBuf> {
        directories
            .iter()
            .map(|directory| directory.join(name))
            .find(|path| {
                use std::os::unix::fs::PermissionsExt;
                path.is_absolute()
                    && std::fs::metadata(path).is_ok_and(|metadata| {
                        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                    })
            })
    }
    Ok(ExampleDefaults {
        project_path,
        entry_path,
        node: find_executable("node", &directories),
        lightpanda: find_executable("lightpanda", &directories),
    })
}

impl From<ResultsError> for CommandError {
    fn from(error: ResultsError) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
pub async fn run_details(
    storage: State<'_, Storage>,
    run_id: Uuid,
) -> Result<RunDetails, CommandError> {
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || results::details(&storage, run_id))
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_runs(
    storage: State<'_, Storage>,
    job_id: Uuid,
    offset: u32,
) -> Result<Vec<Run>, CommandError> {
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || storage.list_runs(job_id, 20, offset))
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn export_run(
    app: tauri::AppHandle,
    storage: State<'_, Storage>,
    run_id: Uuid,
    format: ExportFormat,
) -> Result<Option<PathBuf>, CommandError> {
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || {
        let bytes = results::export_bytes(&storage, run_id, format)?;
        let selected = app
            .dialog()
            .file()
            .set_file_name(format!("bamboo-{run_id}.{}", format.extension()))
            .add_filter(format.extension(), &[format.extension()])
            .blocking_save_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let path = selected.into_path().map_err(|error| CommandError {
            code: "EXPORT_PATH".into(),
            message: error.to_string(),
        })?;
        results::save_export(&path, &bytes)?;
        Ok(Some(path))
    })
    .await
    .map_err(SupervisorError::from)?
}

impl From<StorageError> for CommandError {
    fn from(error: StorageError) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
pub async fn list_jobs(storage: State<'_, Storage>) -> Result<Vec<Job>, CommandError> {
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || storage.list_jobs())
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn register_job(
    storage: State<'_, Storage>,
    name: String,
    project_path: PathBuf,
    entry_path: PathBuf,
    input: Value,
    timeout_ms: u64,
) -> Result<Job, CommandError> {
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || {
        storage.create_job(
            &name,
            &project_path,
            &entry_path,
            &input,
            Duration::from_millis(timeout_ms),
        )
    })
    .await
    .map_err(SupervisorError::from)?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn save_job_settings(
    storage: State<'_, Storage>,
    job_id: Uuid,
    name: String,
    input: Value,
    timeout_ms: i64,
) -> Result<Job, CommandError> {
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || {
        storage.save_job_settings(job_id, &name, &input, timeout_ms)
    })
    .await
    .map_err(SupervisorError::from)?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn select_job_path(
    app: tauri::AppHandle,
    project: bool,
) -> Result<Option<PathBuf>, CommandError> {
    tokio::task::spawn_blocking(move || {
        let dialog = app.dialog().file();
        let selected = if project {
            dialog.blocking_pick_folder()
        } else {
            dialog
                .add_filter("TypeScript job", &["ts", "mts"])
                .blocking_pick_file()
        };
        selected
            .map(|path| path.into_path())
            .transpose()
            .map_err(|error| CommandError {
                code: "JOB_PATH".into(),
                message: error.to_string(),
            })
    })
    .await
    .map_err(SupervisorError::from)?
}
impl From<SupervisorError> for CommandError {
    fn from(error: SupervisorError) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
pub async fn configure_runtimes(
    supervisor: State<'_, Arc<RunSupervisor>>,
    node: PathBuf,
    lightpanda: PathBuf,
) -> Result<RuntimePaths, CommandError> {
    let supervisor = supervisor.inner().clone();
    tokio::task::spawn_blocking(move || {
        supervisor.configure_runtimes(RuntimePaths { node, lightpanda })
    })
    .await
    .map_err(SupervisorError::from)?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn runtime_paths(
    supervisor: State<'_, Arc<RunSupervisor>>,
) -> Result<RuntimePaths, CommandError> {
    let supervisor = supervisor.inner().clone();
    tokio::task::spawn_blocking(move || supervisor.runtime_paths())
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn run_job(
    queue: State<'_, Arc<RunQueue>>,
    job_id: Uuid,
    input: Value,
) -> Result<Run, CommandError> {
    queue
        .inner()
        .enqueue(job_id, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn queue_snapshot(
    queue: State<'_, Arc<RunQueue>>,
) -> Result<QueueSnapshot, CommandError> {
    queue.snapshot().await.map_err(Into::into)
}

#[tauri::command]
pub async fn cancel_run(
    queue: State<'_, Arc<RunQueue>>,
    run_id: Uuid,
) -> Result<bool, CommandError> {
    queue.cancel(run_id).await.map_err(Into::into)
}

impl From<crate::profiles::ProfileError> for CommandError {
    fn from(error: crate::profiles::ProfileError) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
pub async fn list_profiles(
    profiles: State<'_, Arc<crate::profiles::Profiles>>,
) -> Result<Vec<crate::profiles::SessionProfile>, CommandError> {
    let profiles = profiles.inner().clone();
    tokio::task::spawn_blocking(move || profiles.list())
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn profile_capture_status(
    profiles: State<'_, Arc<crate::profiles::Profiles>>,
) -> Result<Option<crate::profiles::CaptureStatus>, CommandError> {
    let profiles = profiles.inner().clone();
    tokio::task::spawn_blocking(move || profiles.status())
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn begin_profile_capture(
    profiles: State<'_, Arc<crate::profiles::Profiles>>,
    supervisor: State<'_, Arc<RunSupervisor>>,
    profile_id: Option<Uuid>,
    name: String,
    login_url: String,
    browser: PathBuf,
) -> Result<crate::profiles::CaptureStatus, CommandError> {
    let profiles = profiles.inner().clone();
    let supervisor = supervisor.inner().clone();
    tokio::task::spawn_blocking(move || {
        let node = supervisor.runtime_paths()?.node;
        profiles
            .begin(profile_id, name, login_url, browser, &node)
            .map_err(Into::into)
    })
    .await
    .map_err(SupervisorError::from)?
}

#[tauri::command]
pub async fn finish_profile_capture(
    profiles: State<'_, Arc<crate::profiles::Profiles>>,
    capture_id: Uuid,
) -> Result<crate::profiles::SessionProfile, CommandError> {
    let profiles = profiles.inner().clone();
    tokio::task::spawn_blocking(move || profiles.finish(capture_id))
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn cancel_profile_capture(
    profiles: State<'_, Arc<crate::profiles::Profiles>>,
    capture_id: Uuid,
) -> Result<(), CommandError> {
    let profiles = profiles.inner().clone();
    tokio::task::spawn_blocking(move || profiles.cancel(Some(capture_id)))
        .await
        .map_err(SupervisorError::from)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_job_profile(
    storage: State<'_, Storage>,
    profiles: State<'_, Arc<crate::profiles::Profiles>>,
    job_id: Uuid,
    profile_id: Option<Uuid>,
) -> Result<Job, CommandError> {
    let storage = storage.inner().clone();
    let profiles = profiles.inner().clone();
    tokio::task::spawn_blocking(move || {
        if let Some(id) = profile_id
            && !profiles.list()?.iter().any(|profile| profile.id == id)
        {
            return Err(crate::profiles::ProfileError::NotFound.into());
        }
        storage
            .set_job_profile(job_id, profile_id)
            .map_err(Into::into)
    })
    .await
    .map_err(SupervisorError::from)?
}

impl From<crate::schedule::ScheduleError> for CommandError {
    fn from(error: crate::schedule::ScheduleError) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
pub async fn save_schedule(
    storage: State<'_, Storage>,
    job_id: Uuid,
    schedule: crate::schedule::Schedule,
) -> Result<crate::schedule::ScheduleDetails, CommandError> {
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || {
        storage.save_schedule(job_id, schedule, &crate::schedule::SystemClock)
    })
    .await
    .map_err(SupervisorError::from)?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn list_schedules(
    storage: State<'_, Storage>,
) -> Result<Vec<crate::schedule::ScheduleDetails>, CommandError> {
    use crate::schedule::Clock;
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || {
        let reading = crate::schedule::SystemClock.read()?;
        storage
            .list_schedules()?
            .iter()
            .map(|saved| saved.details(&reading))
            .collect::<Result<Vec<_>, crate::schedule::ScheduleError>>()
    })
    .await
    .map_err(SupervisorError::from)?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_run(
    queue: State<'_, Arc<RunQueue>>,
    run_id: Uuid,
) -> Result<crate::storage::DeleteRunResult, CommandError> {
    queue
        .delete_history(crate::storage::HistoryTarget::Run(run_id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_job_history(
    queue: State<'_, Arc<RunQueue>>,
    job_id: Uuid,
) -> Result<crate::storage::DeleteRunResult, CommandError> {
    queue
        .delete_history(crate::storage::HistoryTarget::Job(job_id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_schedule(
    storage: State<'_, Storage>,
    job_id: Uuid,
) -> Result<Option<crate::schedule::ScheduleDetails>, CommandError> {
    use crate::schedule::Clock;
    let storage = storage.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<_, crate::schedule::ScheduleError> {
        let reading = crate::schedule::SystemClock.read()?;
        storage
            .get_schedule(job_id)?
            .map(|saved| saved.details(&reading))
            .transpose()
    })
    .await
    .map_err(SupervisorError::from)?
    .map_err(Into::into)
}
