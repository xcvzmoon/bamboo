use std::{fs, time::Duration};

use app_lib::{
    schedule::{Clock, ClockReading, IntervalUnit, Schedule, ScheduleError, SystemClock},
    storage::Storage,
};
use jiff::{Timestamp, tz::TimeZone};

struct TestClock {
    now: Timestamp,
    zone: TimeZone,
}

impl TestClock {
    fn new(now: &str, zone: &str) -> Self {
        Self {
            now: now.parse().unwrap(),
            zone: TimeZone::get(zone).unwrap(),
        }
    }
}

impl Clock for TestClock {
    fn read(&self) -> Result<ClockReading, ScheduleError> {
        Ok(ClockReading {
            now: self.now,
            time_zone: self.zone.clone(),
        })
    }
}

fn next(
    schedule: Schedule,
    anchor: &str,
    now: &str,
    zone: &str,
    last: Option<&str>,
) -> Option<String> {
    let clock = TestClock::new(now, zone);
    schedule
        .next_after(
            anchor.parse().unwrap(),
            last.map(|timestamp| timestamp.parse().unwrap()),
            &clock.read().unwrap(),
        )
        .unwrap()
        .map(|timestamp| timestamp.to_string())
}

#[test]
fn intervals_keep_their_anchor_and_skip_equal_or_missed_occurrences() {
    let schedule = Schedule::Interval {
        every: 15,
        unit: IntervalUnit::Minutes,
    };
    let anchor = "2026-10-27T08:00:00Z";
    assert_eq!(
        next(schedule.clone(), anchor, anchor, "UTC", None).as_deref(),
        Some("2026-10-27T08:15:00Z")
    );
    assert_eq!(
        next(
            schedule.clone(),
            anchor,
            "2026-10-27T08:14:59.999Z",
            "UTC",
            None
        )
        .as_deref(),
        Some("2026-10-27T08:15:00Z")
    );
    assert_eq!(
        next(
            schedule.clone(),
            anchor,
            "2026-10-27T08:15:00Z",
            "UTC",
            None
        )
        .as_deref(),
        Some("2026-10-27T08:30:00Z")
    );
    assert_eq!(
        next(
            schedule.clone(),
            anchor,
            "2026-10-30T08:16:00Z",
            "UTC",
            None
        )
        .as_deref(),
        Some("2026-10-30T08:30:00Z")
    );
    assert_eq!(
        next(schedule, anchor, "2026-10-27T07:30:00Z", "UTC", None).as_deref(),
        Some("2026-10-27T08:15:00Z")
    );
    let hours = Schedule::Interval {
        every: 2,
        unit: IntervalUnit::Hours,
    };
    assert_eq!(
        next(hours, anchor, "2026-10-27T10:05:00Z", "Asia/Manila", None).as_deref(),
        Some("2026-10-27T12:00:00Z")
    );
}

#[test]
fn daily_times_are_local_strictly_future_and_skip_missed_days() {
    let schedule = Schedule::Daily {
        hour: 9,
        minute: 30,
    };
    let anchor = "2026-10-01T00:00:00Z";
    assert_eq!(
        next(
            schedule.clone(),
            anchor,
            "2026-10-27T01:29:59Z",
            "Asia/Manila",
            None
        )
        .as_deref(),
        Some("2026-10-27T01:30:00Z")
    );
    assert_eq!(
        next(
            schedule.clone(),
            anchor,
            "2026-10-27T01:30:00Z",
            "Asia/Manila",
            None
        )
        .as_deref(),
        Some("2026-10-28T01:30:00Z")
    );
    assert_eq!(
        next(
            schedule,
            anchor,
            "2026-11-05T04:00:00Z",
            "Asia/Manila",
            None
        )
        .as_deref(),
        Some("2026-11-06T01:30:00Z")
    );
}

