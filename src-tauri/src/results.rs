use std::{collections::BTreeSet, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::storage::{LogStream, Records, Run, RunStatus, Storage, StorageError};

#[derive(Debug, thiserror::Error)]
pub enum ResultsError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("EXPORT_CSV_NESTED: CSV requires scalar or null values; use JSON for nested data")]
    NestedCsv,
    #[error("EXPORT_CSV_NO_COLUMNS: Records have no columns; export this dataset as JSON")]
    NoColumns,
    #[error("EXPORT_CSV: {0}")]
    Csv(#[from] csv::Error),
    #[error("EXPORT_IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("EXPORT_JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("EXPORT_PATH: Choose an absolute destination with an existing parent directory")]
    Path,
}

impl ResultsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage(error) => error.code(),
            Self::NestedCsv => "EXPORT_CSV_NESTED",
            Self::NoColumns => "EXPORT_CSV_NO_COLUMNS",
            Self::Csv(_) => "EXPORT_CSV",
            Self::Io(_) => "EXPORT_IO",
            Self::Json(_) => "EXPORT_JSON",
            Self::Path => "EXPORT_PATH",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Csv,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

#[derive(Serialize)]
pub struct RunDetails {
    pub run: Run,
    pub records: Option<Records>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

pub fn details(storage: &Storage, id: Uuid) -> Result<RunDetails, ResultsError> {
    let run = storage.get_run(id)?;
    let records = if run.status == RunStatus::Succeeded {
        let mut records = storage.read_results(id)?;
        records.truncate(20);
        Some(records)
    } else {
        None
    };
    let (stdout, stdout_truncated) = storage.read_log_excerpt(id, LogStream::Stdout)?;
    let (stderr, stderr_truncated) = storage.read_log_excerpt(id, LogStream::Stderr)?;
    Ok(RunDetails {
        run,
        records,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
}

pub fn encode_export(records: &Records, format: ExportFormat) -> Result<Vec<u8>, ResultsError> {
    if matches!(format, ExportFormat::Json) {
        return Ok(serde_json::to_vec_pretty(records)?);
    }
    let mut keys = BTreeSet::new();
    for record in records {
        for (key, value) in record {
            if matches!(value, Value::Array(_) | Value::Object(_)) {
                return Err(ResultsError::NestedCsv);
            }
            keys.insert(key.as_str());
        }
    }
    if keys.is_empty() {
        if !records.is_empty() {
            return Err(ResultsError::NoColumns);
        }
        return Ok(Vec::new());
    }
    let mut bytes = Vec::new();
    {
        let mut writer = csv::Writer::from_writer(&mut bytes);
        writer.write_record(&keys)?;
        for record in records {
            let mut cells = Vec::with_capacity(keys.len());
            for key in &keys {
                cells.push(match record.get(*key) {
                    None | Some(Value::Null) => String::new(),
                    Some(Value::String(text)) => text.clone(),
                    Some(value) => value.to_string(),
                });
            }
            writer.write_record(cells)?;
        }
        writer.flush()?;
    }
    Ok(bytes)
}

pub fn export_bytes(
    storage: &Storage,
    id: Uuid,
    format: ExportFormat,
) -> Result<Vec<u8>, ResultsError> {
    encode_export(&storage.read_results(id)?, format)
}

pub fn save_export(destination: &Path, bytes: &[u8]) -> Result<(), ResultsError> {
    if !destination.is_absolute() {
        return Err(ResultsError::Path);
    }
    let parent = destination.parent().ok_or(ResultsError::Path)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}
