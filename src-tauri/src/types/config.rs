use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProxyProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "socks5")]
    Socks5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_dir: String,
    pub max_connections: u32,
    pub max_retries: u32,
    pub user_agent: String,
    pub launch_at_startup: bool,
    #[serde(default = "default_silent_startup")]
    pub silent_startup: bool,
    pub proxies: std::collections::HashMap<String, ProxyConfig>,
    pub global_rate_limit: u64,
    pub default_proxy: String,
    pub home_dir: String,
    pub language: String,
    #[serde(default)]
    pub danger_accept_invalid_certs: bool,
    #[serde(default = "default_global_shortcut")]
    pub global_shortcut: String,
}

impl Default for Settings {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        Self {
            download_dir: dirs::download_dir()
                .unwrap_or_else(|| home.clone())
                .to_string_lossy()
                .to_string(),
            max_connections: 0, // 0 = auto
            max_retries: 10,
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36 Edg/150.0.0.0".to_string(),
            launch_at_startup: false,
            silent_startup: default_silent_startup(),
            proxies: std::collections::HashMap::new(),
            global_rate_limit: 0,
            default_proxy: String::new(),
            home_dir: home.join(".ProxyDM").to_string_lossy().to_string(),
            language: String::from("en"),
            danger_accept_invalid_certs: true,
            global_shortcut: default_global_shortcut(),
        }
    }
}

fn default_silent_startup() -> bool {
    true
}

fn default_global_shortcut() -> String {
    "Ctrl+Super+J".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let s = Settings::default();
        assert_eq!(s.max_connections, 0); // default is auto
        assert!(s.max_retries > 0);
        assert!(!s.download_dir.is_empty());
    }

    #[test]
    fn test_settings_serde_roundtrip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s.max_connections, back.max_connections);
        assert_eq!(s.max_retries, back.max_retries);
    }
}
