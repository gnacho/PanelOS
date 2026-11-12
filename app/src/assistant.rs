//! Assistant mode (Echo Show / Nest Hub-like home screen): clock, weather
//! with forecasts, the media player, quick tiles, and a full-screen
//! conversation. What it shows is set in Home Assistant (nspanel_pro
//! integration); without the integration it falls back to the panel's
//! weather entity and shows no media player or tiles.

use std::collections::HashMap;

use serde_json::Value;
use slint::{ModelRc, SharedString, VecModel};

use crate::dashboard::{self, attr_f64, attr_str, CardRef, RowSpec, ROW_CLIMATE, ROW_COVER, ROW_MEDIA, ROW_PAIR};
use crate::ha::EntityState;
use crate::icons;
use crate::settings::{AssistantConfig, Settings};
use crate::{AssistantData, ForecastItem};

/// The configuration in effect: the integration's, or the fallback.
pub fn effective(settings: &Settings) -> AssistantConfig {
    if settings.assistant.from_integration {
        let mut c = settings.assistant.clone();
        if c.weather_entity.is_empty() {
            c.weather_entity = settings.weather_entity.clone();
        }
        c
    } else {
        AssistantConfig {
            from_integration: false,
            weather_entity: settings.weather_entity.clone(),
            media_player_entity: String::new(),
            tile_entities: Vec::new(),
            show_conversation: true,
        }
    }
}

/// Reads the integration's config message.
pub fn from_integration(v: &Value) -> AssistantConfig {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    AssistantConfig {
        from_integration: true,
        weather_entity: s("weather_entity"),
        media_player_entity: s("media_player_entity"),
        tile_entities: v
            .get("tile_entities")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        show_conversation: v.get("show_conversation").and_then(Value::as_bool).unwrap_or(true),
    }
}

/// Entities the panel follows besides the dashboard's.
pub fn extra_entities(settings: &Settings) -> Vec<String> {
    let c = effective(settings);
    let mut out = vec!["sun.sun".to_string(), settings.weather_entity.clone(), c.weather_entity, c.media_player_entity];
    out.extend(c.tile_entities);
    let mut seen = std::collections::HashSet::new();
    out.retain(|e| !e.is_empty() && seen.insert(e.clone()));
    out
}

/// Quick tiles as dashboard rows (covers, thermostats and players get
/// their own rows, the rest go in pairs).
pub fn tile_rows(c: &AssistantConfig) -> Vec<RowSpec> {
    let mut rows = Vec::new();
    let mut pending: Option<CardRef> = None;
    for entity in &c.tile_entities {
        let card = CardRef { entity: entity.clone(), name: None };
        let kind = match entity.split('.').next().unwrap_or("") {
            "cover" => ROW_COVER,
            "climate" => ROW_CLIMATE,
            "media_player" => ROW_MEDIA,
            _ => ROW_PAIR,
        };
        if kind == ROW_PAIR {
            match pending.take() {
                Some(a) => rows.push(RowSpec { kind, title: String::new(), a: Some(a), b: Some(card) }),
                None => pending = Some(card),
            }
        } else {
            rows.push(RowSpec { kind, title: String::new(), a: Some(card), b: None });
        }
    }
    if let Some(a) = pending {
        rows.push(RowSpec { kind: ROW_PAIR, title: String::new(), a: Some(a), b: None });
    }
    rows
}

/// Seconds since the epoch from an ISO 8601 time ("2026-11-06T10:00:00+00:00",
/// fractions and "Z" allowed).
fn parse_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> { s.get(from..from + len)?.parse().ok() };
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (h, mi) = (num(11, 2)?, num(14, 2)?);
    let sec = num(17, 2).unwrap_or(0);
    let mut i = 19;
    while i < b.len() && (b[i] == b'.' || b[i].is_ascii_digit()) {
        i += 1;
    }
    let offset = match b.get(i) {
        Some(b'+') | Some(b'-') => {
            let sign = if b[i] == b'-' { -1 } else { 1 };
            sign * (num(i + 1, 2)? * 3600 + num(i + 4, 2).unwrap_or(0) * 60)
        }
        _ => 0,
    };
    // Days from civil (Howard Hinnant's algorithm).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + sec - offset)
}

