use crate::types::{PdmError, PdmResult};
use reqwest::Proxy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Idle sockets are recycled after this long so a host that is no longer
/// active does not hold a socket forever. This is an *idle* pool policy, not a
/// limit on connections a running download may open.
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// Idle sockets kept per host. This does not cap active connections: reqwest
/// opens as many as the workers ask for and only the *idle* surplus is dropped.
const POOL_MAX_IDLE_PER_HOST: usize = 128;
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// HTTP/2 flow-control windows sized for high-BDP / proxied links (RFC 7540's
/// 64 KiB default caps a 100ms link at ~640 KB/s across all streams combined).
const H2_INITIAL_STREAM_WINDOW: u32 = 4 * 1024 * 1024;
const H2_INITIAL_CONN_WINDOW: u32 = 32 * 1024 * 1024;
/// Progressive subflow cap: workers 0..3 share Lane 0 (browser-identical single
/// H2 connection, reusing the warm probe socket); higher worker indices open at
/// most 3 extra H2 lanes so per-connection shaping is bypassed without triggering
/// CDN/WAF connection-flood blocks.
const MAX_CLIENT_LANES: usize = 4;
const WORKERS_PER_LANE: usize = 4;

/// One `reqwest::Client` per network path (Direct, or one proxy URL), shared by
/// probe, every download worker and every retry of that path. Cloning a client
/// is cheap and shares the same hyper connection pool.
pub struct NetworkPool {
    clients: Mutex<HashMap<String, reqwest::Client>>,
    danger_accept_invalid_certs: bool,
    client_creations: AtomicUsize,
}

impl NetworkPool {
    pub fn new(danger_accept_invalid_certs: bool) -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            danger_accept_invalid_certs,
            client_creations: AtomicUsize::new(0),
        }
    }

    pub fn get_client(&self, proxy_url: Option<&str>) -> PdmResult<reqwest::Client> {
        self.get_or_build(proxy_url, 0)
    }

    /// Return a striped client lane for concurrent Range worker `worker_idx`.
    /// Workers `0..4` always use Lane 0 (`get_client`), preserving warm probe
    /// socket reuse and single-connection behavior; extra lanes (`1..4`) open
    /// progressively only when concurrency exceeds 4.
    pub fn get_lane_client(
        &self,
        proxy_url: Option<&str>,
        worker_idx: u32,
    ) -> PdmResult<reqwest::Client> {
        let lane = (worker_idx as usize / WORKERS_PER_LANE).min(MAX_CLIENT_LANES - 1);
        self.get_or_build(proxy_url, lane)
    }

    fn get_or_build(&self, proxy_url: Option<&str>, lane: usize) -> PdmResult<reqwest::Client> {
        let base = proxy_url.unwrap_or("direct");
        let key = if lane == 0 {
            base.to_string()
        } else {
            format!("{base}#lane{lane}")
        };
        let mut map = self
            .clients
            .lock()
            .map_err(|e| PdmError::Other(e.to_string()))?;
        if let Some(client) = map.get(&key) {
            return Ok(client.clone());
        }
        log::info!(
            "[ProxyDM] pool creating new client for proxy={} lane={}",
            crate::headers::redact_log(base),
            lane
        );
        let mut builder = reqwest::Client::builder()
            .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
            .pool_idle_timeout(Some(POOL_IDLE_TIMEOUT))
            .tcp_keepalive(Some(TCP_KEEPALIVE))
            .tcp_nodelay(true)
            .http2_adaptive_window(true)
            .http2_initial_stream_window_size(Some(H2_INITIAL_STREAM_WINDOW))
            .http2_initial_connection_window_size(Some(H2_INITIAL_CONN_WINDOW))
            .connect_timeout(CONNECT_TIMEOUT)
            .https_only(false)
            .danger_accept_invalid_certs(self.danger_accept_invalid_certs);

        if let Some(proxy_str) = proxy_url {
            if let Ok(proxy) = Proxy::all(proxy_str) {
                log::info!(
                    "[ProxyDM] pool applying proxy: {}",
                    crate::headers::redact_log(proxy_str)
                );
                builder = builder.proxy(proxy);
            }
        }

        let client = builder
            .build()
            .map_err(|e| PdmError::ClientBuild(e.to_string()))?;
        map.insert(key, client.clone());
        self.client_creations.fetch_add(1, Ordering::Relaxed);
        Ok(client)
    }

    /// Number of `reqwest::Client` instances this pool has ever built. Tests use
    /// it to prove probe, download and retry share one client per network path.
    pub fn client_creation_count(&self) -> usize {
        self.client_creations.load(Ordering::Relaxed)
    }

    /// Number of cached clients (one per distinct network path).
    pub fn cached_client_count(&self) -> usize {
        self.clients.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Clear cached clients so next get_client() rebuilds with current settings
    /// (only settings that change how a Client is constructed, e.g. TLS).
    pub fn clear(&self) {
        let mut map = self.clients.lock().unwrap();
        let count = map.len();
        map.clear();
        log::info!("[ProxyDM] pool cleared {} cached client(s)", count);
    }
}
