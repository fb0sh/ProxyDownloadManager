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
mod icons;

use crate::cmd::AppState;
use crate::log::Logger;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Manager;
use tokio::sync::mpsc;

pub(crate) const SILENT_START_ARG: &str = "--silent";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let silent_start = std::env::args().any(|arg| arg == SILENT_START_ARG);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<crate::types::PendingDownloadRequest>();

    let logger = Mutex::new(Logger::new().expect("Failed to initialize logger"));

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|arg| arg == SILENT_START_ARG) {
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let _ = crate::tray::build_tray(app.handle());

            let icon_cache = crate::icons::IconCache::new();
            app.manage(icon_cache);

            let db = crate::state::db::Db::new().expect("Failed to initialize database");
            let settings = crate::config::load();
            let danger_accept_invalid_certs = settings.danger_accept_invalid_certs;
            let worker_pool = crate::worker::WorkerPool::new(8, event_tx.clone(), danger_accept_invalid_certs);
            let state = Arc::new(AppState {
                db,
                worker_pool,
                logger,
                app_handle: app.handle().clone(),
                runtime: crate::state::runtime::DownloadManagerState::new(),
            });
            let ev_state = state.clone();
            let flush_state = state.clone();
            app.manage(state);

            // Hide from Dock (macOS menu-bar only app)
            #[cfg(target_os = "macos")]
            {
                use objc2::msg_send;
                use objc2::runtime::Object;
                let cls = objc2::class!(NSApplication);
                let ns_app: *mut Object = unsafe { msg_send![cls, sharedApplication] };
                // NSApplicationActivationPolicyAccessory = 1 (no Dock icon)
                let _: () = unsafe { msg_send![ns_app, setActivationPolicy: 1i64] };
            }

            // Set window title and intercept close → hide to tray
            let handle = app.handle();
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_title(&format!("ProxyDownloadManager {}", handle.package_info().version));
                if !silent_start {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            // Spawn event handler: listens for download events, updates DB
            tauri::async_runtime::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    ev_state.handle_event(event).await;
                }
            });

            // Forward WebSocket download requests to main window
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                while let Some(req) = request_rx.recv().await {
                    eprintln!("[ProxyDM consumer] Received request_rx: url={}", req.url);

                    // Activate app before emitting so the new window can
                    // steal focus from the browser.
                    // macOS needs NSApp activation (strict focus policy).
                    // Windows/Linux handle this via the normal window creation path.
                    #[cfg(target_os = "macos")]
                    {
                        use objc2::msg_send;
                        use objc2::runtime::Object;
                        let cls = objc2::class!(NSApplication);
                        let ns_app: *mut Object = unsafe { msg_send![cls, sharedApplication] };
                        let _: () = unsafe { msg_send![ns_app, activateIgnoringOtherApps: true] };
                    }

                    let result = app_handle.emit("browser-download-url", &req.url);
                    eprintln!("[ProxyDM consumer] emit result: {:?}", result);
                }
                eprintln!("[ProxyDM consumer] request_rx stream ended!");
            });

            let ev_tx = event_tx;
            let req_tx = request_tx;
            std::thread::spawn(move || {
                let mut server = crate::ws::server::WsServer::new(ev_tx, req_tx);
                if let Err(e) = server.start("127.0.0.1:18999") {
                    eprintln!("WS server error: {}", e);
                }
            });

            let recovery_state = flush_state.clone();

            // Background task: flush runtime progress to DB every 5 seconds
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let flushed = flush_state.runtime.flush_to_db(&flush_state.db);
                    if flushed > 0 {
                        eprintln!("[ProxyDM] Flushed {} progress entries to DB", flushed);
                    }
                }
            });

            // Crash recovery: re-queue all incomplete downloads
            {
                if let Ok(items) = recovery_state.db.list_downloads() {
                    let mut changed = false;
                    for mut item in items.into_iter() {
                        if matches!(item.status, crate::types::DownloadStatus::Downloading) {
                            item.status = crate::types::DownloadStatus::Paused;
                            let _ = recovery_state.db.update_download(&item);
                            changed = true;
                        }
                    }
                    if changed {
                        eprintln!("[ProxyDM] Crash recovery: paused stale downloads");
                    }
                }
            }

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
            cmd::get_file_icon,
            cmd::open_extensions_folder,
            cmd::get_extensions_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
