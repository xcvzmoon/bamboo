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
        symlink(packaged.join("node_modules"), assets.join("node_modules"))?;
        let browser_script = root.join("browser.mjs");
        fs::write(
            &browser_script,
            format!(
                r#"
            import http from 'node:http';
            import fs from 'node:fs';
            if (process.argv[2] === 'version') {{ console.info('fixture-lightpanda'); process.exit(0); }}
            fs.writeFileSync({}, String(process.pid));
            const mode = {};
            if (mode === 'exit') {{ process.exit(17); }}
            if (mode === 'ignore-term') {{ process.on('SIGTERM', () => {{}}); }}
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
        .run(fixture.job_id, json!({"topic":"Bamboo"}))
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
        let run = fixture.supervisor.run(fixture.job_id, Value::Null).await?;
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
    let run = fixture.supervisor.run(fixture.job_id, Value::Null).await?;
    assert_eq!(code(&run), Some("OUTCOME_MISSING"));
    assert_gone(fs::read_to_string(pid_file)?.parse()?).await?;
    assert_gone(fixture.browser_pid()?).await?;
    Ok(())
}

#[tokio::test]
async fn startup_failure_and_missing_configuration_mark_failure() -> TestResult {
    let fixture = Fixture::new("process.exit(0)", "exit", Duration::from_secs(15))?;
    let run = fixture.supervisor.run(fixture.job_id, Value::Null).await?;
    assert_eq!(code(&run), Some("BROWSER_EXIT"));
    assert!(run.started_at.is_none());
    assert_gone(fixture.browser_pid()?).await?;
    fs::remove_file(fixture.temporary.path().join("configuration/runtimes.json"))?;
    let run = fixture.supervisor.run(fixture.job_id, Value::Null).await?;
    assert_eq!(code(&run), Some("RUNTIME_CONFIG"));
    Ok(())
}

#[tokio::test]
async fn deadline_kills_a_sigterm_resistant_browser() -> TestResult {
    let fixture = Fixture::new("process.exit(0)", "ignore-term", Duration::from_secs(2))?;
    let started = Instant::now();
    let run = fixture.supervisor.run(fixture.job_id, Value::Null).await?;
    assert_eq!(code(&run), Some("RUN_TIMEOUT"));
    assert!(started.elapsed() < Duration::from_secs(8));
    assert_gone(fixture.browser_pid()?).await?;
    Ok(())
}

#[tokio::test]
async fn busy_admission_and_shutdown_keep_single_run_ownership() -> TestResult {
    let fixture = Fixture::new("process.exit(0)", "never-ready", Duration::from_secs(20))?;
    let supervisor = fixture.supervisor.clone();
    let id = fixture.job_id;
    let task = tokio::spawn(async move { supervisor.run(id, Value::Null).await });
    wait_for_file(&fixture.temporary.path().join("browser.pid")).await?;
    assert_eq!(
        fixture
            .supervisor
            .run(id, Value::Null)
            .await
            .err()
            .map(|error| error.code().to_owned())
            .as_deref(),
        Some("RUN_BUSY")
    );
    fixture.supervisor.shutdown().await;
    let run = task.await??;
    assert_eq!(run.status, RunStatus::Interrupted);
    assert_eq!(fixture.storage.list_runs(id, 10, 0)?.len(), 1);
    assert_gone(fixture.browser_pid()?).await?;
    assert_eq!(
        fixture
            .supervisor
            .run(id, Value::Null)
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
    let supervisor = RunSupervisor::new(storage.clone(), assets, root.join("configuration"))?;
    supervisor.configure_runtimes(RuntimePaths {
        node: node_path()?,
        lightpanda,
    })?;
    let run = supervisor
        .run(job.id, json!({"nested":[true,null,1]}))
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
