use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use jiff::{Timestamp, tz::TimeZone};
use serde::Serialize;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::{
    queue::{RunQueue, ScheduleDispatchDecision},
    runner::SupervisorError,
    schedule::{Clock, ClockReading, SavedSchedule, Schedule, ScheduleError},
    storage::Storage,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_TICK_GAP: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error(transparent)]
    Schedule(#[from] ScheduleError),
    #[error(transparent)]
    Run(#[from] SupervisorError),
}

type Result<T> = std::result::Result<T, SchedulerError>;

#[derive(Debug, Serialize)]
pub struct ScheduleDispatchEvent {
    pub job_id: Uuid,
    pub occurrence: i64,
    pub decision: ScheduleDispatchDecision,
}

struct Deadline {
    revision: Uuid,
    next: Option<i64>,
}

struct Observation {
    now: Timestamp,
    monotonic: Instant,
    zone: TimeZone,
}

#[derive(Default)]
struct SchedulerState {
    deadlines: HashMap<Uuid, Deadline>,
    previous: Option<Observation>,
    closed: bool,
}

pub struct Scheduler {
    storage: Storage,
    queue: Arc<RunQueue>,
    clock: Arc<dyn Clock + Send + Sync>,
    state: Mutex<SchedulerState>,
    stopping: watch::Sender<bool>,
}

impl Scheduler {
    pub fn new(
        storage: Storage,
        queue: Arc<RunQueue>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Arc<Self> {
        let (stopping, _) = watch::channel(false);
        Arc::new(Self {
            storage,
            queue,
            clock,
            state: Mutex::new(SchedulerState::default()),
            stopping,
        })
    }

    pub async fn serve(&self) {
        let mut stopping = self.stopping.subscribe();
        let mut timer = tokio::time::interval(POLL_INTERVAL);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if *stopping.borrow() {
                break;
            }
            tokio::select! {
                biased;
                _ = stopping.changed() => break,
                _ = timer.tick() => {
                    if let Err(error) = self.tick(Instant::now()).await {
                        log::error!("SCHEDULER_TICK: {error}");
                    }
                }
            }
        }
    }

    pub async fn shutdown(&self) {
        self.stopping.send_replace(true);
        self.state.lock().await.closed = true;
    }

    pub async fn tick(&self, monotonic: Instant) -> Result<Vec<ScheduleDispatchEvent>> {
        let mut state = self.state.lock().await;
        if state.closed || *self.stopping.borrow() {
            return Ok(Vec::new());
        }
        let result = self.tick_locked(&mut state, monotonic).await;
        if result.is_err() {
            state.deadlines.clear();
        }
        result
    }

    async fn tick_locked(
        &self,
        state: &mut SchedulerState,
        monotonic: Instant,
    ) -> Result<Vec<ScheduleDispatchEvent>> {
        let clock = self.clock.clone();
        let storage = self.storage.clone();
        let (reading, schedules) =
            tokio::task::spawn_blocking(move || -> std::result::Result<_, ScheduleError> {
                Ok((clock.read()?, storage.list_schedules()?))
            })
            .await
            .map_err(SupervisorError::from)??;
        let discontinuity = state
            .previous
            .as_ref()
            .map(|previous| discontinuity(previous, &reading, monotonic))
            .transpose()?
            .unwrap_or(true);
        let zone_changed = state
            .previous
            .as_ref()
            .is_some_and(|previous| previous.zone != reading.time_zone);
        let high_water = state
            .previous
            .as_ref()
            .map_or(reading.now, |previous| previous.now.max(reading.now))
            .as_millisecond();
        let mut events = Vec::new();
        state
            .deadlines
            .retain(|id, _| schedules.iter().any(|saved| saved.job_id == *id));
        for mut saved in schedules {
            if *self.stopping.borrow() {
                break;
            }
            let old = state.deadlines.get(&saved.job_id);
            let rebase = discontinuity
                || old.is_none_or(|deadline| deadline.revision != saved.revision)
                || (zone_changed && matches!(saved.schedule, Schedule::Daily { .. }));
            if rebase {
                if let Some(occurrence) = old.and_then(|deadline| deadline.next)
                    && occurrence <= reading.now.as_millisecond()
                {
                    events.push(ScheduleDispatchEvent {
                        job_id: saved.job_id,
                        occurrence,
                        decision: ScheduleDispatchDecision::SkipMissed,
                    });
                }
                self.advance(&mut saved, high_water).await?;
            } else if let Some(occurrence) = old.and_then(|deadline| deadline.next)
                && occurrence <= reading.now.as_millisecond()
            {
                let late = reading.now.as_millisecond() - occurrence >= 1000;
                let decision = if late {
                    self.advance(&mut saved, reading.now.as_millisecond())
                        .await?;
                    ScheduleDispatchDecision::SkipMissed
                } else {
                    let decision = self
                        .queue
                        .dispatch_schedule(saved.job_id, saved.revision, occurrence)
                        .await?;
                    saved.last_occurrence =
                        Some(saved.last_occurrence.unwrap_or(i64::MIN).max(occurrence));
                    decision
                };
                events.push(ScheduleDispatchEvent {
                    job_id: saved.job_id,
                    occurrence,
                    decision,
                });
            }
            let next = saved.details(&reading)?.next_at;
            state.deadlines.insert(
                saved.job_id,
                Deadline {
                    revision: saved.revision,
                    next,
                },
            );
        }
        state.previous = Some(Observation {
            now: reading.now,
            monotonic,
            zone: reading.time_zone,
        });
        Ok(events)
    }

    async fn advance(&self, saved: &mut SavedSchedule, through: i64) -> Result<()> {
        let storage = self.storage.clone();
        let id = saved.job_id;
        let revision = saved.revision;
        let claimed = tokio::task::spawn_blocking(move || {
            storage.record_schedule_occurrence(id, revision, through)
        })
        .await
        .map_err(SupervisorError::from)?
        .map_err(SupervisorError::from)?;
        if claimed {
            saved.last_occurrence = Some(through);
        }
        Ok(())
    }
}

fn discontinuity(
    previous: &Observation,
    reading: &ClockReading,
    monotonic: Instant,
) -> std::result::Result<bool, ScheduleError> {
    let Some(elapsed) = monotonic.checked_duration_since(previous.monotonic) else {
        return Ok(true);
    };
    let wall = reading.now.as_nanosecond() - previous.now.as_nanosecond();
    let drift = wall - i128::try_from(elapsed.as_nanos()).map_err(|_| ScheduleError::Range)?;
    Ok(elapsed > MAX_TICK_GAP || reading.now < previous.now || drift.abs() > 2_000_000_000)
}
