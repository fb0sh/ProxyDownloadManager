use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tungstenite::Message;

use crate::types::{Event, EventKind, PendingDownloadRequest};

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
                    eprintln!("[ProxyDM WS] Received: {}", &text[..text.len().min(200)]);

                    // Try parsing as browser extension JSON first:
                    // { action: "add", url: "...", referrer, tab_title, filename, proxy_name, connections }
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

                    let request = serde_json::from_str::<Incoming>(&text)
                        .ok()
                        .filter(|i| !i.url.is_empty())
                        .map(|i| PendingDownloadRequest {
                            url: i.url,
                            filename: i.filename,
                            proxy_name: i.proxy_name,
                            connections: i.connections.unwrap_or(1),
                        })
                        .or_else(|| {
                            // Fallback: try direct PendingDownloadRequest
                            serde_json::from_str::<PendingDownloadRequest>(&text).ok()
                        })
                        .unwrap_or_else(|| {
                            // Last resort: treat raw text as URL
                            let url = text.clone();
                            let filename = url.rsplit('/').next().unwrap_or("").to_string();
                            PendingDownloadRequest {
                                url,
                                filename,
                                proxy_name: String::new(),
                                connections: 1,
                            }
                        });

                    eprintln!("[ProxyDM WS] Sending to request_tx... url={}", request.url);

                    if let Err(e) = request_tx.send(request) {
                        eprintln!("[ProxyDM WS] request_tx.send ERROR: {:?}", e);
                        break;
                    }

                    eprintln!("[ProxyDM WS] request_tx.send OK, now event_tx...");
                    let event = Event {
                        kind: EventKind::DownloadQueued,
                        download_id: 0,
                        data: None,
                    };
                    if let Err(e) = event_tx.send(event) {
                        eprintln!("[ProxyDM WS] event_tx.send ERROR: {:?}", e);
                        break;
                    }

                    eprintln!("[ProxyDM WS] event_tx OK, sending ack...");
                    if let Err(e) = ws.send(Message::Text(r#"{"status":"ok"}"#.into())) {
                        eprintln!("[ProxyDM WS] ack send ERROR: {:?}", e);
                        break;
                    }
                    eprintln!("[ProxyDM WS] All done, connection handling complete.");
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