fn local_format(epoch: i64, pattern: &str) -> String {
    // SAFETY: localtime_r/strftime on local buffers.
    unsafe {
        let t = epoch as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let mut buf = [0u8; 32];
        let pattern = std::ffi::CString::new(pattern).unwrap();
        let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), pattern.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

fn now() -> i64 {
    // SAFETY: time(NULL).
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

fn degrees(v: Option<f64>) -> String {
    v.map(|t| format!("{t:.0}°")).unwrap_or_default()
}

fn glyph(name: &str) -> SharedString {
    icons::glyph(name).map(String::from).unwrap_or_default().into()
}

/// Forecast cells: the next hours (from the current hour) or the next days.
pub fn forecast_items(items: &[Value], hourly: bool, count: usize) -> Vec<ForecastItem> {
    let now = now();
    let today = local_format(now, "%Y-%m-%d");
    items
        .iter()
        .filter_map(|it| {
            let t = parse_iso(it.get("datetime")?.as_str()?)?;
            if hourly && t + 3600 <= now {
                return None;
            }
            let label = if hourly {
                local_format(t, "%H:%M")
            } else if local_format(t, "%Y-%m-%d") == today {
                "Today".to_string()
            } else {
                local_format(t, "%a")
            };
            if !hourly && t + 86400 <= now {
                return None;
            }
            let condition = it.get("condition").and_then(Value::as_str).unwrap_or("");
            let precip = it.get("precipitation_probability").and_then(Value::as_f64).filter(|p| *p >= 10.0);
            Some(ForecastItem {
                label: label.into(),
                icon: glyph(dashboard::weather_icon(condition)),
                temp: degrees(it.get("temperature").and_then(Value::as_f64)).into(),
                low: degrees(it.get("templow").and_then(Value::as_f64)).into(),
                precip: precip.map(|p| format!("{p:.0} %")).unwrap_or_default().into(),
            })
        })
        .take(count)
        .collect()
}

fn condition_text(state: &str) -> String {
    match state {
        "partlycloudy" => "Partly cloudy".into(),
        "clear-night" => "Clear".into(),
        "lightning-rainy" => "Thunderstorm".into(),
        "snowy-rainy" => "Sleet".into(),
        "windy-variant" => "Windy".into(),
        other => dashboard::capitalise(other),
    }
}

/// Home screen contents (everything except the tile rows and the artwork).
#[allow(clippy::too_many_arguments)]
pub fn data(
    c: &AssistantConfig,
    states: &HashMap<String, EntityState>,
    hourly: &[Value],
    daily: &[Value],
    wake_word: &str,
    mic_muted: bool,
    connected: bool,
) -> AssistantData {
    let mut d = AssistantData::default();
    if let Some(w) = states.get(&c.weather_entity).filter(|w| !matches!(w.state.as_str(), "unavailable" | "unknown")) {
        d.has_weather = true;
        d.weather_icon = glyph(dashboard::weather_icon(&w.state));
        d.temperature = degrees(attr_f64(w, "temperature")).into();
        d.condition = condition_text(&w.state).into();
        let mut details = Vec::new();
        let today = forecast_items(daily, false, 1);
        if let Some(t) = today.first() {
            if !t.temp.is_empty() && !t.low.is_empty() {
                details.push(format!("↑{} ↓{}", t.temp, t.low));
            }
        }
        if let Some(h) = attr_f64(w, "humidity") {
            details.push(format!("{h:.0} %"));
        }
        if let Some(ws) = attr_f64(w, "wind_speed") {
            details.push(format!("{ws:.0} {}", attr_str(w, "wind_speed_unit").unwrap_or("km/h")));
        }
        d.details = details.join("  ·  ").into();
        d.hourly = ModelRc::new(VecModel::from(forecast_items(hourly, true, 5)));
        d.daily = ModelRc::new(VecModel::from(forecast_items(daily, false, 7)));
    }
    // Without its state (Home Assistant not reached yet) the card would show
    // the raw entity id: leave it out until the first state arrives.
    if !c.media_player_entity.is_empty() && states.contains_key(&c.media_player_entity) {
        let tile = dashboard::tile(&CardRef { entity: c.media_player_entity.clone(), name: None }, states);
        let st = states.get(&c.media_player_entity).map(|s| s.state.as_str()).unwrap_or("");
        // The card is always there when a player is chosen; idle shows its name and state.
        d.has_media = true;
        let active = matches!(st, "playing" | "paused" | "buffering");
        let mut tile = tile;
        if let Some(p) = states.get(&c.media_player_entity) {
            d.media_source = attr_str(p, "source").or_else(|| attr_str(p, "app_name")).unwrap_or("").into();
            d.media_album = attr_str(p, "media_album_name").unwrap_or("").into();
        }
        if !active {
            tile.title = SharedString::new();
            tile.artist = tile.state.clone();
            d.media_album = SharedString::new();
        }
        d.media = tile;
    }
    d.hint = if !connected {
        "Not connected to Home Assistant".into()
    } else if mic_muted {
        "Microphone off".into()
    } else if wake_word.is_empty() {
        SharedString::new()
    } else {
        format!("Say “{wake_word}”").into()
    };
    d.setup_hint = if c.from_integration {
        SharedString::new()
    } else {
        "Add the NSPanel Pro integration in Home Assistant to choose the media player and quick tiles.".into()
    };
    d
}

/// Media artwork URL of the configured player (Home Assistant proxy path).
pub fn artwork_url(c: &AssistantConfig, states: &HashMap<String, EntityState>) -> Option<String> {
    let p = states.get(&c.media_player_entity)?;
    if !matches!(p.state.as_str(), "playing" | "paused" | "buffering") {
        return None;
    }
    attr_str(p, "entity_picture").filter(|u| !u.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::parse_iso;

    #[test]
    fn iso_times() {
        assert_eq!(parse_iso("1970-01-01T00:00:00+00:00"), Some(0));
        assert_eq!(parse_iso("2026-11-06T10:00:00Z"), Some(1_793_959_200));
        assert_eq!(parse_iso("2026-11-06T11:00:00.123+01:00"), Some(1_793_959_200));
    }
}
