use std::collections::HashMap;
use std::sync::Mutex;
use reqwest::Proxy;
use std::time::Duration;
use crate::types::{PdmError, PdmResult};

pub struct NetworkPool {
    clients: Mutex<HashMap<String, reqwest::Client>>,
    danger_accept_invalid_certs: bool,
}

impl NetworkPool {
    pub fn new(danger_accept_invalid_certs: bool) -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            danger_accept_invalid_certs,
        }
    }

    pub fn get_client(&self, proxy_url: Option<&str>) -> PdmResult<reqwest::Client> {
        let key = proxy_url.unwrap_or("direct").to_string();
        let mut map = self.clients.lock().map_err(|e| PdmError::Other(e.to_string()))?;
        if let Some(client) = map.get(&key) {
            return Ok(client.clone());
        }
        log::info!("[ProxyDM] pool creating new client for proxy={}", key);
        let mut builder = reqwest::Client::builder()
            .pool_max_idle_per_host(128)
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .connect_timeout(Duration::from_secs(30))
            .https_only(false)
            .danger_accept_invalid_certs(self.danger_accept_invalid_certs);

        if let Some(proxy_str) = proxy_url {
            if let Ok(proxy) = Proxy::all(proxy_str) {
                log::info!("[ProxyDM] pool applying proxy: {}", proxy_str);
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().map_err(|e| PdmError::ClientBuild(e.to_string()))?;
        map.insert(key, client.clone());
        Ok(client)
    }

    /// Clear cached clients so next get_client() rebuilds with current settings
    pub fn clear(&self) {
        let mut map = self.clients.lock().unwrap();
        let count = map.len();
        map.clear();
        log::info!("[ProxyDM] pool cleared {} cached client(s)", count);
    }
}
