use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tungstenite::Message;

use crate::types::{Event, PendingDownloadRequest};

/// Parse a WebSocket text message into a PendingDownloadRequest.
/// Three-tier fallback:
/// 1. Browser extension JSON: { action, url, filename, proxy_name, connections }
/// 2. Direct PendingDownloadRequest JSON
/// 3. Raw text treated as URL
pub fn parse_message(text: &str) -> PendingDownloadRequest {
    #[derive(serde::Deserialize)]
    struct Incoming {
        #[serde(default)]
        action: String,
        #[serde(default)]
        url: String,
        #[serde(default)]
        filename: String,
        #[serde(default)]
        proxy_name: String,
        connections: Option<u32>,
    }

    serde_json::from_str::<Incoming>(text)
        .ok()
        .filter(|i| !i.url.is_empty())
        .map(|i| PendingDownloadRequest {
            url: i.url,
            filename: i.filename,
            proxy_name: i.proxy_name,
            connections: i.connections.unwrap_or(1),
        })
        .or_else(|| {
            serde_json::from_str::<PendingDownloadRequest>(text).ok()
        })
        .unwrap_or_else(|| {
            let filename = text.rsplit('/').next().unwrap_or("").to_string();
            PendingDownloadRequest {
                url: text.to_string(),
                filename,
                proxy_name: String::new(),
                connections: 1,
            }
        })
}

pub struct WsServer {
    event_tx: tokio::sync::mpsc::UnboundedSender<Event>,
    request_tx: tokio::sync::mpsc::UnboundedSender<PendingDownloadRequest>,
    stop_flag: Arc<AtomicBool>,
}

impl WsServer {
    pub fn new(
        event_tx: tokio::sync::mpsc::UnboundedSender<Event>,
        request_tx: tokio::sync::mpsc::UnboundedSender<PendingDownloadRequest>,
    ) -> Self {
        Self {
            event_tx,
            request_tx,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self, addr: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;

        let stop_flag = Arc::clone(&self.stop_flag);
        let event_tx = self.event_tx.clone();
        let request_tx = self.request_tx.clone();
        let addr = addr.to_string();

        thread::spawn(move || {
            log::info!("[WS] Server listening on {}", addr);

            for stream in listener.incoming() {
                if stop_flag.load(Ordering::Relaxed) {
                    log::info!("[WS] Server stopping");
                    break;
                }

                match stream {
                    Ok(stream) => {
                        // Set the accepted stream to BLOCKING mode.
                        // The listener is non-blocking for the accept loop,
                        // but connection handling needs blocking I/O.
                        let _ = stream.set_nonblocking(false);
                        let et = event_tx.clone();
                        let rt = request_tx.clone();
                        thread::spawn(move || {
                            Self::handle_connection(stream, et, rt);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        log::error!("[WS] Listener error: {}", e);
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn handle_connection(
        stream: std::net::TcpStream,
        event_tx: tokio::sync::mpsc::UnboundedSender<Event>,
        request_tx: tokio::sync::mpsc::UnboundedSender<PendingDownloadRequest>,
    ) {
        let peer = stream.peer_addr().ok();
        log::info!("[WS] New connection from {:?}", peer);

        let mut ws = match tungstenite::accept(stream) {
            Ok(ws) => ws,
            Err(e) => {
                log::error!("[WS] Handshake failed: {}", e);
                return;
            }
        };

        loop {
            let msg = match ws.read() {
                Ok(msg) => msg,
                Err(tungstenite::Error::ConnectionClosed) => {
                    log::info!("[WS] Connection closed from {:?}", peer);
                    break;
                }
                Err(tungstenite::Error::Protocol(msg)) => {
                    log::error!("[WS] Protocol error from {:?}: {}", peer, msg);
                    break;
                }
                Err(e) => {
                    log::error!("[WS] Read error from {:?}: {}", peer, e);
                    break;
                }
            };

            match msg {
                Message::Text(text) => {
                    let max_preview = text.char_indices().nth(200).map(|(i, _)| i).unwrap_or(text.len());
                    log::info!("[ProxyDM WS] Received: {}", &text[..max_preview]);

                    let request = parse_message(&text);

                    log::info!("[ProxyDM WS] Sending to request_tx... url={}", request.url);

                    if let Err(e) = request_tx.send(request) {
                        log::error!("[ProxyDM WS] request_tx.send ERROR: {:?}", e);
                        break;
                    }

                    log::info!("[ProxyDM WS] request_tx.send OK");

                    log::info!("[ProxyDM WS] event_tx OK, sending ack...");
                    if let Err(e) = ws.send(Message::Text(r#"{"status":"ok"}"#.into())) {
                        log::error!("[ProxyDM WS] ack send ERROR: {:?}", e);
                        break;
                    }
                    log::info!("[ProxyDM WS] All done, connection handling complete.");
                }
                Message::Close(_) => {
                    log::info!("[WS] Peer requested close from {:?}", peer);
                    break;
                }
                Message::Ping(data) => {
                    if let Err(e) = ws.send(Message::Pong(data)) {
                        log::error!("[WS] Failed to send pong: {}", e);
                        break;
                    }
                }
                Message::Pong(_) => {
                    // Ignore unsolicited pong
                }
                Message::Binary(_) => {
                    log::warn!("[WS] Unexpected binary message from {:?}", peer);
                }
                Message::Frame(_) => {
                    // Internal frame type, ignore
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_browser_extension_json() {
        let json = r#"{"action":"add","url":"https://example.com/file.zip","filename":"file.zip","proxy_name":"my-proxy","connections":8}"#;
        let req = parse_message(json);
        assert_eq!(req.url, "https://example.com/file.zip");
        assert_eq!(req.filename, "file.zip");
        assert_eq!(req.proxy_name, "my-proxy");
        assert_eq!(req.connections, 8);
    }

    #[test]
    fn test_parse_pending_request_json() {
        let json = r#"{"url":"https://example.com/data.bin","filename":"data.bin","proxy_name":"","connections":4}"#;
        let req = parse_message(json);
        assert_eq!(req.url, "https://example.com/data.bin");
        assert_eq!(req.filename, "data.bin");
        assert_eq!(req.connections, 4);
    }

    #[test]
    fn test_parse_raw_url() {
        let url = "https://cdn.example.com/video.mp4";
        let req = parse_message(url);
        assert_eq!(req.url, url);
        assert_eq!(req.filename, "video.mp4");
        assert_eq!(req.connections, 1);
        assert!(req.proxy_name.is_empty());
    }

    #[test]
    fn test_parse_empty_json_falls_back_to_raw() {
        let json = r#"{"action":"","url":""}"#;
        let req = parse_message(json);
        assert_eq!(req.url, json);
        assert_eq!(req.connections, 1);
    }

    #[test]
    fn test_parse_browser_json_without_connections() {
        let json = r#"{"action":"add","url":"https://x.com/a.zip","filename":"a.zip"}"#;
        let req = parse_message(json);
        assert_eq!(req.url, "https://x.com/a.zip");
        assert_eq!(req.connections, 1); // default
    }
}
