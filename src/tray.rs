// =============================================================================
// tray.rs — System tray integration and main-window hide/show state
// =============================================================================

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static MAIN_VISIBLE: AtomicBool = AtomicBool::new(true);
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
thread_local! {
    static TRAY_ICON: RefCell<Option<tray_icon::TrayIcon>> = const { RefCell::new(None) };
}
static SHOW_ITEM_ID: OnceLock<String> = OnceLock::new();
static QUIT_ITEM_ID: OnceLock<String> = OnceLock::new();

pub fn is_main_visible() -> bool {
    MAIN_VISIBLE.load(Ordering::Relaxed)
}

pub fn set_main_visible(visible: bool) {
    MAIN_VISIBLE.store(visible, Ordering::Relaxed);
}

pub fn quit_requested() -> bool {
    QUIT_REQUESTED.load(Ordering::Relaxed)
}

fn make_icon() -> anyhow::Result<tray_icon::Icon> {
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let in_circle = {
                let dx = x as i32 - 16;
                let dy = y as i32 - 16;
                dx * dx + dy * dy <= 15 * 15
            };
            if in_circle {
                rgba.extend_from_slice(&[35, 120, 215, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).map_err(Into::into)
}

pub fn init() -> anyhow::Result<()> {
    if TRAY_ICON.with(|tray| tray.borrow().is_some()) {
        return Ok(());
    }

    let menu = tray_icon::menu::Menu::new();
    let show_item = tray_icon::menu::MenuItem::new("Show ProxyDM", true, None);
    let quit_item = tray_icon::menu::MenuItem::new("Quit", true, None);
    let _ = SHOW_ITEM_ID.set(show_item.id().0.clone());
    let _ = QUIT_ITEM_ID.set(quit_item.id().0.clone());
    let _ = menu.append(&show_item);
    let _ = menu.append(&quit_item);

    let tray_icon = tray_icon::TrayIconBuilder::new()
        .with_tooltip("ProxyDM")
        .with_icon(make_icon()?)
        .with_menu(Box::new(menu))
        .build()?;

    TRAY_ICON.with(|tray| {
        *tray.borrow_mut() = Some(tray_icon);
    });
    Ok(())
}

pub fn poll_events(ctx: &egui::Context) {
    use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
    use tray_icon::menu::MenuEvent;

    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }) {
            show_main_window(ctx);
        }
    }

    while let Ok(event) = MenuEvent::receiver().try_recv() {
        let id = event.id.0;
        if SHOW_ITEM_ID.get().map(|s| s == &id).unwrap_or(false) {
            show_main_window(ctx);
        } else if QUIT_ITEM_ID.get().map(|s| s == &id).unwrap_or(false) {
            QUIT_REQUESTED.store(true, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

pub fn hide_main_window(ctx: &egui::Context) {
    MAIN_VISIBLE.store(false, Ordering::Relaxed);
    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);

    // eframe only creates child viewports (New Download) during a root repaint.
    // If the root is minimized or Visible(false), some platforms stop repainting
    // it, so browser-triggered windows are delayed until the user restores the
    // app. Keep the root alive but move it far off-screen instead.
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1.0, 1.0)));
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(-10000.0, -10000.0)));
    ctx.request_repaint();
}

pub fn show_main_window(ctx: &egui::Context) {
    MAIN_VISIBLE.store(true, Ordering::Relaxed);
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(960.0, 600.0)));
    if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
        ctx.send_viewport_cmd(cmd);
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
    crate::window_focus::bring_window_to_front();
}
