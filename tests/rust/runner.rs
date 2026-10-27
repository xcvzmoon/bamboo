use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use app_lib::{
    runner::{RunSupervisor, RuntimePaths},
    storage::{LogStream, Run, RunStatus, Storage},
};
use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::{Instant, sleep, timeout};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    temporary: TempDir,
    storage: Storage,
    supervisor: Arc<RunSupervisor>,
    job_id: uuid::Uuid,
    project: PathBuf,
    node: PathBuf,
    browser: PathBuf,
}

fn node_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = std::env::var_os("BAMBOO_NODE_EXECUTABLE") {
        return Ok(path.into());
    }
    let path = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join("node"))
        .find(|path| path.is_file())
        .ok_or("Node is required for runner tests")?;
    Ok(path)
}
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

impl Fixture {
    fn new(
        program: &str,
        browser_mode: &str,
        duration: Duration,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let temporary = tempfile::Builder::new()
            .prefix("bamboo supervisor ")
            .tempdir()?;
        let root = fs::canonicalize(temporary.path())?;
        let project = root.join("project with spaces");
        fs::create_dir(&project)?;
        fs::write(project.join("package.json"), "{\"type\":\"module\"}")?;
        fs::write(
            project.join("job.ts"),
            "export default async function job() { return [] }",
        )?;
        let storage = Storage::open(&root.join("storage"))?;
        let job = storage.create_job(
            "fixture",
            &project,
            &project.join("job.ts"),
            &Value::Null,
            duration,
        )?;
        let node = node_path()?;
        let packaged = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner");
        let assets = root.join("runner assets");
        fs::create_dir(&assets)?;
        fs::write(assets.join("package.json"), "{\"type\":\"module\"}")?;
        fs::write(assets.join("main.ts"), program)?;
        fs::copy(packaged.join("guard.ts"), assets.join("guard.ts"))?;
        symlink(packaged.join("node_modules"), assets.join("node_modules"))?;
        let browser_script = root.join("browser.mjs");
        fs::write(
            &browser_script,
            format!(
                r#"
            import http from 'node:http';
            import fs from 'node:fs';
            if (process.argv[2] === 'version') {{ console.info('fixture-lightpanda'); process.exit(0); }}
            const pidFile = {};
            fs.writeFileSync(pidFile + '.tmp', String(process.pid));
            fs.renameSync(pidFile + '.tmp', pidFile);
            const mode = {};
            if (mode === 'exit') {{ process.exit(17); }}
            if (mode === 'ignore-term' || mode === 'resistant-ready') {{ process.on('SIGTERM', () => {{}}); }}
            const port = Number(process.argv[process.argv.indexOf('--port') + 1]);
            http.createServer((_request, response) => {{
              response.writeHead(mode === 'never-ready' || mode === 'ignore-term' ? 503 : 200);
              response.end('{{}}');
            }}).listen(port, '127.0.0.1');
        "#,
                serde_json::to_string(&root.join("browser.pid"))?,
                serde_json::to_string(browser_mode)?
            ),
        )?;
        let browser = root.join("lightpanda fixture");
        fs::write(
            &browser,
            format!(
                "#!/bin/sh\nexec {} {} \"$@\"\n",
                shell_quote(&node),
                shell_quote(&browser_script)
            ),
        )?;
        fs::set_permissions(&browser, fs::Permissions::from_mode(0o700))?;
        let supervisor = Arc::new(RunSupervisor::new(
            storage.clone(),
            assets,
            root.join("configuration"),
            Arc::new(app_lib::profiles::Profiles::open(
                root.join("profiles"),
                root.join("unused"),
            )?),
        )?);
        supervisor.configure_runtimes(RuntimePaths {
            node: node.clone(),
            lightpanda: browser.clone(),
        })?;
        Ok(Self {
            temporary,
            storage,
            supervisor,
            job_id: job.id,
            project,
            node,
            browser,
        })
    }
    fn browser_pid(&self) -> Result<i32, Box<dyn std::error::Error>> {
        Ok(fs::read_to_string(self.temporary.path().join("browser.pid"))?.parse()?)
    }
}

