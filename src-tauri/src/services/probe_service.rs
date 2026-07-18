use crate::types::*;
use crate::network::pool::NetworkPool;
use std::sync::Arc;

pub struct ProbeOutcome {
    pub file_name: String,
    pub file_size: u64,
    pub supports_range: bool,
}

pub async fn probe_with_fallback(
    url: &str,
    headers: &std::collections::HashMap<String, String>,
    proxy_url: Option<&str>,
    pool: &Arc<NetworkPool>,
    user_agents: &[String],
    filename_override: &str,
) -> ProbeOutcome {
    let result = crate::probe::probe(url, headers, proxy_url, pool, user_agents).await;

    match result {
        Ok(r) => {
            let name = if filename_override.is_empty() { r.file_name } else { filename_override.to_string() };
            ProbeOutcome {
                file_name: name,
                file_size: r.file_size,
                supports_range: r.supports_range,
            }
        }
        Err(e) => {
            let name = if filename_override.is_empty() {
                crate::filename::from_url(url).unwrap_or_else(|| "download".to_string())
            } else {
                filename_override.to_string()
            };
            ProbeOutcome {
                file_name: name,
                file_size: 0,
                supports_range: false,
            }
        }
    }
}

pub fn build_user_agents(user_agent: &str) -> Vec<String> {
    vec![
        user_agent.to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0".to_string(),
    ]
}
