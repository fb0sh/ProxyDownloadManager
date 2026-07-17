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
mod filename;
mod download_manager;

use crate::cmd::AppState;
use crate::download_manager::DownloadManager;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tokio::sync::mpsc;

pub(crate) const SILENT_START_ARG: &str = "--silent";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let silent_start = std::env::args().any(|arg| arg == SILENT_START_ARG);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<crate::types::PendingDownloadRequest>();

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
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        eprintln!("[ProxyDM] global shortcut pressed: {:?}", shortcut);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // macOS: deploy bundled browser extensions
            #[cfg(target_os = "macos")]
            if let Err(e) = crate::cmd::deploy_extensions(app.handle()) {
                eprintln!("[ProxyDM] Failed to deploy browser extensions: {}", e);
            }

            let db = crate::state::db::Db::new().expect("Failed to initialize database");
            let settings = crate::config::load();

            let _ = crate::tray::build_tray(app.handle(), &settings.global_shortcut);

            let icon_cache = crate::icons::IconCache::new();
            app.manage(icon_cache);

            let danger_accept_invalid_certs = settings.danger_accept_invalid_certs;
            let next_id_start = db.max_id().unwrap_or(0) + 1;
            let worker_pool = crate::worker::WorkerPool::new(8, event_tx.clone(), danger_accept_invalid_certs, next_id_start);
            let logger = crate::log::Logger::new().expect("Failed to initialize logger");

            let dm = Arc::new(DownloadManager::new(
                db,
                worker_pool,
                logger,
                crate::state::runtime::DownloadManagerState::new(),
            ));

            let state = Arc::new(AppState {
                dm: dm.clone(),
                app_handle: app.handle().clone(),
            });
            let ev_state = state.clone();
            let flush_dm = dm.clone();
            app.manage(state);

            // Hide from Dock (macOS menu-bar only app)
            #[cfg(target_os = "macos")]
            {
                use objc2::msg_send;
                use objc2::runtime::Object;
                let cls = objc2::class!(NSApplication);
                let ns_app: *mut Object = unsafe { msg_send![cls, sharedApplication] };
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

            // Register global shortcut from settings
            let shortcut_key = settings.global_shortcut.clone();
            #[cfg(desktop)]
            if !shortcut_key.is_empty() {
                if let Err(e) = app.global_shortcut().register(shortcut_key.as_str()) {
                    eprintln!("[ProxyDM] Failed to register global shortcut '{}': {}", shortcut_key, e);
                }
            }

            // Spawn event handler: listens for download events, updates DB, emits to frontend
            let app_handle_for_events = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                while let Some(event) = event_rx.recv().await {
                    let emitted = ev_state.dm.handle_event(event);
                    for e in emitted {
                        let _ = app_handle_for_events.emit(&e.name, e.payload);
                    }
                }
            });

            // Forward WebSocket download requests to main window
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                while let Some(req) = request_rx.recv().await {
                    eprintln!("[ProxyDM consumer] Received request_rx: url={}", req.url);

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

            // Background task: flush runtime progress to DB every 5 seconds
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let flushed = flush_dm.flush();
                    if flushed > 0 {
                        eprintln!("[ProxyDM] Flushed {} progress entries to DB", flushed);
                    }
                }
            });

            // Crash recovery: re-queue all incomplete downloads
            {
                if let Ok(items) = dm.list_items() {
                    let mut changed = false;
                    for mut item in items.into_iter() {
                        if matches!(item.status, crate::types::DownloadStatus::Downloading) {
                            item.status = crate::types::DownloadStatus::Paused;
                            let _ = dm.update_item(&item);
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
            cmd::check_update,
            cmd::open_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