async fn assert_gone(pid: i32) -> TestResult {
    timeout(Duration::from_secs(3), async {
        loop {
            if kill(Pid::from_raw(pid), None) == Err(Errno::ESRCH) {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    Ok(())
}
async fn wait_for_file(path: &Path) -> TestResult {
    timeout(Duration::from_secs(5), async {
        while !path.is_file() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    Ok(())
}
fn code(run: &Run) -> Option<&str> {
    run.failure.as_ref().map(|failure| failure.code.as_str())
}

#[tokio::test]
async fn successful_run_saves_records_logs_and_versions_then_cleans_browser() -> TestResult {
    let program = r#"
        import fs from 'node:fs'; import path from 'node:path';
        const directory = process.argv[2];
        if ((fs.statSync(directory).mode & 0o777) !== 0o700) { throw new Error('Protocol directory must be private'); }
        if ((fs.statSync(path.join(directory, 'request.json')).mode & 0o777) !== 0o600) { throw new Error('Request file must be private'); }
        const request = JSON.parse(fs.readFileSync(path.join(directory, 'request.json'), 'utf8'));
        console.info('fixture stdout'); console.error('fixture stderr');
        fs.writeFileSync(path.join(directory, 'outcome.json'), JSON.stringify({version:1,status:'success',records:[{input:request.input}]}), {mode:0o600});
    "#;
    let fixture = Fixture::new(program, "ready", Duration::from_secs(15))?;
    let selected = fixture.temporary.path().join("selected node");
    fs::create_dir(&selected)?;
    let selected = selected.join("node");
    symlink(&fixture.node, &selected)?;
    fixture.supervisor.configure_runtimes(RuntimePaths {
        node: selected.clone(),
        lightpanda: fixture.browser.clone(),
    })?;
    let paths = fixture.supervisor.runtime_paths()?;
    assert_eq!(paths.node, selected);
    assert_eq!(paths.lightpanda, fixture.browser);
    let run = fixture
        .supervisor
        .run(
            fixture
                .storage
                .queue_run(fixture.job_id, &json!({"topic":"Bamboo"}))?,
        )
        .await?;
    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{:?}; stderr={}",
        run.failure,
        String::from_utf8_lossy(&fixture.storage.read_log(run.id, LogStream::Stderr)?)
    );
    assert_eq!(
        fixture.storage.read_results(run.id)?[0].get("input"),
        Some(&json!({"topic":"Bamboo"}))
    );
    assert!(
        String::from_utf8_lossy(&fixture.storage.read_log(run.id, LogStream::Stdout)?)
            .contains("fixture stdout")
    );
    assert!(
        String::from_utf8_lossy(&fixture.storage.read_log(run.id, LogStream::Stderr)?)
            .contains("fixture stderr")
    );
    assert!(run.runtime_versions.is_some());
    assert_gone(fixture.browser_pid()?).await?;
    Ok(())
}

#[tokio::test]
async fn invalid_outcomes_never_save_a_dataset() -> TestResult {
    for (contents, exit_code, expected) in [
        ("{", 0, "OUTCOME_INVALID"),
        (
            r#"{"version":2,"status":"success","records":[]}"#,
            0,
            "OUTCOME_INVALID",
        ),
        (
            r#"{"version":1,"status":"success","records":[42]}"#,
            0,
            "OUTCOME_INVALID",
        ),
        (
            r#"{"version":1,"status":"success","records":[]}"#,
            3,
            "RUNNER_EXIT",
        ),
        (
            r#"{"version":1,"status":"failure","error":{"code":"JOB_EXECUTION","message":"job failed"}}"#,
            1,
            "JOB_EXECUTION",
        ),
    ] {
        let program = format!(
            "import fs from 'node:fs'; import path from 'node:path'; fs.writeFileSync(path.join(process.argv[2], 'outcome.json'), {}, {{mode:0o600}}); process.exit({exit_code});",
            serde_json::to_string(contents)?
        );
        let fixture = Fixture::new(&program, "ready", Duration::from_secs(15))?;
        let run = fixture
            .supervisor
            .run(fixture.storage.queue_run(fixture.job_id, &Value::Null)?)
            .await?;
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(code(&run), Some(expected));
        assert!(fixture.storage.read_results(run.id).is_err());
        assert_gone(fixture.browser_pid()?).await?;
    }
    Ok(())
}

#[tokio::test]
async fn missing_outcome_and_descendant_cleanup_are_verified() -> TestResult {
    let fixture = Fixture::new("", "ready", Duration::from_secs(15))?;
    let pid_file = fixture.project.join("descendant.pid");
    let assets = fixture.temporary.path().join("runner assets/main.ts");
    fs::write(
        assets,
        format!(
            r#"
        import {{spawn}} from 'node:child_process'; import fs from 'node:fs';
        const child = spawn(process.execPath, ['-e', 'setInterval(() => {{}}, 1000)'], {{stdio:'ignore'}});
        fs.writeFileSync({}, String(child.pid));
        console.info('{{"version":1,"status":"success","records":[]}}');
        process.exit(0);
    "#,
            serde_json::to_string(&pid_file)?
        ),
    )?;
    let run = fixture
        .supervisor
        .run(fixture.storage.queue_run(fixture.job_id, &Value::Null)?)
        .await?;
    assert_eq!(code(&run), Some("OUTCOME_MISSING"));
    assert_gone(fs::read_to_string(pid_file)?.parse()?).await?;
    assert_gone(fixture.browser_pid()?).await?;
    Ok(())
}

#[tokio::test]
async fn startup_failure_and_missing_configuration_mark_failure() -> TestResult {
    let fixture = Fixture::new("process.exit(0)", "exit", Duration::from_secs(15))?;
    let run = fixture
        .supervisor
        .run(fixture.storage.queue_run(fixture.job_id, &Value::Null)?)
        .await?;
    assert_eq!(code(&run), Some("BROWSER_EXIT"));
    assert!(run.started_at.is_none());
    assert_gone(fixture.browser_pid()?).await?;
    fs::remove_file(fixture.temporary.path().join("configuration/runtimes.json"))?;
    let run = fixture
        .supervisor
        .run(fixture.storage.queue_run(fixture.job_id, &Value::Null)?)
        .await?;
    assert_eq!(code(&run), Some("RUNTIME_CONFIG"));
    Ok(())
}

#[tokio::test]
async fn deadline_kills_a_sigterm_resistant_browser() -> TestResult {
    let fixture = Fixture::new(
        "import fs from 'node:fs'; fs.writeFileSync('runner.started', 'ready'); setInterval(() => {}, 1000)",
        "resistant-ready",
        Duration::from_secs(15),
    )?;
    let started = Instant::now();
    let supervisor = fixture.supervisor.clone();
    let queued = fixture.storage.queue_run(fixture.job_id, &Value::Null)?;
    let task = tokio::spawn(async move { supervisor.run(queued).await });
    let pid_path = fixture.temporary.path().join("browser.pid");
    timeout(Duration::from_secs(12), async {
        while !pid_path.is_file() || !fixture.project.join("runner.started").is_file() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    let pid = fixture.browser_pid()?;
    let run = task.await??;
    assert_eq!(code(&run), Some("RUN_TIMEOUT"));
    assert!(started.elapsed() < Duration::from_secs(25));
    assert_gone(pid).await?;
    Ok(())
}

#[tokio::test]
async fn busy_admission_and_shutdown_keep_single_run_ownership() -> TestResult {
    let fixture = Fixture::new("process.exit(0)", "never-ready", Duration::from_secs(20))?;
    let supervisor = fixture.supervisor.clone();
    let id = fixture.job_id;
    let queued = fixture.storage.queue_run(id, &Value::Null)?;
    let starting = queued.clone();
    let task = tokio::spawn(async move { supervisor.run(starting).await });
    wait_for_file(&fixture.temporary.path().join("browser.pid")).await?;
    assert_eq!(
        fixture
            .supervisor
            .run(queued.clone())
            .await
            .err()
            .map(|error| error.code().to_owned())
            .as_deref(),
        Some("RUN_BUSY")
    );
    fixture.supervisor.shutdown().await;
    let run = task.await??;
    assert_eq!(run.status, RunStatus::Cancelled);
    assert_eq!(code(&run), Some("RUN_SHUTDOWN"));
    assert_eq!(fixture.storage.list_runs(id, 10, 0)?.len(), 1);
    assert_gone(fixture.browser_pid()?).await?;
    assert_eq!(
        fixture
            .supervisor
            .run(queued.clone())
            .await
            .err()
            .map(|error| error.code().to_owned())
            .as_deref(),
        Some("RUN_SHUTDOWN")
    );
    Ok(())
}

#[tokio::test]
async fn real_packaged_runner_executes_a_typescript_browser_job() -> TestResult {
    if std::env::var("BAMBOO_RUNTIME_TEST").as_deref() != Ok("1") {
        return Ok(());
    }
    let temporary = tempfile::Builder::new()
        .prefix("bamboo real supervisor ")
        .tempdir()?;
    let root = fs::canonicalize(temporary.path())?;
    let assets = std::env::var_os("BAMBOO_RUNNER_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner"));
    let lightpanda = std::env::var_os("BAMBOO_LIGHTPANDA_EXECUTABLE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|path| path.join("lightpanda"))
                .find(|path| path.is_file())
        })
        .ok_or("Lightpanda must be installed for real-runtime verification")?;
    fs::write(root.join("package.json"), "{\"type\":\"module\"}")?;
    fs::write(
        root.join("helper.ts"),
        "export enum Heading { Ready = 'Runtime connected' }",
    )?;
    fs::write(
        root.join("job.ts"),
        r#"
        import { Heading } from './helper.ts';
        export default async function job({context, input}) {
            console.info('real job stdout'); console.error('real job stderr');
            const page = await context.newPage();
            await page.setContent('<h1>Runtime connected</h1>');
            const heading = await page.locator('h1').innerText();
            if (heading !== Heading.Ready) { throw new Error('Heading mismatch'); }
            return [{heading, input}];
        }
    "#,
    )?;
    let storage = Storage::open(&root.join("storage"))?;
    let job = storage.create_job(
        "real fixture",
        &root,
        &root.join("job.ts"),
        &Value::Null,
        Duration::from_secs(30),
    )?;
    let supervisor = RunSupervisor::new(
        storage.clone(),
        assets,
        root.join("configuration"),
        Arc::new(app_lib::profiles::Profiles::open(
            root.join("profiles"),
            root.join("unused"),
        )?),
    )?;
    supervisor.configure_runtimes(RuntimePaths {
        node: node_path()?,
        lightpanda,
    })?;
    let run = supervisor
        .run(storage.queue_run(job.id, &json!({"nested":[true,null,1]}))?)
        .await?;
    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{:?}; versions={:?}; stdout={}; stderr={}",
        run.failure,
        run.runtime_versions,
        String::from_utf8_lossy(&storage.read_log(run.id, LogStream::Stdout)?),
        String::from_utf8_lossy(&storage.read_log(run.id, LogStream::Stderr)?)
    );
    assert_eq!(
        storage.read_results(run.id)?[0].get("heading"),
        Some(&json!("Runtime connected"))
    );
    assert!(
        String::from_utf8_lossy(&storage.read_log(run.id, LogStream::Stdout)?)
            .contains("real job stdout")
    );
    assert!(
        String::from_utf8_lossy(&storage.read_log(run.id, LogStream::Stderr)?)
            .contains("real job stderr")
    );
    assert!(run.entry_hash.is_some());
    assert!(run.runtime_versions.is_some());
    Ok(())
}

#[tokio::test]
async fn user_cancellation_kills_owned_groups_and_rejects_stale_ids() -> TestResult {
    let fixture = Fixture::new(
        "import fs from 'node:fs';import path from 'node:path';fs.writeFileSync(path.join(process.argv[2], 'outcome.json'), JSON.stringify({version:1,status:'success',records:[{partial:true}]}), {mode:0o600});fs.writeFileSync('node.pid', String(process.pid));setInterval(() => {}, 1000)",
        "ready",
        Duration::from_secs(30),
    )?;
    let supervisor = fixture.supervisor.clone();
    let job_id = fixture.job_id;
    let queued = fixture.storage.queue_run(job_id, &Value::Null)?;
    let task = tokio::spawn(async move { supervisor.run(queued).await });
    wait_for_file(&fixture.temporary.path().join("browser.pid")).await?;
    wait_for_file(&fixture.project.join("node.pid")).await?;
    let node_pid = fs::read_to_string(fixture.project.join("node.pid"))?.parse()?;
    let active = fixture
        .supervisor
        .active_run()?
        .ok_or("Missing active run")?;
    assert!(!fixture.supervisor.cancel(uuid::Uuid::now_v7()));
    assert!(fixture.supervisor.cancel(active.id));
    assert!(!fixture.supervisor.cancel(active.id));
    let finished = timeout(Duration::from_secs(10), task).await???;
    assert_eq!(finished.status, RunStatus::Cancelled);
    assert_eq!(code(&finished), Some("RUN_CANCELLED"));
    assert!(fixture.storage.read_results(finished.id).is_err());
    assert!(fixture.supervisor.active_run()?.is_none());
    assert!(!fixture.supervisor.cancel(active.id));
    assert_gone(fixture.browser_pid()?).await?;
    assert_gone(node_pid).await?;
    Ok(())
}

#[tokio::test]
async fn process_guard_kills_children_when_the_owner_pipe_closes() -> TestResult {
    let temporary = TempDir::new()?;
    let pid_file = temporary.path().join("child.pid");
    let script = format!(
        "const fs=require('node:fs');const path={};fs.writeFileSync(path+'.tmp',String(process.pid));fs.renameSync(path+'.tmp',path);setInterval(() => {{}},1000)",
        serde_json::to_string(&pid_file)?
    );
    let mut guard = tokio::process::Command::new(node_path()?)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner/guard.ts"))
        .arg(node_path()?)
        .arg("-e")
        .arg(script)
        .process_group(0)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(false)
        .spawn()?;
    wait_for_file(&pid_file).await?;
    let pid = fs::read_to_string(pid_file)?.parse()?;
    drop(guard.stdin.take());
    let status = timeout(Duration::from_secs(5), guard.wait()).await??;
    assert!(!status.success());
    assert_gone(pid).await?;
    Ok(())
}

#[tokio::test]
async fn manual_queue_is_fifo_snapshots_settings_and_never_launches_cancelled_work() -> TestResult {
    let fixture = Fixture::new(
        r#"
        import fs from 'node:fs';import path from 'node:path';
        const directory = process.argv[2];
        const {input} = JSON.parse(fs.readFileSync(path.join(directory,'request.json'),'utf8'));
        const lock = fs.openSync('overlap.lock', 'wx');
        fs.appendFileSync('order.log', input.label + '\n');
        fs.writeFileSync(input.label + '.started', 'ready');
        if (input.label === 'first') {
            while (!fs.existsSync('release')) { await new Promise(resolve => setTimeout(resolve, 20)); }
        }
        fs.closeSync(lock);fs.unlinkSync('overlap.lock');
        fs.writeFileSync(path.join(directory,'outcome.json'),JSON.stringify({version:1,status:'success',records:[input]}),{mode:0o600});
    "#,
        "ready",
        Duration::from_secs(30),
    )?;
    let queue = app_lib::queue::RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let first = queue
        .enqueue(fixture.job_id, json!({"label":"first"}))
        .await?;
    wait_for_file(&fixture.project.join("first.started")).await?;
    let second = queue
        .enqueue(fixture.job_id, json!({"label":"second"}))
        .await?;
    let cancelled = queue
        .enqueue(fixture.job_id, json!({"label":"cancelled"}))
        .await?;
    fixture.storage.save_job_settings(
        fixture.job_id,
        "Edited",
        &json!({"label":"changed"}),
        1000,
    )?;
    fs::write(
        fixture.project.join("job.ts"),
        "changed source at actual start",
    )?;
    let snapshot = queue.snapshot().await?;
    assert_eq!(snapshot.active.map(|run| run.id), Some(first.id));
    assert_eq!(
        snapshot
            .pending
            .iter()
            .map(|run| run.id)
            .collect::<Vec<_>>(),
        vec![second.id, cancelled.id]
    );
    assert!(queue.cancel(cancelled.id).await?);
    assert!(!queue.cancel(cancelled.id).await?);
    fs::write(fixture.project.join("release"), "go")?;
    timeout(Duration::from_secs(15), async {
        loop {
            if fixture.storage.get_run(second.id)?.status == RunStatus::Succeeded {
                return Ok::<(), Box<dyn std::error::Error>>(());
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    let completed = fixture.storage.get_run(second.id)?;
    assert_eq!(completed.job, second.job);
    assert_ne!(
        completed.entry_hash,
        fixture.storage.get_run(first.id)?.entry_hash
    );
    assert_eq!(
        fixture.storage.read_results(second.id)?[0].get("label"),
        Some(&json!("second"))
    );
    assert_eq!(
        fs::read_to_string(fixture.project.join("order.log"))?,
        "first\nsecond\n"
    );
    assert!(!fixture.project.join("cancelled.started").exists());
    assert_eq!(
        fixture.storage.get_run(cancelled.id)?.status,
        RunStatus::Cancelled
    );
    queue.shutdown().await?;
    assert!(queue.enqueue(fixture.job_id, Value::Null).await.is_err());
    Ok(())
}

#[tokio::test]
async fn queue_shutdown_cancels_active_and_pending_requests_without_replaying_them() -> TestResult {
    let fixture = Fixture::new(
        "setInterval(() => {}, 1000)",
        "ready",
        Duration::from_secs(30),
    )?;
    let queue = app_lib::queue::RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let active = queue.enqueue(fixture.job_id, Value::Null).await?;
    wait_for_file(&fixture.temporary.path().join("browser.pid")).await?;
    let pending = queue.enqueue(fixture.job_id, Value::Null).await?;
    timeout(Duration::from_secs(10), queue.shutdown()).await??;
    assert_eq!(
        fixture.storage.get_run(active.id)?.status,
        RunStatus::Cancelled
    );
    assert_eq!(
        fixture.storage.get_run(pending.id)?.status,
        RunStatus::Cancelled
    );
    let reopened =
        app_lib::queue::RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    assert!(reopened.snapshot().await?.pending.is_empty());
    assert_gone(fixture.browser_pid()?).await?;
    Ok(())
}

struct ControlledScheduleClock(std::sync::Mutex<(jiff::Timestamp, jiff::tz::TimeZone)>);

impl ControlledScheduleClock {
    fn new(now: &str) -> Self {
        Self(std::sync::Mutex::new((
            now.parse().unwrap(),
            jiff::tz::TimeZone::UTC,
        )))
    }

    fn set(&self, now: &str, zone: &str) {
        *self.0.lock().unwrap() = (now.parse().unwrap(), jiff::tz::TimeZone::get(zone).unwrap());
    }
}

impl app_lib::schedule::Clock for ControlledScheduleClock {
    fn read(&self) -> Result<app_lib::schedule::ClockReading, app_lib::schedule::ScheduleError> {
        let (now, zone) = &*self.0.lock().unwrap();
        Ok(app_lib::schedule::ClockReading {
            now: *now,
            time_zone: zone.clone(),
        })
    }
}

#[tokio::test]
async fn scheduled_dispatch_skips_manual_active_and_pending_work_without_creating_backlogs()
-> TestResult {
    use app_lib::{
        queue::{RunQueue, ScheduleDispatchDecision},
        schedule::{IntervalUnit, Schedule},
        scheduler::Scheduler,
    };
    let fixture = Fixture::new(
        "setInterval(() => {}, 1000)",
        "ready",
        Duration::from_secs(30),
    )?;
    let clock = Arc::new(ControlledScheduleClock::new("2026-10-27T08:00:00Z"));
    fixture.storage.save_schedule(
        fixture.job_id,
        Schedule::Interval {
            every: 1,
            unit: IntervalUnit::Minutes,
        },
        &*clock,
    )?;
    let queue = RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let scheduler = Scheduler::new(fixture.storage.clone(), queue.clone(), clock.clone());
    let start = std::time::Instant::now();
    assert!(scheduler.tick(start).await?.is_empty());
    let active = queue
        .enqueue(fixture.job_id, json!({"manual":"active"}))
        .await?;
    let pending = queue
        .enqueue(fixture.job_id, json!({"manual":"pending"}))
        .await?;
    clock.set("2026-10-27T08:00:59Z", "UTC");
    scheduler.tick(start + Duration::from_secs(59)).await?;
    clock.set("2026-10-27T08:01:00Z", "UTC");
    let events = scheduler.tick(start + Duration::from_secs(60)).await?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].decision, ScheduleDispatchDecision::SkipBusy);
    assert_eq!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.len(), 2);
    assert!(
        scheduler
            .tick(start + Duration::from_secs(60))
            .await?
            .is_empty()
    );
    assert_eq!(
        fixture.storage.list_schedules()?[0].last_occurrence,
        Some(
            "2026-10-27T08:01:00Z"
                .parse::<jiff::Timestamp>()?
                .as_millisecond()
        )
    );
    scheduler.shutdown().await;
    queue.shutdown().await?;
    assert_eq!(
        fixture.storage.get_run(active.id)?.status,
        RunStatus::Cancelled
    );
    assert_eq!(
        fixture.storage.get_run(pending.id)?.status,
        RunStatus::Cancelled
    );
    Ok(())
}

#[tokio::test]
async fn scheduled_dispatch_launches_once_and_publishes_the_saved_job_input() -> TestResult {
    use app_lib::{
        queue::{RunQueue, ScheduleDispatchDecision},
        schedule::{IntervalUnit, Schedule},
        scheduler::Scheduler,
    };
    let program = r#"import fs from 'node:fs'; import path from 'node:path'; const directory=process.argv[2]; const request=JSON.parse(fs.readFileSync(path.join(directory,'request.json'),'utf8')); fs.writeFileSync(path.join(directory,'outcome.json'),JSON.stringify({version:1,status:'success',records:[{input:request.input}]}),{mode:0o600});"#;
    let fixture = Fixture::new(program, "ready", Duration::from_secs(30))?;
    fixture.storage.save_job_settings(
        fixture.job_id,
        "Scheduled input",
        &json!({"saved":true}),
        30_000,
    )?;
    let clock = Arc::new(ControlledScheduleClock::new("2026-10-27T08:00:00Z"));
    fixture.storage.save_schedule(
        fixture.job_id,
        Schedule::Interval {
            every: 1,
            unit: IntervalUnit::Minutes,
        },
        &*clock,
    )?;
    let queue = RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let scheduler = Scheduler::new(fixture.storage.clone(), queue.clone(), clock.clone());
    let start = std::time::Instant::now();
    scheduler.tick(start).await?;
    clock.set("2026-10-27T08:00:59Z", "UTC");
    scheduler.tick(start + Duration::from_secs(59)).await?;
    clock.set("2026-10-27T08:01:00.250Z", "UTC");
    let events = scheduler
        .tick(start + Duration::from_millis(60_250))
        .await?;
    let ScheduleDispatchDecision::Launched { run_id } = events[0].decision else {
        panic!("Expected scheduled launch")
    };
    assert!(
        scheduler
            .tick(start + Duration::from_millis(60_250))
            .await?
            .is_empty()
    );
    let saved = fixture.storage.list_schedules()?.remove(0);
    assert_eq!(
        queue
            .dispatch_schedule(fixture.job_id, saved.revision, events[0].occurrence)
            .await?,
        ScheduleDispatchDecision::Stale
    );
    timeout(Duration::from_secs(15), async {
        while fixture.storage.get_run(run_id)?.status != RunStatus::Succeeded {
            sleep(Duration::from_millis(20)).await;
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await??;
    assert_eq!(
        fixture.storage.read_results(run_id)?[0]["input"],
        json!({"saved":true})
    );
    assert_eq!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.len(), 1);
    scheduler.shutdown().await;
    queue.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn scheduler_skips_sleep_clock_jumps_and_restart_and_stops_with_the_app() -> TestResult {
    use app_lib::{
        queue::{RunQueue, ScheduleDispatchDecision},
        schedule::{IntervalUnit, Schedule},
        scheduler::Scheduler,
    };
    let fixture = Fixture::new(
        "setInterval(() => {}, 1000)",
        "ready",
        Duration::from_secs(30),
    )?;
    let clock = Arc::new(ControlledScheduleClock::new("2026-10-27T08:00:00Z"));
    fixture.storage.save_schedule(
        fixture.job_id,
        Schedule::Interval {
            every: 1,
            unit: IntervalUnit::Minutes,
        },
        &*clock,
    )?;
    let queue = RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let scheduler = Scheduler::new(fixture.storage.clone(), queue.clone(), clock.clone());
    let start = std::time::Instant::now();
    scheduler.tick(start).await?;
    clock.set("2026-10-27T08:00:59Z", "UTC");
    scheduler.tick(start + Duration::from_secs(59)).await?;
    clock.set("2026-10-27T09:00:00Z", "UTC");
    let events = scheduler.tick(start + Duration::from_secs(3600)).await?;
    assert_eq!(events[0].decision, ScheduleDispatchDecision::SkipMissed);
    clock.set("2026-10-27T08:01:00Z", "UTC");
    scheduler.tick(start + Duration::from_secs(3601)).await?;
    assert!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.is_empty());
    scheduler.shutdown().await;
    let restarted = Scheduler::new(fixture.storage.clone(), queue.clone(), clock.clone());
    clock.set("2026-10-28T08:01:00Z", "UTC");
    assert!(
        restarted
            .tick(start + Duration::from_secs(7200))
            .await?
            .is_empty()
    );
    clock.set("2026-10-29T08:01:00Z", "UTC");
    restarted.tick(start + Duration::from_secs(7201)).await?;
    assert!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.is_empty());
    let serving = restarted.clone();
    let task = tokio::spawn(async move { serving.serve().await });
    tokio::task::yield_now().await;
    restarted.shutdown().await;
    timeout(Duration::from_secs(2), task).await??;
    clock.set("2026-10-29T08:02:00Z", "UTC");
    assert!(
        restarted
            .tick(start + Duration::from_secs(7202))
            .await?
            .is_empty()
    );
    queue.shutdown().await?;
    let saved = fixture.storage.list_schedules()?.remove(0);
    assert_eq!(
        queue
            .dispatch_schedule(fixture.job_id, saved.revision, i64::MAX)
            .await?,
        ScheduleDispatchDecision::Closed
    );
    assert!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.is_empty());
    assert!(!fixture.temporary.path().join("browser.pid").exists());
    Ok(())
}

#[tokio::test]
async fn schedule_edits_and_timezone_changes_invalidate_old_deadlines() -> TestResult {
    use app_lib::{
        queue::RunQueue,
        schedule::{IntervalUnit, Schedule},
        scheduler::Scheduler,
    };
    let fixture = Fixture::new(
        "setInterval(() => {}, 1000)",
        "ready",
        Duration::from_secs(30),
    )?;
    let clock = Arc::new(ControlledScheduleClock::new("2026-10-27T00:59:59Z"));
    clock.set("2026-10-27T00:59:59Z", "Asia/Manila");
    fixture.storage.save_schedule(
        fixture.job_id,
        Schedule::Daily { hour: 9, minute: 0 },
        &*clock,
    )?;
    let queue = RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let scheduler = Scheduler::new(fixture.storage.clone(), queue.clone(), clock.clone());
    let start = std::time::Instant::now();
    scheduler.tick(start).await?;
    clock.set("2026-10-27T01:00:00Z", "America/New_York");
    scheduler.tick(start + Duration::from_secs(1)).await?;
    assert!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.is_empty());
    let original = fixture.storage.save_schedule(
        fixture.job_id,
        Schedule::Interval {
            every: 1,
            unit: IntervalUnit::Minutes,
        },
        &*clock,
    )?;
    scheduler.tick(start + Duration::from_secs(1)).await?;
    clock.set("2026-10-27T01:00:59Z", "America/New_York");
    scheduler.tick(start + Duration::from_secs(60)).await?;
    fixture
        .storage
        .save_schedule(fixture.job_id, Schedule::Disabled, &*clock)?;
    clock.set("2026-10-27T01:01:00Z", "America/New_York");
    scheduler.tick(start + Duration::from_secs(61)).await?;
    assert!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.is_empty());
    assert_eq!(
        queue
            .dispatch_schedule(
                fixture.job_id,
                original.saved.revision,
                "2026-10-27T01:01:00Z"
                    .parse::<jiff::Timestamp>()?
                    .as_millisecond()
            )
            .await?,
        app_lib::queue::ScheduleDispatchDecision::Stale
    );
    scheduler.shutdown().await;
    queue.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn app_shutdown_cancels_an_active_scheduled_run_and_its_owned_browser() -> TestResult {
    use app_lib::{
        queue::{RunQueue, ScheduleDispatchDecision},
        schedule::{IntervalUnit, Schedule},
        scheduler::Scheduler,
    };
    let fixture = Fixture::new(
        "setInterval(() => {}, 1000)",
        "ready",
        Duration::from_secs(30),
    )?;
    let clock = Arc::new(ControlledScheduleClock::new("2026-10-27T08:00:00Z"));
    fixture.storage.save_schedule(
        fixture.job_id,
        Schedule::Interval {
            every: 1,
            unit: IntervalUnit::Minutes,
        },
        &*clock,
    )?;
    let queue = RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let scheduler = Scheduler::new(fixture.storage.clone(), queue.clone(), clock.clone());
    let start = std::time::Instant::now();
    scheduler.tick(start).await?;
    clock.set("2026-10-27T08:00:59Z", "UTC");
    scheduler.tick(start + Duration::from_secs(59)).await?;
    clock.set("2026-10-27T08:01:00Z", "Asia/Manila");
    let events = scheduler.tick(start + Duration::from_secs(60)).await?;
    let ScheduleDispatchDecision::Launched { run_id } = events[0].decision else {
        panic!("Expected interval launch despite timezone change")
    };
    wait_for_file(&fixture.temporary.path().join("browser.pid")).await?;
    scheduler.shutdown().await;
    timeout(Duration::from_secs(10), queue.shutdown()).await??;
    assert_eq!(
        fixture.storage.get_run(run_id)?.status,
        RunStatus::Cancelled
    );
    assert_gone(fixture.browser_pid()?).await?;
    clock.set("2026-10-27T08:02:00Z", "Asia/Manila");
    assert!(
        scheduler
            .tick(start + Duration::from_secs(120))
            .await?
            .is_empty()
    );
    assert_eq!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_clock_failure_skips_missed_work_on_recovery_without_closing_the_manual_queue()
-> TestResult {
    use app_lib::{
        queue::RunQueue,
        schedule::{Clock, ClockReading, IntervalUnit, Schedule, ScheduleError},
        scheduler::Scheduler,
    };
    struct FallibleClock {
        clock: ControlledScheduleClock,
        fail: std::sync::atomic::AtomicBool,
    }
    impl Clock for FallibleClock {
        fn read(&self) -> Result<ClockReading, ScheduleError> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(ScheduleError::TimeZone);
            }
            self.clock.read()
        }
    }
    let fixture = Fixture::new(
        "setInterval(() => {}, 1000)",
        "ready",
        Duration::from_secs(30),
    )?;
    let clock = Arc::new(FallibleClock {
        clock: ControlledScheduleClock::new("2026-10-27T08:00:00Z"),
        fail: std::sync::atomic::AtomicBool::new(false),
    });
    fixture.storage.save_schedule(
        fixture.job_id,
        Schedule::Interval {
            every: 1,
            unit: IntervalUnit::Minutes,
        },
        &*clock,
    )?;
    let queue = RunQueue::new(fixture.storage.clone(), fixture.supervisor.clone());
    let scheduler = Scheduler::new(fixture.storage.clone(), queue.clone(), clock.clone());
    let start = std::time::Instant::now();
    scheduler.tick(start).await?;
    clock.clock.set("2026-10-27T08:00:59Z", "UTC");
    scheduler.tick(start + Duration::from_secs(59)).await?;
    clock.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        scheduler
            .tick(start + Duration::from_secs(60))
            .await
            .is_err()
    );
    clock.clock.set("2026-10-27T08:01:00.500Z", "UTC");
    clock.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(
        scheduler
            .tick(start + Duration::from_millis(60_500))
            .await?
            .is_empty()
    );
    assert!(fixture.storage.list_runs(fixture.job_id, 20, 0)?.is_empty());
    queue.enqueue(fixture.job_id, Value::Null).await?;
    scheduler.shutdown().await;
    queue.shutdown().await?;
    Ok(())
}
