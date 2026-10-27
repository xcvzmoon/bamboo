use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use crate::{
    runner::{RunSupervisor, SupervisorError},
    storage::{
        DeleteRunResult, HistoryTarget, Run, RunCompletion, RunFailure, Storage, StorageError,
    },
};

type Result<T> = std::result::Result<T, SupervisorError>;

#[derive(Default)]
struct QueueState {
    active: Option<Run>,
    pending: VecDeque<Run>,
    worker_running: bool,
    closed: bool,
}

#[derive(Serialize)]
pub struct QueueSnapshot {
    pub active: Option<Run>,
    pub pending: Vec<Run>,
    pub closed: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ScheduleDispatchDecision {
    Launched { run_id: Uuid },
    SkipBusy,
    SkipMissed,
    Stale,
    Closed,
}

pub struct RunQueue {
    storage: Storage,
    supervisor: Arc<RunSupervisor>,
    state: Mutex<QueueState>,
    idle: Notify,
    manual_requests: AtomicUsize,
}

impl RunQueue {
    pub fn new(storage: Storage, supervisor: Arc<RunSupervisor>) -> Arc<Self> {
        Arc::new(Self {
            storage,
            supervisor,
            state: Mutex::new(QueueState::default()),
            idle: Notify::new(),
            manual_requests: AtomicUsize::new(0),
        })
    }

    pub async fn enqueue(self: &Arc<Self>, job_id: Uuid, input: Value) -> Result<Run> {
        self.manual_requests.fetch_add(1, Ordering::SeqCst);
        let _request = ManualRequest(&self.manual_requests);
        let mut state = self.state.lock().await;
        if state.closed {
            return Err(SupervisorError::Failure {
                code: "RUN_QUEUE_CLOSED",
                message: "The run queue is closed".into(),
            });
        }
        let storage = self.storage.clone();
        let run = tokio::task::spawn_blocking(move || storage.queue_run(job_id, &input)).await??;
        state.pending.push_back(run.clone());
        if !state.worker_running {
            state.worker_running = true;
            let queue = self.clone();
            tokio::spawn(async move {
                queue.drive().await;
            });
        }
        Ok(run)
    }

    pub async fn dispatch_schedule(
        self: &Arc<Self>,
        job_id: Uuid,
        revision: Uuid,
        occurrence: i64,
    ) -> Result<ScheduleDispatchDecision> {
        let mut state = self.state.lock().await;
        if state.closed {
            return Ok(ScheduleDispatchDecision::Closed);
        }
        let storage = self.storage.clone();
        if state.active.is_some()
            || !state.pending.is_empty()
            || state.worker_running
            || self.manual_requests.load(Ordering::SeqCst) > 0
        {
            let claimed = tokio::task::spawn_blocking(move || {
                storage.record_schedule_occurrence(job_id, revision, occurrence)
            })
            .await??;
            return Ok(if claimed {
                ScheduleDispatchDecision::SkipBusy
            } else {
                ScheduleDispatchDecision::Stale
            });
        }
        let run = tokio::task::spawn_blocking(move || {
            storage.queue_scheduled_run(job_id, revision, occurrence)
        })
        .await??;
        let Some(run) = run else {
            return Ok(ScheduleDispatchDecision::Stale);
        };
        let id = run.id;
        self.supervisor.reserve(id);
        state.active = Some(run.clone());
        state.worker_running = true;
        let queue = self.clone();
        tokio::spawn(async move {
            queue.execute_run(run).await;
            queue.drive().await;
        });
        Ok(ScheduleDispatchDecision::Launched { run_id: id })
    }

    pub async fn delete_history(&self, target: HistoryTarget) -> Result<DeleteRunResult> {
        let state = self.state.lock().await;
        let matches = |run: &Run| match target {
            HistoryTarget::Run(id) => run.id == id,
            HistoryTarget::Job(id) => run.job.id == id,
        };
        if state.active.as_ref().is_some_and(matches) || state.pending.iter().any(matches) {
            return Err(StorageError::HistoryActive.into());
        }
        let storage = self.storage.clone();
        let supervisor = self.supervisor.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<DeleteRunResult> {
            for id in storage.deletion_candidates(target)? {
                supervisor.publish_profile_update(id)?;
            }
            storage.delete_history(target).map_err(Into::into)
        })
        .await?;
        drop(state);
        result
    }

