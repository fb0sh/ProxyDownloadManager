// =============================================================================
// window_focus.rs — Cross-platform window activation for egui/eframe apps
//
// Brings the application window to the front, restores if minimized,
// and acquires input focus.  Designed to be called from any thread
// (e.g. a WebSocket message handler running on a background thread).
//
// Strategy (three levels, all attempted):
//   1. egui::Context::send_viewport_cmd(Focus)  — thread-safe, works on macOS
//      and Linux (X11 _NET_ACTIVE_WINDOW / Wayland xdg_activation_v1).
//   2. Platform-specific native fallbacks:
//      - Windows:  AttachThreadInput + SetForegroundWindow + HWND_TOPMOST trick
//      - macOS:    osascript "System Events" to make the app frontmost
//      - Linux:    wmctrl CLI, then X11 _NET_ACTIVE_WINDOW / XRaiseWindow
// =============================================================================

use std::sync::OnceLock;

/// Global handle to the egui rendering context, set once during startup.
static EGUI_CTX: OnceLock<egui::Context> = OnceLock::new();

/// Register the egui context so that [`bring_window_to_front`] can dispatch
/// `ViewportCommand::Focus` from any thread.  Call this **once** from the
/// `eframe::run_native` creation closure AND/OR from the first `ui()` call
/// (setting is idempotent).
///
/// # Example
/// ```ignore
/// eframe::run_native(APP_NAME, options, Box::new(|cc| {
///     window_focus::register_egui_context(cc.egui_ctx.clone());
///     Ok(Box::new(MyApp::new()))
/// }))
/// ```
pub fn register_egui_context(ctx: egui::Context) {
    let _ = EGUI_CTX.set(ctx);
}

/// Bring the application window to the front of the desktop.
///
/// * If the window is minimised it will be restored.
/// * The window will be raised above all peers.
/// * Input focus will be acquired (works reliably on macOS, X11, and Windows;
///   Wayland may still require workspace visibility).
///
/// **Thread-safe** — may be called from any thread, including background
/// WebSocket / TCP / HTTP server handlers.
pub fn request_repaint() {
    if let Some(ctx) = EGUI_CTX.get() {
        ctx.request_repaint();
    }
}

/// Restore the root viewport enough for eframe to render child viewports.
/// Used by browser-download interception: a minimized root viewport may not
/// repaint until it is restored, so `show_viewport_immediate` for New Download
/// would otherwise be delayed until the user manually restores the app.
pub fn restore_for_child_window() {
    if let Some(ctx) = EGUI_CTX.get() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.request_repaint();
    }
}

