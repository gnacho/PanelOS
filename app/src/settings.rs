//! Panel settings changed on the panel (and later from Home Assistant):
//! `/mnt/persist/nspanel/settings.json`.

use serde::{Deserialize, Serialize};

use crate::config::PERSIST_DIR;

/// Screensaver timeouts offered in the settings, in seconds (0 = never).
pub const SCREENSAVER_CHOICES: &[(u32, &str)] =
    &[(0, "Never"), (30, "30 s"), (60, "1 min"), (120, "2 min"), (300, "5 min"), (600, "10 min"), (1800, "30 min")];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "dark", "light" or "auto" (sun below the horizon / dark room).
    pub theme: String,
    /// Manual backlight level, percent.
    pub brightness: u8,
    pub auto_brightness: bool,
    pub screensaver_seconds: u32,
    /// "clock" or "black".
    pub screensaver_style: String,
    pub proximity_wake: bool,
    /// Dashboard url_path; empty = the one in app.json.
    pub dashboard: String,
    /// Weather entity shown on the screensaver; empty = none.
    pub weather_entity: String,
    /// Active wake word model ids (first → Home Assistant's "Assistant",
    /// second → "Assistant 2"), as on ESPHome voice satellites.
    pub wake_words: Vec<String>,
    pub mic_muted: bool,
    /// Speaker volume, percent (codec digital volume: 100 % = 0 dB, 0 % = off).
    pub volume: u8,
    /// "dashboard" (native Home Assistant dashboard) or "assistant"
    /// (clock, weather, media, quick tiles, full-screen conversation).
    pub display_mode: String,
    /// Last assistant mode configuration received from the nspanel_pro
    /// integration (kept so a restart starts with the right entities).
    pub assistant: AssistantConfig,
}

/// What the assistant mode shows; set in Home Assistant (nspanel_pro
/// integration options). Without the integration: the weather entity above,
/// no media player, no tiles.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantConfig {
    /// True when the integration answered (otherwise the defaults apply).
    pub from_integration: bool,
    pub weather_entity: String,
    pub media_player_entity: String,
    pub tile_entities: Vec<String>,
    pub show_conversation: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            brightness: 70,
            auto_brightness: true,
            screensaver_seconds: 120,
            screensaver_style: "clock".into(),
            proximity_wake: true,
            dashboard: String::new(),
            weather_entity: String::new(),
            wake_words: vec!["okay_nabu".into(), "hey_jarvis".into()],
            mic_muted: false,
            volume: 80,
            display_mode: "dashboard".into(),
            assistant: AssistantConfig::default(),
        }
    }
}

fn path() -> String {
    std::env::var("NSPANEL_SETTINGS").unwrap_or_else(|_| format!("{PERSIST_DIR}/settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = path();
        let tmp = format!("{p}.tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }

    pub fn screensaver_index(&self) -> i32 {
        SCREENSAVER_CHOICES.iter().position(|(s, _)| *s == self.screensaver_seconds).unwrap_or(0) as i32
    }
}
