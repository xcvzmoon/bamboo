use std::{fs, io::Write, time::Duration};

use app_lib::{results::*, storage::*};
use serde_json::json;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn csv_round_trips_union_keys_escaping_nulls_and_record_order() -> TestResult {
    let records: Records = serde_json::from_value(json!([
        {"name":"竹, \"Bamboo\"\nline", "active":true,"count":2,"empty":null},
        {"extra":"第二", "count":-1.5}
    ]))?;
    let bytes = encode_export(&records, ExportFormat::Csv)?;
    let mut reader = csv::Reader::from_reader(bytes.as_slice());
    assert_eq!(
        reader.headers()?,
        &csv::StringRecord::from(vec!["active", "count", "empty", "extra", "name"])
    );
    let rows = reader.records().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        rows[0],
        csv::StringRecord::from(vec!["true", "2", "", "", "竹, \"Bamboo\"\nline"])
    );
    assert_eq!(
        rows[1],
        csv::StringRecord::from(vec!["", "-1.5", "", "第二", ""])
    );
    assert!(encode_export(&Vec::new(), ExportFormat::Csv)?.is_empty());
    assert_eq!(encode_export(&Vec::new(), ExportFormat::Json)?, b"[]");
    let no_columns = serde_json::from_value(json!([{}]))?;
    assert!(matches!(
        encode_export(&no_columns, ExportFormat::Csv),
        Err(ResultsError::NoColumns)
    ));
    let nested_array = serde_json::from_value(json!([{"values": [1]}]))?;
    assert!(matches!(
        encode_export(&nested_array, ExportFormat::Csv),
        Err(ResultsError::NestedCsv)
    ));
    Ok(())
}

#[test]
fn details_and_exports_survive_restart_and_preserve_published_data() -> TestResult {
    let temporary = TempDir::new()?;
    let project = temporary.path().join("project with spaces");
    fs::create_dir(&project)?;
    let entry = project.join("job.ts");
    fs::write(&entry, "export default async function job() { return [] }")?;
    let root = temporary.path().join("storage");
    let storage = Storage::open(&root)?;
    let job = storage.create_job(
        "Results",
        &project,
        &entry,
        &json!({}),
        Duration::from_secs(30),
    )?;
    let run = storage.queue_run(job.id, &json!({}))?;
    let mut stdout = storage.open_log(run.id, LogStream::Stdout)?;
    stdout.write_all(&vec![b'x'; 70_000])?;
    drop(stdout);
    storage.start_run(
        run.id,
        RuntimeVersions {
            node: "26".into(),
            lightpanda: "1".into(),
            playwright: "1".into(),
        },
    )?;
    let records: Records = serde_json::from_value(json!(vec![json!({"nested":{"value":1}}); 25]))?;
    storage.finish_run(run.id, RunCompletion::Succeeded(records.clone()))?;
    let storage = Storage::open(&root)?;
    let loaded = details(&storage, run.id)?;
    assert_eq!(loaded.records.map(|records| records.len()), Some(20));
    assert_eq!(loaded.run.record_count, Some(25));
    assert_eq!(loaded.stdout.len(), 65_536);
    assert!(loaded.stdout_truncated);
    assert!(!loaded.stderr_truncated);
    assert!(matches!(
        export_bytes(&storage, run.id, ExportFormat::Csv),
        Err(ResultsError::NestedCsv)
    ));
    let json_bytes = export_bytes(&storage, run.id, ExportFormat::Json)?;
    assert_eq!(serde_json::from_slice::<Records>(&json_bytes)?, records);
    let destination = temporary.path().join("export with spaces.json");
    save_export(&destination, &json_bytes)?;
    assert!(save_export(&destination, b"replace").is_err());
    assert_eq!(fs::read(&destination)?, json_bytes);
    assert_eq!(storage.read_results(run.id)?, records);
    assert!(save_export(&temporary.path().join("missing/export.json"), b"data").is_err());
    assert!(matches!(
        save_export(std::path::Path::new("relative.json"), b"data"),
        Err(ResultsError::Path)
    ));
    let failed = storage.queue_run(job.id, &json!(null))?;
    storage.finish_run(
        failed.id,
        RunCompletion::Failed(RunFailure {
            code: "JOB_ERROR".into(),
            message: "failed".into(),
        }),
    )?;
    assert!(details(&storage, failed.id)?.records.is_none());
    assert!(export_bytes(&storage, failed.id, ExportFormat::Json).is_err());
    Ok(())
}
