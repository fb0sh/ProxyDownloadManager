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
mod update;
mod platform;
mod event_bus;
mod services;

use crate::cmd::AppState;
use crate::download_manager::DownloadManager;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tokio::sync::mpsc;

pub(crate) const SILENT_START_ARG: &str = "--silent";

/// Hide from Dock on macOS (menu-bar only app).
#[cfg(target_os = "macos")]
fn hide_from_dock() {
    use objc2::msg_send;
    use objc2::runtime::Object;
    let cls = objc2::class!(NSApplication);
    let ns_app: *mut Object = unsafe { msg_send![cls, sharedApplication] };
    let _: () = unsafe { msg_send![ns_app, setActivationPolicy: 1i64] };
}

/// Configure main window: title, close-to-tray, initial visibility.
fn setup_window(app: &tauri::App, silent_start: bool) {
    let handle = app.handle();
    let Some(window) = handle.get_webview_window("main") else { return };
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

/// Spawn the event handler: receives download events from engines, updates DB, emits to frontend.
fn spawn_event_handler(
    dm: Arc<DownloadManager>,
    bus: Arc<crate::event_bus::EventBus>,
    mut event_rx: mpsc::UnboundedReceiver<crate::types::Event>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            dm.handle_event(event, &bus);
        }
    });
}

/// Spawn the WebSocket request forwarder: receives download requests from browser extensions,
/// activates the app, and emits to the frontend.
fn spawn_ws_forwarder(
    bus: Arc<crate::event_bus::EventBus>,
    mut request_rx: mpsc::UnboundedReceiver<crate::types::PendingDownloadRequest>,
) {
    tauri::async_runtime::spawn(async move {
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

            bus.emit(crate::event_bus::FrontendEvent::BrowserDownloadUrl, req.url.clone());
        }
        eprintln!("[ProxyDM consumer] request_rx stream ended!");
    });
}

/// Start the WebSocket server in a dedicated thread.
fn start_ws_server(
    event_tx: mpsc::UnboundedSender<crate::types::Event>,
    request_tx: mpsc::UnboundedSender<crate::types::PendingDownloadRequest>,
) {
    std::thread::spawn(move || {
        let mut server = crate::ws::server::WsServer::new(event_tx, request_tx);
        if let Err(e) = server.start("127.0.0.1:18999") {
            eprintln!("WS server error: {}", e);
        }
    });
}

/// Spawn the background DB flush loop (every 5 seconds).
fn spawn_flush_loop(dm: Arc<DownloadManager>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let flushed = dm.flush();
            if flushed > 0 {
                eprintln!("[ProxyDM] Flushed {} progress entries to DB", flushed);
            }
        }
    });
}

/// Crash recovery: mark stale "downloading" entries as "paused".
fn crash_recovery(dm: &DownloadManager) {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let silent_start = std::env::args().any(|arg| arg == SILENT_START_ARG);
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (request_tx, request_rx) = mpsc::unbounded_channel::<crate::types::PendingDownloadRequest>();

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
            if let Err(e) = crate::platform::deploy_extensions(app.handle()) {
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

            let bus = Arc::new(crate::event_bus::EventBus::new(app.handle().clone()));
            let state = Arc::new(AppState {
                dm: dm.clone(),
                app_handle: app.handle().clone(),
                bus: bus.clone(),
            });
            app.manage(state);

            // Platform-specific setup
            #[cfg(target_os = "macos")]
            hide_from_dock();

            setup_window(app, silent_start);

            // Register global shortcut from settings
            let shortcut_key = settings.global_shortcut.clone();
            #[cfg(desktop)]
            if !shortcut_key.is_empty() {
                if let Err(e) = app.global_shortcut().register(shortcut_key.as_str()) {
                    eprintln!("[ProxyDM] Failed to register global shortcut '{}': {}", shortcut_key, e);
                }
            }

            // Spawn background tasks
            spawn_event_handler(dm.clone(), bus.clone(), event_rx);
            spawn_ws_forwarder(bus, request_rx);
            start_ws_server(event_tx, request_tx);
            spawn_flush_loop(dm.clone());
            crash_recovery(&dm);

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
            update::check_update,
            cmd::open_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