#[test]
fn daylight_saving_gaps_use_the_first_valid_instant() {
    let anchor = "2026-01-01T00:00:00Z";
    assert_eq!(
        next(
            Schedule::Daily {
                hour: 2,
                minute: 30
            },
            anchor,
            "2026-03-08T06:59:00Z",
            "America/New_York",
            None
        )
        .as_deref(),
        Some("2026-03-08T07:00:00Z")
    );
    assert_eq!(
        next(
            Schedule::Daily {
                hour: 2,
                minute: 15
            },
            anchor,
            "2026-10-03T15:00:00Z",
            "Australia/Lord_Howe",
            None
        )
        .as_deref(),
        Some("2026-10-03T15:30:00Z")
    );
    assert_eq!(
        next(
            Schedule::Daily { hour: 9, minute: 0 },
            "2011-12-01T00:00:00Z",
            "2011-12-29T23:00:00Z",
            "Pacific/Apia",
            None
        )
        .as_deref(),
        Some("2011-12-30T10:00:00Z")
    );
}

#[test]
fn repeated_local_times_have_one_occurrence_and_clock_rewinds_do_not_repeat_it() {
    let schedule = Schedule::Daily {
        hour: 1,
        minute: 30,
    };
    let anchor = "2026-01-01T00:00:00Z";
    assert_eq!(
        next(
            schedule.clone(),
            anchor,
            "2026-11-01T05:00:00Z",
            "America/New_York",
            None
        )
        .as_deref(),
        Some("2026-11-01T05:30:00Z")
    );
    assert_eq!(
        next(
            schedule.clone(),
            anchor,
            "2026-11-01T05:45:00Z",
            "America/New_York",
            None
        )
        .as_deref(),
        Some("2026-11-02T06:30:00Z")
    );
    assert_eq!(
        next(
            schedule,
            anchor,
            "2026-11-01T05:00:00Z",
            "America/New_York",
            Some("2026-11-01T05:30:00Z")
        )
        .as_deref(),
        Some("2026-11-02T06:30:00Z")
    );
}

#[test]
fn timezone_changes_recompute_daily_times_but_not_intervals() {
    let now = "2026-10-27T01:00:00Z";
    let anchor = "2026-10-01T00:00:00Z";
    let daily = Schedule::Daily { hour: 9, minute: 0 };
    assert_eq!(
        next(daily.clone(), anchor, now, "Asia/Manila", None).as_deref(),
        Some("2026-10-28T01:00:00Z")
    );
    assert_eq!(
        next(daily, anchor, now, "America/New_York", None).as_deref(),
        Some("2026-10-27T13:00:00Z")
    );
    let interval = Schedule::Interval {
        every: 1,
        unit: IntervalUnit::Hours,
    };
    assert_eq!(
        next(interval.clone(), anchor, now, "Asia/Manila", None),
        next(interval, anchor, now, "America/New_York", None)
    );
}

#[test]
fn disabled_invalid_and_out_of_range_schedules_are_explicit() {
    assert_eq!(
        next(
            Schedule::Disabled,
            "2026-01-01T00:00:00Z",
            "2026-10-27T00:00:00Z",
            "UTC",
            None
        ),
        None
    );
    for schedule in [
        Schedule::Interval {
            every: 0,
            unit: IntervalUnit::Minutes,
        },
        Schedule::Daily {
            hour: 24,
            minute: 0,
        },
        Schedule::Daily {
            hour: 0,
            minute: 60,
        },
    ] {
        assert!(matches!(schedule.validate(), Err(ScheduleError::Input)));
    }
    let clock = ClockReading {
        now: Timestamp::MAX,
        time_zone: TimeZone::UTC,
    };
    assert!(matches!(
        Schedule::Interval {
            every: u32::MAX,
            unit: IntervalUnit::Hours
        }
        .next_after(Timestamp::MAX, None, &clock),
        Err(ScheduleError::Range)
    ));
    assert!(
        Schedule::Daily {
            hour: 23,
            minute: 59
        }
        .next_after(Timestamp::MAX, None, &clock)
        .is_err()
    );
    assert!(
        serde_json::from_str::<Schedule>(
            r#"{"kind":"daily","hour":9,"minute":0,"timezone":"UTC"}"#
        )
        .is_err()
    );
}

