use std::fs;

use jiff::{
    Timestamp,
    civil::Time,
    tz::{AmbiguousOffset, TimeZone},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error(
        "SCHEDULE_INPUT: Interval must be positive, and daily time must be between 00:00 and 23:59"
    )]
    Input,
    #[error("SCHEDULE_RANGE: Schedule exceeds the supported date or timestamp range")]
    Range,
    #[error("SCHEDULE_TIMEZONE: Cannot read the current system timezone")]
    TimeZone,
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),
}

impl ScheduleError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Input => "SCHEDULE_INPUT",
            Self::Range => "SCHEDULE_RANGE",
            Self::TimeZone => "SCHEDULE_TIMEZONE",
            Self::Storage(error) => error.code(),
        }
    }
}

type Result<T> = std::result::Result<T, ScheduleError>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntervalUnit {
    Minutes,
    Hours,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schedule {
    Disabled,
    Interval { every: u32, unit: IntervalUnit },
    Daily { hour: u8, minute: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedSchedule {
    pub job_id: Uuid,
    pub revision: Uuid,
    pub schedule: Schedule,
    pub anchor_at: i64,
    pub last_occurrence: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleDetails {
    pub saved: SavedSchedule,
    pub next_at: Option<i64>,
}

pub struct ClockReading {
    pub now: Timestamp,
    pub time_zone: TimeZone,
}

pub trait Clock {
    fn read(&self) -> Result<ClockReading>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn read(&self) -> Result<ClockReading> {
        let bytes = fs::read("/etc/localtime").map_err(|_| ScheduleError::TimeZone)?;
        let time_zone = TimeZone::tzif("system", &bytes).map_err(|_| ScheduleError::TimeZone)?;
        Ok(ClockReading {
            now: Timestamp::now(),
            time_zone,
        })
    }
}

impl Schedule {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Interval { every: 0, .. } => Err(ScheduleError::Input),
            Self::Daily { hour, minute } if *hour > 23 || *minute > 59 => Err(ScheduleError::Input),
            _ => Ok(()),
        }
    }

    pub fn next_after(
        &self,
        anchor: Timestamp,
        last_occurrence: Option<Timestamp>,
        clock: &ClockReading,
    ) -> Result<Option<Timestamp>> {
        self.validate()?;
        let after = last_occurrence
            .map_or(clock.now, |last| last.max(clock.now))
            .max(anchor);
        match self {
            Self::Disabled => Ok(None),
            Self::Interval { every, unit } => {
                let unit_milliseconds = match unit {
                    IntervalUnit::Minutes => 60_000_i64,
                    IntervalUnit::Hours => 3_600_000_i64,
                };
                let period = i64::from(*every) * unit_milliseconds;
                let anchor = anchor.as_millisecond();
                let after = after.as_millisecond();
                let steps = if after < anchor {
                    1
                } else {
                    (after - anchor) / period + 1
                };
                let next = steps
                    .checked_mul(period)
                    .and_then(|elapsed| anchor.checked_add(elapsed))
                    .ok_or(ScheduleError::Range)?;
                Timestamp::from_millisecond(next)
                    .map(Some)
                    .map_err(|_| ScheduleError::Range)
            }
            Self::Daily { hour, minute } => {
                let time = Time::new(
                    i8::try_from(*hour).map_err(|_| ScheduleError::Input)?,
                    i8::try_from(*minute).map_err(|_| ScheduleError::Input)?,
                    0,
                    0,
                )
                .map_err(|_| ScheduleError::Input)?;
                let mut date = after.to_zoned(clock.time_zone.clone()).date();
                loop {
                    let ambiguous = clock
                        .time_zone
                        .to_ambiguous_timestamp(date.to_datetime(time));
                    let next = match ambiguous.offset() {
                        AmbiguousOffset::Gap { .. } => {
                            let earlier = ambiguous.earlier().map_err(|_| ScheduleError::Range)?;
                            clock
                                .time_zone
                                .following(earlier)
                                .next()
                                .ok_or(ScheduleError::Range)?
                                .timestamp()
                        }
                        _ => ambiguous.earlier().map_err(|_| ScheduleError::Range)?,
                    };
                    if next > after {
                        return Ok(Some(next));
                    }
                    date = date.tomorrow().map_err(|_| ScheduleError::Range)?;
                }
            }
        }
    }
}

impl SavedSchedule {
    pub fn details(&self, clock: &ClockReading) -> Result<ScheduleDetails> {
        let anchor =
            Timestamp::from_millisecond(self.anchor_at).map_err(|_| ScheduleError::Range)?;
        let last = self
            .last_occurrence
            .map(Timestamp::from_millisecond)
            .transpose()
            .map_err(|_| ScheduleError::Range)?;
        let next_at = self
            .schedule
            .next_after(anchor, last, clock)?
            .map(|next| next.as_millisecond());
        Ok(ScheduleDetails {
            saved: self.clone(),
            next_at,
        })
    }
}
