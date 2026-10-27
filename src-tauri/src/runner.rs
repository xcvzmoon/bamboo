use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::Arc,
    time::Duration,
};

use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::TempDir;
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::{Mutex, watch},
    time::{Instant, sleep, timeout},
};
use uuid::Uuid;

use crate::profiles::{ProfileError, Profiles};
use crate::storage::{
    LogStream, Records, Run, RunCompletion, RunFailure, RuntimeVersions, Storage, StorageError,
};

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("{0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Profile(#[from] ProfileError),
    #[error("RUN_IO: {0}")]
    Io(#[from] io::Error),
    #[error("RUN_JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("RUN_TASK: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("{code}: {message}")]
    Failure { code: &'static str, message: String },
}
impl SupervisorError {
    pub fn code(&self) -> &str {
        match self {
            Self::Storage(error) => error.code(),
            Self::Profile(error) => error.code(),
            Self::Io(_) => "RUN_IO",
            Self::Json(_) => "RUN_JSON",
            Self::Task(_) => "RUN_TASK",
            Self::Failure { code, .. } => code,
        }
    }
}

type Result<T> = std::result::Result<T, SupervisorError>;
fn failure(code: &'static str, message: impl Into<String>) -> SupervisorError {
    SupervisorError::Failure {
        code,
        message: message.into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePaths {
    pub node: PathBuf,
    pub lightpanda: PathBuf,
}
impl RuntimePaths {
    pub fn validate(&self) -> Result<Self> {
        fn executable(path: &Path) -> Result<PathBuf> {
            if !path.is_absolute() {
                return Err(failure("RUNTIME_PATH", "Runtime paths must be absolute"));
            }
            let path = path.to_path_buf();
            let metadata =
                fs::metadata(&path).map_err(|error| failure("RUNTIME_PATH", error.to_string()))?;
            if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
                return Err(failure(
                    "RUNTIME_PATH",
                    "Runtime path is not an executable file",
                ));
            }
            Ok(path)
        }
        Ok(Self {
            node: executable(&self.node)?,
            lightpanda: executable(&self.lightpanda)?,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest<'a> {
    version: u8,
    project_directory: &'a Path,
    entry_file: &'a Path,
    endpoint: &'a str,
    input: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<bool>,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
enum RunOutcome {
    Success { version: u8, records: Records },
    Failure { version: u8, error: OutcomeFailure },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutcomeFailure {
    code: OutcomeFailureCode,
    message: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OutcomeFailureCode {
    RequestInvalid,
    JobImport,
    JobExport,
    BrowserConnect,
    JobExecution,
    JobResult,
    BrowserCleanup,
    AuthenticationRequired,
    ProfileState,
}

impl OutcomeFailureCode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::RequestInvalid => "REQUEST_INVALID",
            Self::JobImport => "JOB_IMPORT",
            Self::JobExport => "JOB_EXPORT",
            Self::BrowserConnect => "BROWSER_CONNECT",
            Self::JobExecution => "JOB_EXECUTION",
            Self::JobResult => "JOB_RESULT",
            Self::BrowserCleanup => "BROWSER_CLEANUP",
            Self::AuthenticationRequired => "AUTHENTICATION_REQUIRED",
            Self::ProfileState => "PROFILE_STATE",
        }
    }
}

#[derive(Clone, Copy)]
struct RunControl {
    id: Uuid,
    cancelled: bool,
}

pub struct RunSupervisor {
    storage: Storage,
    profiles: Arc<Profiles>,
    runner_directory: PathBuf,
    config_path: PathBuf,
    admission: Mutex<()>,
    shutdown: watch::Sender<bool>,
    control: watch::Sender<Option<RunControl>>,
}

impl RunSupervisor {
    pub fn new(
        storage: Storage,
        runner_directory: PathBuf,
        config_directory: PathBuf,
        profiles: Arc<Profiles>,
    ) -> Result<Self> {
        if !runner_directory.is_absolute()
            || !runner_directory.join("main.ts").is_file()
            || !runner_directory.join("guard.ts").is_file()
        {
            return Err(failure("RUNNER_ASSETS", "Packaged runner entry is missing"));
        }
        fs::create_dir_all(&config_directory)?;
        if !fs::symlink_metadata(&config_directory)?.is_dir() {
            return Err(failure(
                "RUNTIME_CONFIG",
                "Configuration directory must not be a symlink",
            ));
        }
        fs::set_permissions(&config_directory, fs::Permissions::from_mode(0o700))?;
        let (shutdown, _) = watch::channel(false);
        let (control, _) = watch::channel(None);
        Ok(Self {
            storage,
            profiles,
            runner_directory,
            config_path: config_directory.join("runtimes.json"),
            admission: Mutex::new(()),
            shutdown,
            control,
        })
    }

    pub fn configure_runtimes(&self, paths: RuntimePaths) -> Result<RuntimePaths> {
        let paths = paths.validate()?;
        let parent = self
            .config_path
            .parent()
            .ok_or_else(|| failure("RUNTIME_CONFIG", "Missing configuration parent"))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(&mut file, &paths)?;
        file.as_file().sync_all()?;
        file.persist(&self.config_path)
            .map_err(|error| error.error)?;
        File::open(parent)?.sync_all()?;
        Ok(paths)
    }

    pub(crate) fn publish_profile_update(&self, run_id: Uuid) -> Result<()> {
        self.profiles
            .publish(run_id, &self.storage)
            .map_err(Into::into)
    }

    pub fn runtime_paths(&self) -> Result<RuntimePaths> {
        let file = File::open(&self.config_path)
            .map_err(|error| failure("RUNTIME_CONFIG", error.to_string()))?;
        let paths: RuntimePaths = serde_json::from_reader(file)
            .map_err(|error| failure("RUNTIME_CONFIG", error.to_string()))?;
        paths.validate()
    }

    pub fn active_run(&self) -> Result<Option<Run>> {
        let id = self.control.borrow().map(|control| control.id);
        id.map(|id| self.storage.get_run(id))
            .transpose()
            .map_err(Into::into)
    }

    pub fn cancel(&self, id: Uuid) -> bool {
        self.control.send_if_modified(|active| {
            if let Some(control) = active.as_mut()
                && control.id == id
                && !control.cancelled
            {
                control.cancelled = true;
                return true;
            }
            false
        })
    }

    pub(crate) fn reserve(&self, id: Uuid) {
        self.control.send_if_modified(|control| {
            if control.is_some_and(|control| control.id == id) {
                return false;
            }
            *control = Some(RunControl {
                id,
                cancelled: false,
            });
            true
        });
    }

    pub async fn shutdown(&self) {
        self.shutdown.send_replace(true);
        let _guard = self.admission.lock().await;
    }

    pub async fn run(&self, queued: Run) -> Result<Run> {
        let _guard = self
            .admission
            .try_lock()
            .map_err(|_| failure("RUN_BUSY", "Another managed run is active"))?;
        let mut shutdown = self.shutdown.subscribe();
        if *shutdown.borrow() {
            return Err(failure("RUN_SHUTDOWN", "The supervisor is shutting down"));
        }
        let storage = self.storage.clone();
        let id = queued.id;
        self.reserve(id);
        let mut cancellation = self.control.subscribe();
        let profiles = self.profiles.clone();
        let profile_id = queued.job.profile_id;
        let prepared = tokio::task::spawn_blocking(move || -> Result<_> {
            let protocol = storage.create_protocol_directory(id)?;
            fs::set_permissions(protocol.path(), fs::Permissions::from_mode(0o700))?;
            profiles.recover(&storage)?;
            let snapshot = profile_id.map(|id| profiles.snapshot(id)).transpose()?;
            if let Some(snapshot) = &snapshot {
                use std::io::Write;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(protocol.path().join("profile.json"))?;
                file.write_all(&snapshot.state)?;
            }
            Ok((
                protocol,
                storage.open_log(id, LogStream::Stdout)?,
                storage.open_log(id, LogStream::Stderr)?,
                snapshot,
            ))
        })
        .await
        .map_err(SupervisorError::from)
        .and_then(|result| result);
        let completion = match prepared {
            Err(error) => RunCompletion::Failed(RunFailure {
                code: error.code().into(),
                message: error.to_string(),
            }),
            Ok((protocol, stdout, stderr, snapshot)) => {
                let mut active = ActiveRun {
                    children: Vec::new(),
                    guard: self.runner_directory.join("guard.ts"),
                    node: None,
                    starting: None,
                    protocol,
                    stdout,
                    stderr,
                };
                let execution = tokio::select! {
                    biased;
                    _ = async {
                        if !cancellation.borrow().is_some_and(|control| control.cancelled) {
                            let _ = cancellation.changed().await;
                        }
                    } => Err(failure("RUN_CANCELLED", "Run cancelled by user")),
                    result = timeout(Duration::from_millis(queued.job.timeout_ms as u64), self.execute(&queued, &mut active)) => {
                        match result {
                            Ok(result) => result,
                            Err(_) => Err(failure("RUN_TIMEOUT", "Managed run exceeded its timeout")),
                        }
                    }
                    _ = shutdown.changed() => Err(failure("RUN_SHUTDOWN", "App is shutting down")),
                };
                let cleanup = active.stop().await;
                let execution = match cleanup {
                    Ok(()) => execution,
                    Err(error) => Err(error),
                };
                let execution = if let Some(snapshot) = snapshot {
                    let profiles = self.profiles.clone();
                    let candidate = active.protocol.path().join("candidate.json");
                    tokio::task::spawn_blocking(move || -> Result<Records> {
                        if execution.is_ok() {
                            if fs::metadata(&candidate)?.len() > 16 * 1024 * 1024 {
                                return Err(ProfileError::State.into());
                            }
                            let state = zeroize::Zeroizing::new(fs::read(candidate)?);
                            profiles.stage(id, &snapshot, Some(&state))?;
                        } else if execution
                            .as_ref()
                            .is_err_and(|error| error.code() == "AUTHENTICATION_REQUIRED")
                        {
                            profiles.stage(id, &snapshot, None)?;
                        }
                        execution
                    })
                    .await?
                } else {
                    execution
                };
                let completion = match execution {
                    Ok(records) => RunCompletion::Succeeded(records),
                    Err(error) => {
                        use std::io::Write;
                        if let Err(log_error) = writeln!(active.stderr, "{error}") {
                            log::error!("RUN_LOG: {log_error}");
                        }
                        if error.code() == "RUN_CANCELLED" {
                            RunCompletion::Cancelled
                        } else if error.code() == "RUN_SHUTDOWN" {
                            RunCompletion::AppExit
                        } else {
                            RunCompletion::Failed(RunFailure {
                                code: error.code().into(),
                                message: error.to_string(),
                            })
                        }
                    }
                };
                drop(active);
                completion
            }
        };
        let previous = self.control.send_replace(None);
        let completion = if matches!(completion, RunCompletion::Succeeded(_)) && *shutdown.borrow()
        {
            RunCompletion::AppExit
        } else if matches!(completion, RunCompletion::Succeeded(_))
            && previous.is_some_and(|control| control.cancelled)
        {
            RunCompletion::Cancelled
        } else {
            completion
        };
        let storage = self.storage.clone();
        let profiles = self.profiles.clone();
        tokio::task::spawn_blocking(move || -> Result<Run> {
            let run = storage.finish_run(id, completion)?;
            if let Err(error) = profiles.publish(id, &storage) {
                log::error!("PROFILE_PUBLICATION: {error}");
            }
            Ok(run)
        })
        .await?
    }

    async fn execute(&self, queued: &Run, active: &mut ActiveRun) -> Result<Records> {
        let paths = self.runtime_paths()?;
        active.node = Some(paths.node.clone());
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct NodeInfo {
            node: String,
            playwright: String,
        }
        let mut node = Command::new(&paths.node);
        let expression = format!(
            "JSON.stringify({{node:process.versions.node,playwright:require({}).version}})",
            serde_json::to_string(
                &self
                    .runner_directory
                    .join("node_modules/playwright-core/package.json")
            )?
        );
        node.args(["-p", &expression])
            .current_dir(&queued.job.project_path);
        let info: NodeInfo = serde_json::from_slice(&active.version_output(node).await?)
            .map_err(|error| failure("RUNTIME_NODE", error.to_string()))?;
        if info
            .node
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .is_none_or(|major| major < 26)
        {
            return Err(failure("RUNTIME_NODE", "Node 26 or newer is required"));
        }
        let mut browser_version = Command::new(&paths.lightpanda);
        browser_version.arg("version");
        let lightpanda = String::from_utf8(active.version_output(browser_version).await?)
            .map_err(|error| failure("RUNTIME_BROWSER", error.to_string()))?;
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut browser = Command::new(&paths.lightpanda);
        browser
            .args(["serve", "--host", "127.0.0.1", "--port", &port.to_string()])
            .env("LIGHTPANDA_DISABLE_TELEMETRY", "true");
        active.spawn_logged(browser)?;
        let browser_index = active.children.len() - 1;
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(300))
            .build()
            .map_err(|error| failure("BROWSER_STARTUP", error.to_string()))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if active.children[browser_index].child.try_wait()?.is_some() {
                return Err(failure(
                    "BROWSER_EXIT",
                    "Lightpanda exited before readiness",
                ));
            }
            if let Ok(response) = client.get(format!("{endpoint}/json/version")).send().await
                && response.status().is_success()
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(failure(
                    "BROWSER_STARTUP",
                    "Lightpanda did not become ready within ten seconds",
                ));
            }
            sleep(Duration::from_millis(50)).await;
        }
        let storage = self.storage.clone();
        let id = queued.id;
        let versions = RuntimeVersions {
            node: info.node,
            lightpanda: lightpanda.trim().into(),
            playwright: info.playwright,
        };
        active.starting = Some(tokio::task::spawn_blocking(move || {
            storage.start_run(id, versions)
        }));
        let started = active
            .starting
            .as_mut()
            .ok_or_else(|| failure("RUN_STATE", "Missing start task"))?
            .await;
        active.starting.take();
        started??;
        let request = RunRequest {
            version: 1,
            project_directory: &queued.job.project_path,
            entry_file: &queued.job.entry_path,
            endpoint: &endpoint,
            input: &queued.job.input,
            profile: queued.job.profile_id.map(|_| true),
        };
        let request_path = active.protocol.path().join("request.json");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(request_path)?;
        serde_json::to_writer(file, &request)?;
        let mut node = Command::new(paths.node);
        node.arg(self.runner_directory.join("main.ts"))
            .arg(active.protocol.path())
            .current_dir(&queued.job.project_path);
        active.spawn_logged(node)?;
        let node_index = active.children.len() - 1;
        let status = loop {
            if active.children[browser_index].child.try_wait()?.is_some() {
                return Err(failure(
                    "BROWSER_EXIT",
                    "Lightpanda exited during job execution",
                ));
            }
            if let Some(status) = active.children[node_index].child.try_wait()? {
                break status;
            }
            sleep(Duration::from_millis(25)).await;
        };
        read_outcome(active.protocol.path(), status)
    }
}

struct ActiveRun {
    guard: PathBuf,
    node: Option<PathBuf>,
    children: Vec<OwnedProcess>,
    starting: Option<tokio::task::JoinHandle<std::result::Result<Run, StorageError>>>,
    protocol: TempDir,
    stdout: File,
    stderr: File,
}
impl ActiveRun {
    fn guarded(&self, command: Command) -> Result<Command> {
        let node = self
            .node
            .as_ref()
            .ok_or_else(|| failure("RUN_PROCESS", "Missing guard runtime"))?;
        let original = command.as_std();
        let mut guarded = Command::new(node);
        guarded
            .arg(&self.guard)
            .arg(original.get_program())
            .args(original.get_args());
        if let Some(directory) = original.get_current_dir() {
            guarded.current_dir(directory);
        }
        for (name, value) in original.get_envs() {
            if let Some(value) = value {
                guarded.env(name, value);
            } else {
                guarded.env_remove(name);
            }
        }
        Ok(guarded)
    }

    fn spawn_logged(&mut self, command: Command) -> Result<()> {
        let mut command = self.guarded(command)?;
        command
            .stdout(Stdio::from(self.stdout.try_clone()?))
            .stderr(Stdio::from(self.stderr.try_clone()?));
        self.children.push(OwnedProcess::spawn(command)?);
        Ok(())
    }

    async fn version_output(&mut self, command: Command) -> Result<Vec<u8>> {
        let mut command = self.guarded(command)?;
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::from(self.stderr.try_clone()?));
        self.children.push(OwnedProcess::spawn(command)?);
        let process = self
            .children
            .last_mut()
            .ok_or_else(|| failure("RUN_PROCESS", "Missing version process"))?;
        let result = timeout(Duration::from_secs(3), async {
            let output = process
                .child
                .stdout
                .take()
                .ok_or_else(|| failure("RUNTIME_VERSION", "Missing version output"))?;
            let mut bytes = Vec::new();
            output.take(4097).read_to_end(&mut bytes).await?;
            if bytes.len() > 4096 {
                return Err(failure(
                    "RUNTIME_VERSION",
                    "Version output exceeded 4096 bytes",
                ));
            }
            let status = process.child.wait().await?;
            if !status.success() {
                return Err(failure(
                    "RUNTIME_VERSION",
                    "Version command exited unsuccessfully",
                ));
            }
            Ok(bytes)
        })
        .await
        .map_err(|_| failure("RUNTIME_VERSION", "Version command timed out"))?;
        let cleanup = process.stop().await;
        self.children.pop();
        let bytes = result?;
        cleanup?;
        Ok(bytes)
    }

    async fn stop(&mut self) -> Result<()> {
        let mut first = None;
        for child in self.children.iter_mut().rev() {
            if let Err(error) = child.stop().await {
                log::error!("RUN_CLEANUP: {error}");
                if first.is_none() {
                    first = Some(error);
                }
            }
        }
        self.children.clear();
        if let Some(starting) = self.starting.take()
            && let Err(error) = starting
                .await
                .map_err(SupervisorError::from)
                .and_then(|result| result.map_err(Into::into))
            && first.is_none()
        {
            first = Some(error);
        }
        first.map_or(Ok(()), Err)
    }
}

struct OwnedProcess {
    child: Child,
    group: Option<Pid>,
}
impl OwnedProcess {
    fn spawn(mut command: Command) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        let child = command.spawn()?;
        let id = child
            .id()
            .ok_or_else(|| failure("RUN_PROCESS", "Child has no process ID"))?;
        let group = Pid::from_raw(
            i32::try_from(id)
                .map_err(|_| failure("RUN_PROCESS", "Child process ID is out of range"))?,
        );
        Ok(Self {
            child,
            group: Some(group),
        })
    }

    async fn stop(&mut self) -> Result<()> {
        let Some(group) = self.group else {
            return Ok(());
        };
        signal_group(group, Signal::SIGTERM)?;
        let owner_pipe = self.child.stdin.take();
        let result = timeout(Duration::from_secs(2), self.child.wait()).await;
        signal_group(group, Signal::SIGKILL)?;
        drop(owner_pipe);
        self.group = None;
        if let Ok(status) = result {
            status?;
        } else {
            self.child.wait().await?;
        }
        Ok(())
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if let Some(group) = self.group
            && let Err(error) = signal_group(group, Signal::SIGKILL)
        {
            log::error!("RUN_CLEANUP: {error}");
        }
    }
}
fn signal_group(group: Pid, signal: Signal) -> Result<()> {
    match killpg(group, signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(Errno::EPERM) if nix::sys::signal::kill(group, None) == Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(failure("RUN_CLEANUP", error.to_string())),
    }
}

fn read_outcome(directory: &Path, status: ExitStatus) -> Result<Records> {
    let path = directory.join("outcome.json");
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| {
            failure(
                if error.kind() == io::ErrorKind::NotFound {
                    "OUTCOME_MISSING"
                } else {
                    "OUTCOME_INVALID"
                },
                error.to_string(),
            )
        })?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != fs::metadata(directory)?.uid()
    {
        return Err(failure(
            "OUTCOME_INVALID",
            "Outcome file is not private, regular, and owned by the run user",
        ));
    }
    let outcome: RunOutcome = serde_json::from_reader(file)
        .map_err(|error| failure("OUTCOME_INVALID", error.to_string()))?;
    match outcome {
        RunOutcome::Success {
            version: 1,
            records,
        } if status.success() => Ok(records),
        RunOutcome::Failure { version: 1, error } if !error.message.trim().is_empty() => {
            Err(failure(error.code.as_str(), error.message))
        }
        RunOutcome::Success { version: 1, .. } => Err(failure(
            "RUNNER_EXIT",
            "Runner exited unsuccessfully despite a success outcome",
        )),
        _ => Err(failure(
            "OUTCOME_INVALID",
            "Unsupported outcome version or invalid failure",
        )),
    }
}