#[test]
fn schedules_survive_restart_advance_monotonically_and_reject_stale_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let storage = Storage::open(&root.join("storage")).unwrap();
    let entry = root.join("job.ts");
    fs::write(&entry, "export default async () => []").unwrap();
    let job = storage
        .create_job(
            "Scheduled job",
            root,
            &entry,
            &serde_json::Value::Null,
            Duration::from_secs(30),
        )
        .unwrap();
    let mut clock = TestClock::new("2026-10-27T08:00:00Z", "UTC");
    let details = storage
        .save_schedule(
            job.id,
            Schedule::Interval {
                every: 15,
                unit: IntervalUnit::Minutes,
            },
            &clock,
        )
        .unwrap();
    let first = details.next_at.unwrap();
    assert!(
        storage
            .record_schedule_occurrence(job.id, details.saved.revision, first)
            .unwrap()
    );
    assert!(
        !storage
            .record_schedule_occurrence(job.id, details.saved.revision, first)
            .unwrap()
    );
    clock.now = "2026-10-27T07:00:00Z".parse().unwrap();
    let reopened = Storage::open(&root.join("storage")).unwrap();
    let saved = reopened.list_schedules().unwrap().remove(0);
    assert_eq!(
        saved,
        app_lib::schedule::SavedSchedule {
            last_occurrence: Some(first),
            ..details.saved.clone()
        }
    );
    assert_eq!(
        saved
            .details(&clock.read().unwrap())
            .unwrap()
            .next_at
            .unwrap(),
        "2026-10-27T08:30:00Z"
            .parse::<Timestamp>()
            .unwrap()
            .as_millisecond()
    );
    clock.now = "2026-11-01T08:16:00Z".parse().unwrap();
    assert_eq!(
        saved
            .details(&clock.read().unwrap())
            .unwrap()
            .next_at
            .unwrap(),
        "2026-11-01T08:30:00Z"
            .parse::<Timestamp>()
            .unwrap()
            .as_millisecond()
    );
    assert!(
        storage
            .save_schedule(
                job.id,
                Schedule::Interval {
                    every: 0,
                    unit: IntervalUnit::Hours
                },
                &clock
            )
            .is_err()
    );
    assert_eq!(storage.list_schedules().unwrap()[0], saved);
    let disabled = storage
        .save_schedule(job.id, Schedule::Disabled, &clock)
        .unwrap();
    assert!(disabled.next_at.is_none());
    assert!(
        !storage
            .record_schedule_occurrence(job.id, details.saved.revision, first + 60_000)
            .unwrap()
    );
    assert!(
        !storage
            .record_schedule_occurrence(
                job.id,
                disabled.saved.revision,
                clock.now.as_millisecond() + 60_000
            )
            .unwrap()
    );
    assert_ne!(details.saved.revision, disabled.saved.revision);
    assert!(storage.list_runs(job.id, 20, 0).unwrap().is_empty());
}

#[test]
fn system_clock_reads_current_mac_timezone_and_returns_a_future_daily_time() {
    let reading = SystemClock.read().unwrap();
    let next = Schedule::Daily { hour: 9, minute: 0 }
        .next_after(reading.now, None, &reading)
        .unwrap()
        .unwrap();
    assert!(next > reading.now);
}

