use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
        // A claim with an empty URL must stay a claim so the socket can reject
        // it. Falling through would treat the JSON text as a raw download URL.
        if !req.url.is_empty() || req.action.eq_ignore_ascii_case("claim") {
            if !req.url.is_empty() {
                req.headers = filter_headers(&req.headers);
            }
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
            protocol_version: if i.protocol_version == 0 {
                1
            } else {
                i.protocol_version
            },
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

/// First frame after the socket is up. The extension shows `version` and
/// must not treat this frame as a download ACK.
pub fn desktop_hello() -> String {
    serde_json::json!({
        "type": "hello",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": 1,
    })
    .to_string()
}

pub fn ack_json(ack: &DownloadAck) -> String {
    serde_json::to_string(ack)
        .unwrap_or_else(|_| r#"{"accepted":false,"reason":"serialize"}"#.into())
}

/// What the socket should do with one inbound message.
/// `Claim` only confirms the desktop is up. It must not open a download.
#[derive(Debug, PartialEq, Eq)]
enum WsRoute {
    RejectEmpty,
    Claim,
    DuplicateAdd,
    Forward,
}

const ADD_SEEN_TTL: Duration = Duration::from_secs(120);
static ADD_SEEN: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// First `add` for a request id is forwarded. A retry ACKs and does not open
/// a second window. Empty ids (legacy clients) always forward.
fn add_should_forward(request_id: &str) -> bool {
    if request_id.is_empty() {
        return true;
    }
    let mut seen = ADD_SEEN.lock().unwrap_or_else(|poison| poison.into_inner());
    let now = Instant::now();
    seen.retain(|_, at| now.duration_since(*at) < ADD_SEEN_TTL);
    if seen.contains_key(request_id) {
        return false;
    }
    seen.insert(request_id.to_string(), now);
    true
}

fn route_request(req: &PendingDownloadRequest) -> WsRoute {
    if req.action.eq_ignore_ascii_case("claim") {
        return if req.url.is_empty() {
            WsRoute::RejectEmpty
        } else {
            WsRoute::Claim
        };
    }
    if req.url.is_empty() {
        return WsRoute::RejectEmpty;
    }
    if add_should_forward(&req.request_id) {
        WsRoute::Forward
    } else {
        WsRoute::DuplicateAdd
    }
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

        if let Err(e) = ws.send(Message::Text(desktop_hello().into())) {
            log::error!("[WS] hello send failed: {}", e);
            return;
        }

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
                    let max_preview = preview
                        .char_indices()
                        .nth(200)
                        .map(|(i, _)| i)
                        .unwrap_or(preview.len());
                    log::info!("[ProxyDM WS] Received: {}", &preview[..max_preview]);

                    let request = parse_message(&text);
                    let request_id = request.request_id.clone();

                    match route_request(&request) {
                        WsRoute::RejectEmpty => {
                            let ack = DownloadAck::fail(&request_id, "empty url");
                            let _ = ws.send(Message::Text(ack_json(&ack).into()));
                            continue;
                        }
                        WsRoute::Claim | WsRoute::DuplicateAdd => {
                            // Claim does not create a download. A repeated add
                            // already opened the window; ACK so the extension
                            // does not start a second browser download.
                            let ack = DownloadAck::ok(&request_id);
                            if let Err(e) = ws.send(Message::Text(ack_json(&ack).into())) {
                                log::error!("[ProxyDM WS] ack send ERROR: {:?}", e);
                                break;
                            }
                            continue;
                        }
                        WsRoute::Forward => {}
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
                            let ack =
                                DownloadAck::fail(&request_id, "desktop not accepting downloads");
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
        // Framing headers are dropped; a browser-managed one is replayed, since
        // an origin can key on it and dropping it is how a request that worked
        // in the tab comes back 403.
        assert!(!req.headers.keys().any(|k| k.eq_ignore_ascii_case("host")));
        assert!(req.headers.contains_key("Sec-Fetch-Mode"));
    }

    #[test]
    fn claim_acks_without_becoming_an_add() {
        let id = format!("claim-{}", std::process::id());
        let claim = parse_message(&format!(
            r#"{{"action":"claim","request_id":"{id}","url":"https://cdn.example/a.txt","filename":"a.txt"}}"#
        ));
        assert_eq!(claim.action, "claim");
        assert_eq!(route_request(&claim), WsRoute::Claim);
        assert_eq!(route_request(&claim), WsRoute::Claim);

        let add = parse_message(&format!(
            r#"{{"action":"add","request_id":"{id}","url":"https://cdn.example/a.txt","filename":"a.txt"}}"#
        ));
        assert_eq!(route_request(&add), WsRoute::Forward);
        assert_eq!(route_request(&add), WsRoute::DuplicateAdd);
    }

    #[test]
    fn empty_claim_is_rejected() {
        let req = parse_message(r#"{"action":"claim","request_id":"e1","url":""}"#);
        assert_eq!(req.action, "claim");
        assert!(req.url.is_empty());
        assert_eq!(route_request(&req), WsRoute::RejectEmpty);
    }

    #[test]
    fn legacy_add_without_request_id_always_forwards() {
        let req = parse_message(
            r#"{"action":"add","url":"https://cdn.example/b.zip","filename":"b.zip"}"#,
        );
        assert!(req.request_id.is_empty());
        assert_eq!(route_request(&req), WsRoute::Forward);
        assert_eq!(route_request(&req), WsRoute::Forward);
    }

    #[test]
    fn hello_carries_the_desktop_version_and_is_not_an_ack() {
        let hello: serde_json::Value = serde_json::from_str(&desktop_hello()).unwrap();
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(hello["protocol_version"], 1);
        assert!(hello.get("accepted").is_none());
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
