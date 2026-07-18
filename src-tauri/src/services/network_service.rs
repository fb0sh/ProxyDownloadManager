use crate::types::*;
use crate::network::pool::NetworkPool;
use std::sync::Arc;

pub struct NetworkService {
    pool: Arc<NetworkPool>,
}

impl NetworkService {
    pub fn new(pool: Arc<NetworkPool>) -> Self {
        Self { pool }
    }

    /// Check for application updates from GitHub.
    pub async fn check_update(&self, proxy_url: Option<&str>) -> PdmResult<serde_json::Value> {
        let client = self.pool.get_client(proxy_url)?;

        let resp = client
            .get("https://api.github.com/repos/fb0sh/ProxyDownloadManager/releases/latest")
            .header("User-Agent", concat!("ProxyDM/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github.v3+json")
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Failed to check update: {}", e))?;

        if !resp.status().is_success() {
            return Err(PdmError::Other(format!("GitHub API responded with status {}", resp.status())));
        }

        let body = resp.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        Ok(serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse release info: {}", e))?)
    }

    /// Test proxy connectivity.
    pub async fn test_proxy(&self, proxy_url: Option<&str>) -> PdmResult<serde_json::Value> {
        let client = self.pool.get_client(proxy_url)?;

        let start = std::time::Instant::now();
        match client
            .get("https://www.google.com")
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let ok = resp.status().is_success();
                let status = resp.status().as_u16();
                Ok(serde_json::json!({
                    "ok": ok,
                    "latency_ms": latency_ms,
                    "status": status,
                }))
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                Ok(serde_json::json!({
                    "ok": false,
                    "latency_ms": latency_ms,
                    "error": format!("{}", e),
                }))
            }
        }
    }
}
