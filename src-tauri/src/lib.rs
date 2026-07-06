mod types;
mod config;
mod log;
mod state;
mod probe;
mod engine;
mod network;
mod worker;
mod ws;
mod cmd;
mod tray;

use crate::cmd::AppState;
use crate::log::Logger;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Manager;
use tokio::sync::mpsc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (request_tx, _request_rx) = mpsc::unbounded_channel();

    let logger = Mutex::new(Logger::new().expect("Failed to initialize logger"));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let _ = crate::tray::build_tray(app.handle());

            let db = crate::state::db::Db::new().expect("Failed to initialize database");
            let settings = crate::config::load();
            let danger_accept_invalid_certs = settings.danger_accept_invalid_certs;
            let worker_pool = crate::worker::WorkerPool::new(8, event_tx.clone(), danger_accept_invalid_certs);
            let state = Arc::new(AppState {
                db,
                worker_pool,
                logger,
                app_handle: app.handle().clone(),
            });
            let ev_state = state.clone();
            app.manage(state);

            // Set window title with version
            let handle = app.handle();
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_title(&format!("ProxyDownloadManager {}", handle.package_info().version));
            }

            // Spawn event handler: listens for download events, updates DB
            tauri::async_runtime::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    ev_state.handle_event(event).await;
                }
            });

            let ev_tx = event_tx;
            let req_tx = request_tx;
            std::thread::spawn(move || {
                let mut server = crate::ws::server::WsServer::new(ev_tx, req_tx);
                if let Err(e) = server.start("127.0.0.1:18999") {
                    eprintln!("WS server error: {}", e);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd::list_downloads,
            cmd::start_download,
            cmd::pause_download,
            cmd::resume_download,
            cmd::cancel_download,
            cmd::delete_download,
            cmd::get_settings,
            cmd::save_settings,
            cmd::redownload_download,
            cmd::exit_app,
            cmd::read_logs,
            cmd::file_exists,
            cmd::test_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
