use std::collections::HashMap;
use std::sync::Mutex;
use reqwest::Proxy;
use std::time::Duration;

pub struct NetworkPool {
    clients: Mutex<HashMap<String, reqwest::Client>>,
}

impl NetworkPool {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_client(&self, proxy_url: Option<&str>) -> reqwest::Client {
        let key = proxy_url.unwrap_or("direct").to_string();
        let mut map = self.clients.lock().unwrap();
        if let Some(client) = map.get(&key) {
            return client.clone();
        }
        let mut builder = reqwest::Client::builder()
            .pool_max_idle_per_host(128)
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(30))
            .https_only(false)
            .danger_accept_invalid_certs(false);

        if let Some(proxy_str) = proxy_url {
            if let Ok(proxy) = Proxy::all(proxy_str) {
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().expect("Failed to build reqwest Client");
        map.insert(key, client.clone());
        client
    }
}