pub fn bring_window_to_front() {
    // ── Level 1 — egui's built-in viewport command ──────────────────────────
    // This is thread-safe because egui::Context is Send + Sync (internally an
    // Arc) and send_viewport_cmd queues the command for the next frame.
    if let Some(ctx) = EGUI_CTX.get() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    // ── Level 2 — platform-specific native APIs ────────────────────────────
    #[cfg(target_os = "windows")]
    bring_to_front_windows();

    #[cfg(target_os = "macos")]
    bring_to_front_macos();

    #[cfg(target_os = "linux")]
    bring_to_front_linux();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Windows  —  raw FFI (no windows crate dependency needed)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod win_impl {
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    // ── Win32 type aliases ──────────────────────────────────────────────
    type HWND = *mut std::ffi::c_void;
    type BOOL = i32;
    type DWORD = u32;
    type HANDLE = *mut std::ffi::c_void;

    const FALSE: BOOL = 0;
    const TRUE: BOOL = 1;

    // ShowWindow commands
    const SW_HIDE: i32 = 0;
    const SW_SHOWNORMAL: i32 = 1;
    const SW_SHOW: i32 = 5;
    const SW_MINIMIZE: i32 = 6;
    const SW_RESTORE: i32 = 9;
    const SW_SHOWDEFAULT: i32 = 10;

    // SetWindowPos flags
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const HWND_TOPMOST: isize = -1;
    const HWND_NOTOPMOST: isize = -2;
    const HWND_TOP: isize = 0;

    // ── Imported Win32 functions ────────────────────────────────────────
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: Option<
                unsafe extern "system" fn(HWND, isize) -> BOOL,
            >,
            lParam: isize,
        ) -> BOOL;

        fn GetWindowThreadProcessId(
            hWnd: HWND,
            lpdwProcessId: *mut DWORD,
        ) -> DWORD;

        fn GetCurrentThreadId() -> DWORD;
        fn GetForegroundWindow() -> HWND;
        fn SetForegroundWindow(hWnd: HWND) -> BOOL;
        fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
        fn IsIconic(hWnd: HWND) -> BOOL;
        fn IsWindowVisible(hWnd: HWND) -> BOOL;
        fn SetFocus(hWnd: HWND) -> HWND;

        fn SetWindowPos(
            hWnd: HWND,
            hWndInsertAfter: HWND,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> BOOL;

        fn AttachThreadInput(
            idAttach: DWORD,
            idAttachTo: DWORD,
            fAttach: BOOL,
        ) -> BOOL;

        fn BringWindowToTop(hWnd: HWND) -> BOOL;
    }

    // ── Globals for the EnumWindows callback ────────────────────────────
    static HWND_CACHE: AtomicUsize = AtomicUsize::new(0);
    static CACHE_PID: AtomicU32 = AtomicU32::new(0);

    /// Callback for `EnumWindows` — finds the first visible window belonging
    /// to `CACHE_PID` and stores its HWND in `HWND_CACHE`.
    unsafe extern "system" fn enum_window_proc(hwnd: HWND, _lparam: isize) -> BOOL {
        let target_pid = CACHE_PID.load(Ordering::Relaxed);
        if target_pid == 0 {
            return FALSE; // stop (guard against uninitialised state)
        }
        let mut pid: DWORD = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == target_pid && IsWindowVisible(hwnd) != FALSE {
            HWND_CACHE.store(hwnd as usize, Ordering::SeqCst);
            return FALSE; // found → stop enumeration
        }
        TRUE // continue
    }

    /// Find and cache the main window's HWND.  Safe to call repeatedly.
    pub(crate) fn cache_hwnd() {
        if HWND_CACHE.load(Ordering::Relaxed) != 0 {
            return; // already cached
        }
        unsafe {
            CACHE_PID.store(std::process::id(), Ordering::Relaxed);
            EnumWindows(Some(enum_window_proc), 0);
        }
    }

    /// Bring the main window to the front.
    ///
    /// The Windows `SetForegroundWindow` API has a well-known restriction:
    /// it only succeeds if the calling thread was the last to receive user
    /// input.  We work around this with a three-pronged attack:
    ///
    /// 1. `AttachThreadInput` — attach our thread to the foreground window's
    ///    input queue so `SetForegroundWindow` believes we're legitimate.
    /// 2. `SetWindowPos(HWND_TOPMOST…)` / `(HWND_NOTOPMOST…)` — the
    ///    "topmost trick" forces the window above all others even when the
    ///    standard path fails.
    /// 3. `BringWindowToTop` / `SetFocus` — final low-level attempts.
    pub(crate) fn bring_to_front() {
        unsafe {
            cache_hwnd();
            let hwnd = HWND_CACHE.load(Ordering::Acquire) as HWND;
            if hwnd.is_null() {
                return;
            }

            // ── If minimised, restore ─────────────────────────────────────
            if IsIconic(hwnd) != FALSE {
                ShowWindow(hwnd, SW_RESTORE);
            }

            // ── Ensure visible ────────────────────────────────────────────
            ShowWindow(hwnd, SW_SHOW);

            // ── AttachThreadInput workaround ───────────────────────────────
            let fg_hwnd = GetForegroundWindow();
            let fg_thread = GetWindowThreadProcessId(fg_hwnd, std::ptr::null_mut());
            let cur_thread = GetCurrentThreadId();

            if fg_thread != cur_thread && fg_thread != 0 {
                // Attach input queues so SetForegroundWindow "thinks" we are
                // the foreground thread.
                AttachThreadInput(fg_thread, cur_thread, TRUE);
                SetForegroundWindow(hwnd);
                AttachThreadInput(fg_thread, cur_thread, FALSE);
            } else {
                SetForegroundWindow(hwnd);
            }

            // ── Topmost trick (always works) ──────────────────────────────
            // HWND_TOPMOST forces the window above all non-topmost peers
            // even when SetForegroundWindow is blocked.
            SetWindowPos(
                hwnd,
                HWND_TOPMOST as HWND,
                0, 0, 0, 0,
                SWP_NOSIZE | SWP_NOMOVE,
            );
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST as HWND,
                0, 0, 0, 0,
                SWP_NOSIZE | SWP_NOMOVE,
            );

            // ── Final attempts ────────────────────────────────────────────
            BringWindowToTop(hwnd);
            SetFocus(hwnd);
        }
    }
}