    pub async fn snapshot(&self) -> Result<QueueSnapshot> {
        let state = self.state.lock().await;
        let active_id = state.active.as_ref().map(|run| run.id);
        let pending = state.pending.iter().cloned().collect();
        let closed = state.closed;
        let storage = self.storage.clone();
        let active = tokio::task::spawn_blocking(move || {
            active_id.map(|id| storage.get_run(id)).transpose()
        })
        .await??;
        Ok(QueueSnapshot {
            active,
            pending,
            closed,
        })
    }

    pub async fn cancel(&self, id: Uuid) -> Result<bool> {
        let mut state = self.state.lock().await;
        if let Some(index) = state.pending.iter().position(|run| run.id == id) {
            let storage = self.storage.clone();
            tokio::task::spawn_blocking(move || storage.finish_run(id, RunCompletion::Cancelled))
                .await??;
            state.pending.remove(index);
            return Ok(true);
        }
        if state.active.as_ref().is_some_and(|run| run.id == id) {
            return Ok(self.supervisor.cancel(id));
        }
        Ok(false)
    }

    pub async fn shutdown(&self) -> Result<()> {
        let mut first_error = None;
        {
            let mut state = self.state.lock().await;
            state.closed = true;
            while let Some(run) = state.pending.pop_front() {
                let id = run.id;
                let storage = self.storage.clone();
                let result = tokio::task::spawn_blocking(move || {
                    storage.finish_run(id, RunCompletion::AppExit)
                })
                .await
                .map_err(SupervisorError::from)
                .and_then(|result| result.map_err(Into::into));
                if let Err(error) = result {
                    first_error.get_or_insert(error);
                }
            }
        }
        self.supervisor.shutdown().await;
        loop {
            let finished = self.idle.notified();
            tokio::pin!(finished);
            finished.as_mut().enable();
            if !self.state.lock().await.worker_running {
                break;
            }
            finished.await;
        }
        first_error.map_or(Ok(()), Err)
    }

    async fn drive(self: Arc<Self>) {
        loop {
            let run = {
                let mut state = self.state.lock().await;
                let run = if state.closed {
                    None
                } else {
                    state.pending.pop_front()
                };
                let Some(run) = run else {
                    state.worker_running = false;
                    self.idle.notify_waiters();
                    return;
                };
                self.supervisor.reserve(run.id);
                state.active = Some(run.clone());
                run
            };
            self.execute_run(run).await;
        }
    }

    async fn execute_run(&self, run: Run) {
        let id = run.id;
        if let Err(error) = self.supervisor.run(run).await {
            let storage = self.storage.clone();
            let completion = if self.state.lock().await.closed {
                RunCompletion::AppExit
            } else {
                RunCompletion::Failed(RunFailure {
                    code: error.code().into(),
                    message: error.to_string(),
                })
            };
            if let Err(error) =
                tokio::task::spawn_blocking(move || storage.finish_run(id, completion))
                    .await
                    .map_err(SupervisorError::from)
                    .and_then(|result| result.map_err(Into::into))
            {
                log::error!("RUN_QUEUE_STORAGE: {error}");
                self.state.lock().await.closed = true;
            }
        }
        self.state.lock().await.active = None;
    }
}

struct ManualRequest<'a>(&'a AtomicUsize);

