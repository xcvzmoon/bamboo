mod commands;
pub mod runner;
pub mod storage;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::configure_runtimes,
            commands::runtime_paths,
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
            let supervisor = runner::RunSupervisor::new(
                storage.clone(),
                app.path().resource_dir()?.join("runner"),
                app.path().app_config_dir()?,
            )?;
            app.manage(storage);
            app.manage(Arc::new(supervisor));
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
                let supervisor = app.state::<Arc<runner::RunSupervisor>>().inner().clone();
                let app = app.clone();
                let ready = ready.clone();
                tauri::async_runtime::spawn(async move {
                    supervisor.shutdown().await;
                    ready.store(true, Ordering::Release);
                    app.exit(0);
                });
            }
        }
        _ => {}
    });
}
