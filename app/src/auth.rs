//! Home Assistant login: OAuth2 authorization-code flow with the panel as an
//! IndieAuth client. The refresh token is stored on the panel (root only);
//! short-lived access tokens are fetched from it when needed.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Serialize, Deserialize)]
struct Stored {
    client_id: String,
    refresh_token: String,
}

#[derive(Default)]
struct Inner {
    stored: Option<Stored>,
    access_token: String,
    expires_at: u64,
}

pub struct Auth {
    ha_url: String,
    path: PathBuf,
    inner: Mutex<Inner>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub enum TokenError {
    /// No login, or Home Assistant revoked it: pairing is needed.
    NotPaired,
    /// Temporary problem (network, Home Assistant restarting).
    Retry(String),
}

impl Auth {
    pub fn load(ha_url: &str, path: PathBuf) -> Self {
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Stored>(&t).ok())
            .filter(|s| !s.refresh_token.is_empty());
        Self { ha_url: ha_url.to_string(), path, inner: Mutex::new(Inner { stored, ..Default::default() }) }
    }

    pub fn is_paired(&self) -> bool {
        self.inner.lock().unwrap().stored.is_some()
    }

    fn save(&self, stored: &Stored) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(serde_json::to_string(stored).unwrap().as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, &self.path)
    }

    fn post_token(&self, form: &[(&str, &str)]) -> Result<serde_json::Value, (u16, String)> {
        let url = format!("{}/auth/token", self.ha_url);
        match ureq::post(&url).timeout(std::time::Duration::from_secs(15)).send_form(form) {
            Ok(resp) => serde_json::from_reader(resp.into_reader()).map_err(|e| (0, e.to_string())),
            Err(ureq::Error::Status(code, resp)) => Err((code, resp.into_string().unwrap_or_default())),
            Err(e) => Err((0, e.to_string())),
        }
    }

    /// Exchanges the code from the login redirect for tokens and stores them.
    pub fn exchange_code(&self, client_id: &str, code: &str) -> Result<(), String> {
        let v = self
            .post_token(&[("grant_type", "authorization_code"), ("code", code), ("client_id", client_id)])
            .map_err(|(c, e)| format!("token exchange failed ({c}): {e}"))?;
        let refresh = v["refresh_token"].as_str().ok_or("no refresh_token in answer")?.to_string();
        let access = v["access_token"].as_str().ok_or("no access_token in answer")?.to_string();
        let expires = v["expires_in"].as_u64().unwrap_or(1800);
        let stored = Stored { client_id: client_id.to_string(), refresh_token: refresh };
        self.save(&stored).map_err(|e| format!("cannot save login: {e}"))?;
        let mut inner = self.inner.lock().unwrap();
        inner.stored = Some(stored);
        inner.access_token = access;
        inner.expires_at = now() + expires;
        Ok(())
    }

    /// A valid access token, refreshed when it is about to expire.
    pub fn access_token(&self) -> Result<String, TokenError> {
        let mut inner = self.inner.lock().unwrap();
        let Some(stored) = inner.stored.clone() else { return Err(TokenError::NotPaired) };
        if !inner.access_token.is_empty() && inner.expires_at > now() + 60 {
            return Ok(inner.access_token.clone());
        }
        match self.post_token(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &stored.refresh_token),
            ("client_id", &stored.client_id),
        ]) {
            Ok(v) => {
                let access = v["access_token"].as_str().unwrap_or_default().to_string();
                if access.is_empty() {
                    return Err(TokenError::Retry("no access_token in answer".into()));
                }
                inner.access_token = access.clone();
                inner.expires_at = now() + v["expires_in"].as_u64().unwrap_or(1800);
                Ok(access)
            }
            Err((400, _)) | Err((401, _)) | Err((403, _)) => {
                // The refresh token was revoked in Home Assistant: log in again.
                inner.stored = None;
                inner.access_token.clear();
                let _ = std::fs::remove_file(&self.path);
                Err(TokenError::NotPaired)
            }
            Err((code, e)) => Err(TokenError::Retry(format!("token refresh failed ({code}): {e}"))),
        }
    }

    /// Logs out: forgets the stored login (Home Assistant still lists the
    /// token in the user profile until it is removed there or expires).
    pub fn forget(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.stored = None;
        inner.access_token.clear();
        let _ = std::fs::remove_file(&self.path);
    }

    /// Forces a refresh on the next access_token() call.
    pub fn invalidate_access(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.access_token.clear();
        inner.expires_at = 0;
    }
}