#[cfg(target_os = "windows")]
fn bring_to_front_windows() {
    win_impl::bring_to_front();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  macOS  —  osascript / AppleScript
// ═══════════════════════════════════════════════════════════════════════════════
//
// The most reliable way to activate an app on macOS from any context is via
// osascript.  `activateIgnoringOtherApps` is already dispatched by winit
// when egui sends ViewportCommand::Focus, but osascript provides a
// belt-and-braces approach that also works when the egui context has not
// been registered yet (e.g. during early startup).

#[cfg(target_os = "macos")]
fn bring_to_front_macos() {
    let pid = std::process::id();
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "tell application \"System Events\" to set frontmost of \
             every process whose unix id is {} to true",
            pid
        ))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Linux  —  wmctrl  +  X11 (raw FFI)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Primary:   `wmctrl -a <window-title>`  — works on most WMs (both X11 and
//            Wayland via XWayland).
// Fallback:  Raw X11 (Xlib) FFI when wmctrl is absent and DISPLAY is set.

#[cfg(target_os = "linux")]
fn bring_to_front_linux() {
    // ── Attempt 1: wmctrl ──────────────────────────────────────────────────
    // wmctrl is widely available on desktop Linux; `wmctrl -a <name>` raises
    // and focuses the first window whose title contains <name>.
    let app_name = env!("CARGO_PKG_NAME");
    let ok = std::process::Command::new("wmctrl")
        .arg("-a")
        .arg(app_name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        return; // wmctrl handled it
    }

    // ── Attempt 2: X11 native (raw FFI, no crate needed) ──────────────────
    // Only try if DISPLAY is set (i.e. X11 or XWayland is available).
    if std::env::var("DISPLAY").is_err() {
        return; // pure Wayland — stick with egui ViewportCommand::Focus
    }

    // Minimal Xlib FFI declarations (only what we need).
    type Display = std::ffi::c_void;
    type XID = u64; // Window, Atom, etc. are all u64 on 64-bit

    extern "C" {
        fn XOpenDisplay(display_name: *const i8) -> *mut Display;
        fn XDefaultRootWindow(dpy: *mut Display) -> XID;
        fn XInternAtom(dpy: *mut Display, name: *const i8, only_if_exists: i32) -> XID;
        fn XGetWindowProperty(
            dpy: *mut Display,
            w: XID,
            property: XID,
            long_offset: i64,
            long_length: i64,
            delete: i32,
            req_type: XID,
            actual_type_return: *mut XID,
            actual_format_return: *mut i32,
            nitems_return: *mut u64,
            bytes_after_return: *mut u64,
            prop_return: *mut *mut u8,
        ) -> i32;
        fn XFree(data: *mut std::ffi::c_void) -> i32;
        fn XQueryTree(
            dpy: *mut Display,
            w: XID,
            root_return: *mut XID,
            parent_return: *mut XID,
            children_return: *mut *mut XID,
            nchildren_return: *mut u32,
        ) -> i32;
        fn XMapRaised(dpy: *mut Display, w: XID) -> i32;
        fn XRaiseWindow(dpy: *mut Display, w: XID) -> i32;
        fn XSetInputFocus(
            dpy: *mut Display,
            w: XID,
            revert_to: i32,
            time: u64,
        ) -> i32;
        fn XCloseDisplay(dpy: *mut Display) -> i32;
        fn XFlush(dpy: *mut Display) -> i32;
        fn XSendEvent(
            dpy: *mut Display,
            w: XID,
            propagate: i32,
            event_mask: i64,
            event_send: *mut i8,
        ) -> i32;
        fn XMapWindow(dpy: *mut Display, w: XID) -> i32;
    }

    unsafe {
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return;
        }

        let root = XDefaultRootWindow(display);
        let mut root_ret: XID = 0;
        let mut parent: XID = 0;
        let mut children: *mut XID = std::ptr::null_mut();
        let mut nchildren: u32 = 0;

        let found = if XQueryTree(display, root, &mut root_ret, &mut parent, &mut children, &mut nchildren) != 0
        {
            let pid_atom = XInternAtom(
                display,
                std::ffi::CStr::from_bytes_with_nul(b"_NET_WM_PID\0")
                    .unwrap()
                    .as_ptr(),
                0,
            );
            let target_pid = std::process::id() as u64;
            let count = nchildren as usize;
            let wins = std::slice::from_raw_parts(children, count);

            let mut our_win: Option<XID> = None;

            for &win in wins {
                if win == 0 {
                    continue;
                }
                let mut actual_type: XID = 0;
                let mut actual_format: i32 = 0;
                let mut nitems: u64 = 0;
                let mut bytes_after: u64 = 0;
                let mut prop_data: *mut u8 = std::ptr::null_mut();

                let status = XGetWindowProperty(
                    display,
                    win,
                    pid_atom,
                    0,
                    1,
                    0, // delete = False
                    0, // AnyPropertyType
                    &mut actual_type,
                    &mut actual_format,
                    &mut nitems,
                    &mut bytes_after,
                    &mut prop_data,
                );

                if status == 0 && actual_type != 0 && actual_format == 32 && nitems > 0 {
                    let pid_value = *(prop_data as *const u64);
                    XFree(prop_data as *mut std::ffi::c_void);
                    if pid_value == target_pid {
                        our_win = Some(win);
                        break;
                    }
                } else if !prop_data.is_null() {
                    XFree(prop_data as *mut std::ffi::c_void);
                }
            }

            if !children.is_null() {
                XFree(children as *mut std::ffi::c_void);
            }

            our_win
        } else {
            None
        };

        if let Some(win) = found {
            // Ensure the window is mapped (visible).
            XMapWindow(display, win);
            // Raise to the top of the stacking order.
            XMapRaised(display, win);
            XRaiseWindow(display, win);

            // Send _NET_ACTIVE_WINDOW (ClientMessage) to the window manager.
            let net_active = XInternAtom(
                display,
                std::ffi::CStr::from_bytes_with_nul(b"_NET_ACTIVE_WINDOW\0")
                    .unwrap()
                    .as_ptr(),
                0,
            );

            // Build an XClientMessageEvent  (24 × u64 is plenty of room).
            let mut ev: [u64; 24] = [0; 24];
            ev[0] = 33;           // type = ClientMessage
            ev[4] = win;          // window
            ev[5] = net_active;   // message_type
            ev[6] = 32;           // format (32-bit data)
            ev[7] = 2;            // l[0] = source indication (2 = pager/app)
            ev[8] = 0;            // l[1] = timestamp (CurrentTime = 0)

            XSendEvent(
                display,
                root,
                0, // propagate = False
                // SubstructureRedirectMask | SubstructureNotifyMask
                0x400000 | 0x80000,
                &mut ev as *mut _ as *mut i8,
            );

            XFlush(display);

            // Low-level input focus.
            XSetInputFocus(display, win, 1 /* RevertToParent */, 0 /* CurrentTime */);
            XFlush(display);
        }

        XCloseDisplay(display);
    }
}