#[test]
fn persisted_daily_schedules_recompute_with_the_clock_and_keep_settings_on_clock_failure() {
    struct UnavailableClock;
    impl Clock for UnavailableClock {
        fn read(&self) -> Result<ClockReading, ScheduleError> {
            Err(ScheduleError::TimeZone)
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let storage = Storage::open(&root.join("storage")).unwrap();
    let entry = root.join("job.ts");
    fs::write(&entry, "export default async () => []").unwrap();
    let job = storage
        .create_job(
            "Daily job",
            root,
            &entry,
            &serde_json::Value::Null,
            Duration::from_secs(30),
        )
        .unwrap();
    let mut clock = TestClock::new("2026-10-27T01:00:00Z", "Asia/Manila");
    let original = storage
        .save_schedule(job.id, Schedule::Daily { hour: 9, minute: 0 }, &clock)
        .unwrap();
    assert_eq!(
        original.next_at,
        Some(
            "2026-10-28T01:00:00Z"
                .parse::<Timestamp>()
                .unwrap()
                .as_millisecond()
        )
    );
    clock.zone = TimeZone::get("America/New_York").unwrap();
    let saved = Storage::open(&root.join("storage"))
        .unwrap()
        .list_schedules()
        .unwrap()
        .remove(0);
    assert_eq!(
        saved.details(&clock.read().unwrap()).unwrap().next_at,
        Some(
            "2026-10-27T13:00:00Z"
                .parse::<Timestamp>()
                .unwrap()
                .as_millisecond()
        )
    );
    assert!(matches!(
        storage.save_schedule(job.id, Schedule::Disabled, &UnavailableClock),
        Err(ScheduleError::TimeZone)
    ));
    assert_eq!(storage.list_schedules().unwrap()[0], original.saved);
    assert!(
        storage
            .record_schedule_occurrence(job.id, saved.revision, i64::MAX)
            .is_err()
    );
    let interval = Schedule::Interval {
        every: 1,
        unit: IntervalUnit::Hours,
    };
    assert_eq!(
        next(
            interval,
            "2026-11-01T04:30:00Z",
            "2026-11-01T05:30:00Z",
            "America/New_York",
            None
        )
        .as_deref(),
        Some("2026-11-01T06:30:00Z")
    );
}

#[test]
fn scheduled_claim_and_run_publication_commit_together_and_reject_duplicates() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let storage = Storage::open(&root.join("storage")).unwrap();
    let entry = root.join("job.ts");
    fs::write(&entry, "export default async () => []").unwrap();
    let job = storage
        .create_job(
            "Atomic schedule",
            root,
            &entry,
            &serde_json::json!({"saved":true}),
            Duration::from_secs(30),
        )
        .unwrap();
    let clock = TestClock::new("2026-10-27T08:00:00Z", "UTC");
    let schedule = storage
        .save_schedule(
            job.id,
            Schedule::Interval {
                every: 1,
                unit: IntervalUnit::Minutes,
            },
            &clock,
        )
        .unwrap();
    let database = rusqlite::Connection::open(root.join("storage/bamboo.sqlite3")).unwrap();
    database.execute_batch("CREATE TRIGGER reject_scheduled_run BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT,'fixture publication failure'); END;").unwrap();
    assert!(
        storage
            .queue_scheduled_run(job.id, schedule.saved.revision, schedule.next_at.unwrap())
            .is_err()
    );
    assert!(
        storage.list_schedules().unwrap()[0]
            .last_occurrence
            .is_none()
    );
    assert!(storage.list_runs(job.id, 20, 0).unwrap().is_empty());
    assert_eq!(fs::read_dir(root.join("storage/runs")).unwrap().count(), 0);
    database
        .execute_batch("DROP TRIGGER reject_scheduled_run")
        .unwrap();
    let run = storage
        .queue_scheduled_run(job.id, schedule.saved.revision, schedule.next_at.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(run.job.input, serde_json::json!({"saved":true}));
    assert!(
        storage
            .queue_scheduled_run(job.id, schedule.saved.revision, schedule.next_at.unwrap())
            .unwrap()
            .is_none()
    );
    let reopened = Storage::open(&root.join("storage")).unwrap();
    reopened.recover_unfinished_runs().unwrap();
    assert!(
        reopened
            .queue_scheduled_run(job.id, schedule.saved.revision, schedule.next_at.unwrap())
            .unwrap()
            .is_none()
    );
}
