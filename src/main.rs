// =============================================================================
// ProxyDM — A download manager built with egui (Rust)
// Features: multi-threaded downloads, pause/resume, proxy support,
//           persistent state, tree-filtered table view, system file icons.
// =============================================================================

// Module declarations
mod app;
mod download;
mod icons;
mod logger;
mod persist;
mod types;
mod ui;
mod window_focus;
mod ws_server;

use app::ProxyDownloadManager;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

fn main() -> Result<(), eframe::Error> {
    // ── Shared state for WebSocket + UI ──
    let ws_focus = Arc::new(AtomicBool::new(false));
    let ws_url = Arc::new(Mutex::new(String::new()));
    let shared_state = Arc::new(Mutex::new(Vec::new()));

    // ── Start WebSocket server for browser extension ──
    ws_server::start(
        shared_state.clone(),
        ws_focus.clone(),
        ws_url.clone(),
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title(&format!("Proxy Download Manager v{}", env!("CARGO_PKG_VERSION"))),
        ..Default::default()
    };

    eframe::run_native(
        types::APP_NAME,
        options,
        Box::new(|cc| {
            // Register the egui context so bring_window_to_front() works from background threads
            window_focus::register_egui_context(cc.egui_ctx.clone());
            Ok(Box::new(ProxyDownloadManager::new_with_state(
                shared_state.clone(),
                ws_focus,
                ws_url,
            )))
        }),
    )
}
