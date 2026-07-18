use crate::types::*;
use crate::config;
use std::sync::Mutex;

pub struct SettingsService {
    settings: Mutex<Settings>,
}

/// Result of saving settings — encodes what Tauri-specific side effects the caller should trigger.
pub struct SettingsSaveResult {
    pub tls_changed: bool,
    pub shortcut_changed: bool,
    pub old_shortcut: String,
    pub new_shortcut: String,
    pub launch_at_startup: bool,
    pub silent_startup: bool,
}

impl SettingsService {
    pub fn new() -> Self {
        let settings = config::load();
        Self {
            settings: Mutex::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn reload(&self) {
        if let Ok(mut s) = self.settings.lock() {
            *s = config::load();
        }
    }

    /// Save settings to disk and reload the cache.
    /// Returns change flags so the caller can handle Tauri-specific side effects (autostart, shortcuts).
    pub fn save(&self, new_settings: &Settings) -> PdmResult<SettingsSaveResult> {
        let old = self.get();
        let tls_changed = old.danger_accept_invalid_certs != new_settings.danger_accept_invalid_certs;
        let shortcut_changed = old.global_shortcut != new_settings.global_shortcut;

        config::save(new_settings)?;
        self.reload();

        Ok(SettingsSaveResult {
            tls_changed,
            shortcut_changed,
            old_shortcut: old.global_shortcut,
            new_shortcut: new_settings.global_shortcut.clone(),
            launch_at_startup: new_settings.launch_at_startup,
            silent_startup: new_settings.silent_startup,
        })
    }

    /// Resolve a proxy name to a URL using cached settings.
    pub fn resolve_proxy_url(&self, proxy_name: &str) -> Option<String> {
        let settings = self.get();
        resolve_proxy_url_from(proxy_name, &settings)
    }

    /// Build the user-agent fallback list: configured UA + browser defaults.
    pub fn build_user_agents(&self) -> Vec<String> {
        let settings = self.get();
        crate::services::probe_service::build_user_agents(&settings.user_agent)
    }
}

/// Resolve a proxy name to a URL using the given settings.
pub(crate) fn resolve_proxy_url_from(proxy_name: &str, settings: &Settings) -> Option<String> {
    if proxy_name.is_empty() {
        return None;
    }
    let proxy = settings.proxies.get(proxy_name)?;
    let protocol = match proxy.protocol {
        ProxyProtocol::Http => "http",
        ProxyProtocol::Socks5 => "socks5",
    };
    Some(format!("{}://{}:{}", protocol, proxy.host, proxy.port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_proxy_url_empty() {
        let settings = Settings::default();
        assert!(resolve_proxy_url_from("", &settings).is_none());
    }

    #[test]
    fn test_resolve_proxy_url_missing() {
        let settings = Settings::default();
        assert!(resolve_proxy_url_from("nonexistent", &settings).is_none());
    }

    #[test]
    fn test_settings_service_roundtrip() {
        let svc = SettingsService::new();
        let s = svc.get();
        assert!(!s.download_dir.is_empty());
    }
}
