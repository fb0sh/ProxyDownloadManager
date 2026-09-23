use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProxyProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "https")]
    Https,
    #[serde(rename = "socks5")]
    Socks5,
}

impl Default for ProxyProtocol {
    fn default() -> Self {
        Self::Http
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    #[serde(default)]
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileConflictPolicy {
    Ask,
    #[default]
    Rename,
    Overwrite,
    Skip,
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
    #[serde(default)]
    pub file_conflict: FileConflictPolicy,
    /// Comma-separated proxy names used as a failover group. Empty = off.
    #[serde(default)]
    pub proxy_group: Vec<String>,
    /// When a proxy group is set, try a direct connection after the group fails.
    #[serde(default)]
    pub proxy_group_fallback_direct: bool,
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
            file_conflict: FileConflictPolicy::Rename,
            proxy_group: Vec::new(),
            proxy_group_fallback_direct: false,
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
        assert_eq!(s.file_conflict, FileConflictPolicy::Rename);
    }

    #[test]
    fn test_settings_serde_roundtrip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s.max_connections, back.max_connections);
        assert_eq!(s.max_retries, back.max_retries);
        assert_eq!(back.file_conflict, FileConflictPolicy::Rename);
    }

    #[test]
    fn old_proxy_json_without_auth_parses() {
        let json = r#"{"protocol":"socks5","host":"127.0.0.1","port":1080}"#;
        let p: ProxyConfig = serde_json::from_str(json).unwrap();
        assert_eq!(p.host, "127.0.0.1");
        assert!(p.username.is_empty());
        assert!(p.password.is_empty());
    }
}
