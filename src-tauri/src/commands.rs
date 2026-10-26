use std::{path::PathBuf, sync::Arc};

use serde::Serialize;
use serde_json::Value;
use tauri::{Emitter, State};
use uuid::Uuid;

use crate::{
    runner::{RunSupervisor, RuntimePaths, SupervisorError},
    storage::Run,
};

#[derive(Serialize)]
pub struct CommandError {
    code: String,
    message: String,
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
    app: tauri::AppHandle,
    supervisor: State<'_, Arc<RunSupervisor>>,
    job_id: Uuid,
    input: Value,
) -> Result<Run, CommandError> {
    let run = supervisor.run(job_id, input).await?;
    if let Err(error) = app.emit("run-updated", &run) {
        log::error!("RUN_EVENT: {error}");
    }
    Ok(run)
}
