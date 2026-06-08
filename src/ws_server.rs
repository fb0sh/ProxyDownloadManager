// =============================================================================
// ws_server.rs — WebSocket server for browser extension communication
//
// Listens on ws://127.0.0.1:18999/ for download URLs from the Edge extension.
// When a URL is received, it's added to the download queue and the main
// application window is brought to front.
// =============================================================================

use crate::types::*;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Start the WebSocket server on a background thread.
pub fn start(
    _state: Arc<Mutex<Vec<DownloadItem>>>,
    focus_request: Arc<AtomicBool>,
    incoming_url: Arc<Mutex<String>>,
) {
    thread::spawn(move || {
        let addr = "127.0.0.1:18999";
        let listener = match TcpListener::bind(addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[proxydm-ws] Failed to bind {}: {}", addr, e);
                return;
            }
        };
        listener.set_nonblocking(true).ok();
        crate::log_info!("WebSocket server listening on ws://{}", addr);

        let mut incoming_buf = Vec::new();

        loop {
            // Accept connections (non-blocking)
            match listener.accept() {
                Ok((stream, addr)) => {
                    crate::log_info!("WebSocket connection from {}", addr);
                    stream.set_nonblocking(false).ok();
                    let peer = addr;

                    let result = tungstenite::accept(stream);
                    match result {
                        Ok(mut ws) => {
                            crate::log_info!("WebSocket handshake OK from {}", peer);
                            loop {
                                match ws.read() {
                                    Ok(tungstenite::Message::Text(text)) => {
                                        incoming_buf.clear();
                                        incoming_buf.extend_from_slice(text.as_bytes());
                                        process_message(
                                            &incoming_buf,
                                            &focus_request,
                                            &incoming_url,
                                        );
                                    }
                                    Ok(tungstenite::Message::Close(_)) => {
                                        crate::log_info!("WebSocket closed by {}", peer);
                                        break;
                                    }
                                    Ok(tungstenite::Message::Ping(data)) => {
                                        let _ = ws.send(tungstenite::Message::Pong(data));
                                    }
                                    Err(e) => {
                                        crate::log_info!("WebSocket error from {}: {}", peer, e);
                                        break;
                                    }
                                    _ => {} // ignore other message types
                                }
                            }
                        }
                        Err(e) => {
                            crate::log_info!("WebSocket handshake failed from {}: {}", peer, e);
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection pending — sleep and retry
                    thread::sleep(std::time::Duration::from_millis(500));
                }
                Err(e) => {
                    crate::log_info!("WebSocket accept error: {}", e);
                    thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }
    });
}

/// Parse an incoming JSON message and add a download.
fn process_message(
    data: &[u8],
    focus_request: &Arc<AtomicBool>,
    incoming_url: &Arc<Mutex<String>>,
) {
    #[derive(serde::Deserialize)]
    struct Incoming {
        #[serde(default)]
        action: String,
        #[serde(default)]
        url: String,
        #[serde(default)]
        referrer: String,
        #[serde(default)]
        tab_title: String,
    }

    let msg: Incoming = match serde_json::from_slice(data) {
        Ok(m) => m,
        Err(e) => {
            crate::log_info!("WS: invalid JSON: {}", e);
            return;
        }
    };

    let url = if !msg.url.is_empty() {
        msg.url
    } else if msg.action == "add" {
        // action-only message without url field
        return;
    } else {
        // maybe the JSON root is just a URL string?
        if let Ok(s) = std::str::from_utf8(data) {
            let s = s.trim().trim_matches('"');
            if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ftp://") {
                s.to_string()
            } else {
                return;
            }
        } else {
            return;
        }
    };

    // Validate URL
    if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("ftp://") {
        crate::log_info!("WS: invalid URL scheme: {}", url);
        return;
    }

    crate::log_info!("WS: received download URL: {}", url);

    // Store URL for the UI to pick up and show in the New Download dialog
    {
        let mut u = incoming_url.lock().unwrap();
        *u = url.clone();
    }

    // Signal the main window: come to front + open New Download dialog
    focus_request.store(true, Ordering::Relaxed);

    crate::log_info!("WS: stored URL for UI confirmation: {}", url);
}


