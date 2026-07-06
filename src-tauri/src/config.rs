use crate::types::Settings;
use std::path::PathBuf;

fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".ProxyDM/ProxyDM.toml")
}

pub fn load() -> Settings {
    let path = config_path();
    if !path.exists() {
        let settings = Settings::default();
        save(&settings).ok();
        return settings;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[ProxyDM] Failed to read config at {:?}: {}", path, e);
            return Settings::default();
        }
    };
    match toml::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[ProxyDM] Failed to parse config at {:?}: {}. Using defaults.", path, e);
            Settings::default()
        }
    }
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = toml::to_string(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_toml_roundtrip() {
        let mut s = Settings::default();
        s.max_connections = 16;
        s.max_retries = 5;
        s.user_agent = "TestAgent/1.0".to_string();
        s.proxies.insert("p1".to_string(), crate::types::ProxyConfig {
            protocol: crate::types::ProxyProtocol::Socks5,
            host: "127.0.0.1".to_string(),
            port: 1080,
        });

        let toml_str = toml::to_string(&s).unwrap();
        let back: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.max_connections, 16);
        assert_eq!(back.max_retries, 5);
        assert_eq!(back.user_agent, "TestAgent/1.0");
        assert_eq!(back.proxies.len(), 1);
        assert_eq!(back.proxies["p1"].host, "127.0.0.1");
    }

    #[test]
    fn test_settings_default_serializable() {
        let s = Settings::default();
        let toml_str = toml::to_string(&s).unwrap();
        assert!(toml_str.contains("max_connections"));
        assert!(toml_str.contains("download_dir"));
    }
}
