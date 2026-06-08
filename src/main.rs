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

use app::ProxyDownloadManager;

fn main() -> Result<(), eframe::Error> {
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
        Box::new(|_cc| Ok(Box::new(ProxyDownloadManager::default()))),
    )
}
