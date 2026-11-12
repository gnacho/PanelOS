//! App settings: `/mnt/persist/nspanel/app.json` (or the file named by
//! NSPANEL_CONFIG). It holds no credentials: the login token is kept in
//! `ha_auth.json`.

use serde::{Deserialize, Serialize};

pub const PERSIST_DIR: &str = "/mnt/persist/nspanel";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Home Assistant base URL, e.g. "http://192.168.1.10:8123" (port 80 when
    /// none is given; `.local` names do not resolve on the panel).
    pub ha_url: String,
    /// Dashboard url_path shown on the panel ("" = the default dashboard).
    #[serde(default = "default_dashboard")]
    pub dashboard: String,
    /// Port of the panel's small web server (login callback).
    #[serde(default = "default_port")]
    pub http_port: u16,
    /// Further addresses allowed to use the ESPHome API besides Home
    /// Assistant's own (e.g. when Home Assistant reaches the panel from
    /// another address than its URL's).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_allow: Vec<String>,
}

fn default_dashboard() -> String {
    String::new()
}

fn default_port() -> u16 {
    8080
}

impl AppConfig {
    pub fn load() -> Result<Self, String> {
        let path = std::env::var("NSPANEL_CONFIG").unwrap_or_else(|_| format!("{PERSIST_DIR}/app.json"));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {path}: {e}"))?;
        let mut cfg: AppConfig = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        cfg.ha_url = cfg.ha_url.trim_end_matches('/').to_string();
        Ok(cfg)
    }

    /// "host:port" of the Home Assistant URL.
    pub fn ha_host_port(&self) -> String {
        let rest = self.ha_url.split("://").nth(1).unwrap_or(&self.ha_url);
        let host = rest.split('/').next().unwrap_or(rest);
        if host.contains(':') {
            host.to_string()
        } else if self.ha_url.starts_with("https") {
            format!("{host}:443")
        } else {
            format!("{host}:80")
        }
    }

    pub fn ws_url(&self) -> String {
        let base = if let Some(rest) = self.ha_url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.ha_url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            self.ha_url.clone()
        };
        format!("{base}/api/websocket")
    }
}
