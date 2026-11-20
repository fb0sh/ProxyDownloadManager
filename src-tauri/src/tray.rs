use tauri::{
    menu::{Menu, MenuItem},
    tray::MouseButton,
    tray::MouseButtonState,
    tray::TrayIconBuilder,
    tray::TrayIconEvent,
    AppHandle, Manager, Runtime,
};

#[cfg(any(target_os = "macos", test))]
const MACOS_TRAY_ICON_SIZE: u32 = 36;
#[cfg(any(target_os = "macos", test))]
const MACOS_TRAY_ICON_RGBA: &[u8] = include_bytes!("../icons/tray-icon.rgba");

pub fn build_tray<R: Runtime>(
    app: &AppHandle<R>,
    _shortcut: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show ProxyDM", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    // macOS: monochrome template icon (menu bar adapts it to light/dark).
    // 36x36 matches tray-icon's 18pt height 1:1 on @2x Retina displays.
    // Elsewhere the template icon renders as a black-on-dark-taskbar arrow —
    // effectively invisible on Windows — so use the colored app icon instead.
    #[cfg(target_os = "macos")]
    let builder = TrayIconBuilder::new()
        .icon(tauri::image::Image::new(
            MACOS_TRAY_ICON_RGBA,
            MACOS_TRAY_ICON_SIZE,
            MACOS_TRAY_ICON_SIZE,
        ))
        .icon_as_template(true);
    #[cfg(not(target_os = "macos"))]
    let builder = TrayIconBuilder::new().icon(
        app.default_window_icon()
            .cloned()
            .ok_or("no default window icon")?,
    );

    builder
        .tooltip("ProxyDM")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
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
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
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

#[cfg(test)]
mod tests {
    use super::{MACOS_TRAY_ICON_RGBA, MACOS_TRAY_ICON_SIZE};

    #[test]
    fn macos_tray_icon_rgba_matches_dimensions_and_has_opaque_strokes() {
        let expected_bytes = (MACOS_TRAY_ICON_SIZE * MACOS_TRAY_ICON_SIZE * 4) as usize;
        assert_eq!(MACOS_TRAY_ICON_RGBA.len(), expected_bytes);
        let solid_pixels = MACOS_TRAY_ICON_RGBA
            .chunks_exact(4)
            .filter(|px| px[3] == 255)
            .count();
        assert!(
            solid_pixels > 200,
            "expected crisp opaque strokes, got {solid_pixels}"
        );
    }
}