impl Drop for ManualRequest<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        profiles::Profiles,
        runner::RunSupervisor,
        schedule::{Clock, ClockReading, IntervalUnit, Schedule, ScheduleError},
    };
    use std::{fs, path::PathBuf, time::Duration};

    struct FixedClock;

    impl Clock for FixedClock {
        fn read(&self) -> std::result::Result<ClockReading, ScheduleError> {
            Ok(ClockReading {
                now: "2026-10-27T08:00:00Z".parse().unwrap(),
                time_zone: jiff::tz::TimeZone::UTC,
            })
        }
    }

    #[tokio::test]
    async fn waiting_manual_request_takes_priority_over_an_earlier_scheduled_lock_waiter() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let entry = root.join("job.ts");
        fs::write(&entry, "export default async () => []").unwrap();
        let storage = Storage::open(&root.join("storage")).unwrap();
        let job = storage
            .create_job(
                "Priority",
                root,
                &entry,
                &Value::Null,
                Duration::from_secs(10),
            )
            .unwrap();
        let schedule = storage
            .save_schedule(
                job.id,
                Schedule::Interval {
                    every: 1,
                    unit: IntervalUnit::Minutes,
                },
                &FixedClock,
            )
            .unwrap();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner");
        let profiles = Arc::new(Profiles::open(root.join("profiles"), assets.clone()).unwrap());
        let supervisor = Arc::new(
            RunSupervisor::new(
                storage.clone(),
                assets,
                root.join("configuration"),
                profiles,
            )
            .unwrap(),
        );
        let queue = RunQueue::new(storage.clone(), supervisor);
        let locked = queue.state.lock().await;
        let scheduled_queue = queue.clone();
        let scheduled = tokio::spawn(async move {
            scheduled_queue
                .dispatch_schedule(job.id, schedule.saved.revision, schedule.next_at.unwrap())
                .await
        });
        tokio::task::yield_now().await;
        let manual_queue = queue.clone();
        let manual = tokio::spawn(async move { manual_queue.enqueue(job.id, Value::Null).await });
        while queue.manual_requests.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        drop(locked);
        assert_eq!(
            scheduled.await.unwrap().unwrap(),
            ScheduleDispatchDecision::SkipBusy
        );
        let manual = manual.await.unwrap().unwrap();
        queue.shutdown().await.unwrap();
        let runs = storage.list_runs(job.id, 20, 0).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, manual.id);
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;
    use crate::{profiles::Profiles, storage::RuntimeVersions};
    use std::{fs, path::PathBuf, time::Duration};

    #[tokio::test]
    async fn finalizing_work_cannot_be_deleted_even_after_success_metadata_is_published() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let entry = root.join("job.ts");
        fs::write(&entry, "export default async () => []").unwrap();
        let storage = Storage::open(&root.join("storage")).unwrap();
        let job = storage
            .create_job(
                "Finalizing",
                root,
                &entry,
                &Value::Null,
                Duration::from_secs(10),
            )
            .unwrap();
        let run = storage.queue_run(job.id, &Value::Null).unwrap();
        storage
            .start_run(
                run.id,
                RuntimeVersions {
                    node: "fixture".into(),
                    lightpanda: "fixture".into(),
                    playwright: "fixture".into(),
                },
            )
            .unwrap();
        let run = storage
            .finish_run(run.id, RunCompletion::Succeeded(vec![]))
            .unwrap();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner");
        let profiles = Arc::new(Profiles::open(root.join("profiles"), assets.clone()).unwrap());
        let supervisor = Arc::new(
            RunSupervisor::new(storage.clone(), assets, root.join("config"), profiles).unwrap(),
        );
        let queue = RunQueue::new(storage.clone(), supervisor);
        queue.state.lock().await.active = Some(run.clone());
        for target in [HistoryTarget::Run(run.id), HistoryTarget::Job(job.id)] {
            let error = queue.delete_history(target).await.unwrap_err();
            assert_eq!(error.code(), "HISTORY_ACTIVE");
        }
        assert!(storage.read_results(run.id).is_ok());
        queue.state.lock().await.active = None;
        assert_eq!(
            queue
                .delete_history(HistoryTarget::Run(run.id))
                .await
                .unwrap()
                .deleted,
            vec![run.id]
        );
        assert_eq!(storage.get_job(job.id).unwrap().id, job.id);
        queue.shutdown().await.unwrap();
    }
}
