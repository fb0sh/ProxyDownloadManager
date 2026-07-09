use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    tray::TrayIconEvent,
    tray::MouseButton,
    tray::MouseButtonState,
    AppHandle, Runtime, Manager,
};

pub fn build_tray<R: Runtime>(app: &AppHandle<R>, _shortcut: &str) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show ProxyDM", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    // Dedicated tray icon (monochrome download arrow, 32x32)
    let tray_icon = tauri::image::Image::new(
        include_bytes!("../icons/tray-icon.rgba"),
        32,
        32,
    );
    TrayIconBuilder::new()
        .icon(tray_icon)
        .icon_as_template(true)
        .tooltip("ProxyDM")
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up, ..
            } = event {
                let app: &AppHandle<R> = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _: Result<(), _> = window.show();
                    let _: Result<(), _> = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
