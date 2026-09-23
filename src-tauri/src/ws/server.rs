use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tungstenite::Message;

use crate::headers::{filter_headers, redact_log};
use crate::types::{DownloadAck, Event, PendingDownloadRequest};

/// Parse a WebSocket text message into a PendingDownloadRequest.
///
/// Accepts:
/// 1. Structured v1 JSON (`protocol_version`, `request_id`, `url`, headers, …)
/// 2. Legacy browser JSON `{ action, url, filename, proxy_name, connections }`
/// 3. Direct `PendingDownloadRequest` JSON
/// 4. Raw URL text
pub fn parse_message(text: &str) -> PendingDownloadRequest {
    if let Ok(mut req) = serde_json::from_str::<PendingDownloadRequest>(text) {
        if !req.url.is_empty() {
            req.headers = filter_headers(&req.headers);
            if req.connections == 0 && text.contains("\"connections\"") {
                // explicit 0 = Auto; keep it
            }
            return req;
        }
    }

    #[derive(serde::Deserialize)]
    struct Incoming {
        #[serde(default)]
        protocol_version: u32,
        #[serde(default)]
        request_id: String,
        #[serde(default)]
        action: String,
        #[serde(default)]
        url: String,
        #[serde(default)]
        final_url: String,
        #[serde(default)]
        filename: String,
        #[serde(default)]
        method: String,
        #[serde(default)]
        referrer: String,
        #[serde(default)]
        user_agent: String,
        #[serde(default)]
        cookies: String,
        #[serde(default)]
        headers: std::collections::HashMap<String, String>,
        #[serde(default)]
        tab_url: String,
        #[serde(default)]
        content_type: String,
        #[serde(default)]
        content_length: u64,
        #[serde(default)]
        proxy_name: String,
        connections: Option<u32>,
    }

    serde_json::from_str::<Incoming>(text)
        .ok()
        .filter(|i| !i.url.is_empty())
        .map(|i| PendingDownloadRequest {
            protocol_version: if i.protocol_version == 0 { 1 } else { i.protocol_version },
            request_id: i.request_id,
            action: i.action,
            url: i.url,
            final_url: i.final_url,
            filename: i.filename,
            method: i.method,
            referrer: i.referrer,
            user_agent: i.user_agent,
            cookies: i.cookies,
            headers: filter_headers(&i.headers),
            tab_url: i.tab_url,
            content_type: i.content_type,
            content_length: i.content_length,
            proxy_name: i.proxy_name,
            connections: i.connections.unwrap_or(0),
        })
        .unwrap_or_else(|| {
            let filename = text.rsplit('/').next().unwrap_or("").to_string();
            PendingDownloadRequest {
                protocol_version: 0,
                url: text.to_string(),
                filename,
                connections: 0,
                ..Default::default()
            }
        })
}

pub fn ack_json(ack: &DownloadAck) -> String {
    serde_json::to_string(ack).unwrap_or_else(|_| r#"{"accepted":false,"reason":"serialize"}"#.into())
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
        _event_tx: tokio::sync::mpsc::UnboundedSender<Event>,
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
                    let preview = redact_log(&text);
                    let max_preview = preview.char_indices().nth(200).map(|(i, _)| i).unwrap_or(preview.len());
                    log::info!("[ProxyDM WS] Received: {}", &preview[..max_preview]);

                    let request = parse_message(&text);
                    let request_id = request.request_id.clone();

                    if request.url.is_empty() {
                        let ack = DownloadAck::fail(&request_id, "empty url");
                        let _ = ws.send(Message::Text(ack_json(&ack).into()));
                        continue;
                    }

                    log::info!(
                        "[ProxyDM WS] Sending to request_tx... url={} headers={}",
                        request.effective_url(),
                        request.headers.len()
                    );

                    let ack = match request_tx.send(request) {
                        Ok(()) => DownloadAck::ok(&request_id),
                        Err(e) => {
                            log::error!("[ProxyDM WS] request_tx.send ERROR: {:?}", e);
                            let ack = DownloadAck::fail(&request_id, "desktop not accepting downloads");
                            let _ = ws.send(Message::Text(ack_json(&ack).into()));
                            break;
                        }
                    };

                    if let Err(e) = ws.send(Message::Text(ack_json(&ack).into())) {
                        log::error!("[ProxyDM WS] ack send ERROR: {:?}", e);
                        break;
                    }
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
                Message::Pong(_) => {}
                Message::Binary(_) => {
                    log::warn!("[WS] Unexpected binary message from {:?}", peer);
                }
                Message::Frame(_) => {}
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
        assert!(req.proxy_name.is_empty());
    }

    #[test]
    fn test_parse_empty_json_falls_back_to_raw() {
        let json = r#"{"action":"","url":""}"#;
        let req = parse_message(json);
        assert_eq!(req.url, json);
    }

    #[test]
    fn test_parse_browser_json_without_connections() {
        let json = r#"{"action":"add","url":"https://x.com/a.zip","filename":"a.zip"}"#;
        let req = parse_message(json);
        assert_eq!(req.url, "https://x.com/a.zip");
        assert_eq!(req.connections, 0);
    }

    #[test]
    fn test_parse_v1_with_headers() {
        let json = r#"{
            "protocol_version":1,
            "request_id":"abc",
            "action":"add",
            "url":"https://cdn.example/file.zip",
            "final_url":"https://cdn.example/file.zip?sig=1",
            "filename":"file.zip",
            "method":"GET",
            "referrer":"https://example.com/page",
            "user_agent":"Mozilla/5.0",
            "cookies":"sid=1",
            "headers":{"Cookie":"sid=1","Referer":"https://example.com/page","Host":"cdn.example","Sec-Fetch-Mode":"navigate","Authorization":"Bearer x"},
            "tab_url":"https://example.com/page",
            "content_type":"application/zip",
            "content_length":1024
        }"#;
        let req = parse_message(json);
        assert_eq!(req.request_id, "abc");
        assert_eq!(req.final_url, "https://cdn.example/file.zip?sig=1");
        assert!(req.headers.contains_key("Cookie"));
        assert!(req.headers.contains_key("Referer"));
        assert!(req.headers.contains_key("Authorization"));
        assert!(!req.headers.keys().any(|k| k.eq_ignore_ascii_case("host")));
        assert!(!req.headers.keys().any(|k| k.to_ascii_lowercase().starts_with("sec-")));
    }

    #[test]
    fn ack_json_contains_request_id() {
        let ack = DownloadAck::ok("rid-1");
        let s = ack_json(&ack);
        assert!(s.contains("rid-1"));
        assert!(s.contains("\"accepted\":true"));
        let fail = DownloadAck::fail("rid-1", "offline");
        let s = ack_json(&fail);
        assert!(s.contains("\"accepted\":false"));
        assert!(s.contains("offline"));
    }
}
