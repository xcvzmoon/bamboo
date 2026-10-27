mod commands;
pub mod profiles;
pub mod queue;
pub mod results;
pub mod runner;
pub mod schedule;
pub mod scheduler;
pub mod storage;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::example_defaults,
            commands::list_profiles,
            commands::set_job_profile,
            commands::profile_capture_status,
            commands::begin_profile_capture,
            commands::finish_profile_capture,
            commands::cancel_profile_capture,
            commands::run_details,
            commands::delete_run,
            commands::delete_job_history,
            commands::get_schedule,
            commands::list_runs,
            commands::export_run,
            commands::list_jobs,
            commands::list_schedules,
            commands::save_schedule,
            commands::register_job,
            commands::save_job_settings,
            commands::select_job_path,
            commands::configure_runtimes,
            commands::runtime_paths,
            commands::queue_snapshot,
            commands::cancel_run,
            commands::run_job
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let storage = storage::Storage::open(&app.path().app_data_dir()?.join("storage"))?;
            storage.recover_unfinished_runs()?;
            let profiles = Arc::new(profiles::Profiles::open(
                app.path().app_data_dir()?.join("profiles"),
                app.path().resource_dir()?.join("runner"),
            )?);
            profiles.recover(&storage)?;
            let supervisor = runner::RunSupervisor::new(
                storage.clone(),
                app.path().resource_dir()?.join("runner"),
                app.path().app_config_dir()?,
                profiles.clone(),
            )?;
            let supervisor = Arc::new(supervisor);
            let queue = queue::RunQueue::new(storage.clone(), supervisor.clone());
            let scheduler = scheduler::Scheduler::new(
                storage.clone(),
                queue.clone(),
                Arc::new(schedule::SystemClock),
            );
            app.manage(queue);
            app.manage(scheduler.clone());
            tauri::async_runtime::spawn(async move {
                scheduler.serve().await;
            });
            app.manage(profiles);
            app.manage(storage);
            app.manage(supervisor);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    let stopping = Arc::new(AtomicBool::new(false));
    let ready = Arc::new(AtomicBool::new(false));
    app.run(move |app, event| match event {
        tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            api.prevent_close();
            app.exit(0);
        }
        tauri::RunEvent::ExitRequested { api, .. } if !ready.load(Ordering::Acquire) => {
            api.prevent_exit();
            if !stopping.swap(true, Ordering::AcqRel) {
                let scheduler = app.state::<Arc<scheduler::Scheduler>>().inner().clone();
                let profiles = app.state::<Arc<profiles::Profiles>>().inner().clone();
                let queue = app.state::<Arc<queue::RunQueue>>().inner().clone();
                let app = app.clone();
                let ready = ready.clone();
                tauri::async_runtime::spawn(async move {
                    scheduler.shutdown().await;
                    let _ = tokio::task::spawn_blocking(move || profiles.cancel(None)).await;
                    if let Err(error) = queue.shutdown().await {
                        log::error!("RUN_QUEUE_SHUTDOWN: {error}");
                    }
                    ready.store(true, Ordering::Release);
                    app.exit(0);
                });
            }
        }
        _ => {}
    });
}
