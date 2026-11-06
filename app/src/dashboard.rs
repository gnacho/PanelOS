//! Turns a Home Assistant dashboard config into native rows (one list per
//! view) and entity states into tile and detail contents. Only the content
//! is taken from Home Assistant; the look is the app's own.

use std::collections::HashMap;

use serde_json::Value;
use slint::{Color, ModelRc, SharedString, VecModel};

use crate::ha::EntityState;
use crate::icons;
use crate::{MoreInfoData, RowData, TileData};

/// Row kinds (same numbers as RowData.kind in rows.slint).
pub const ROW_HEADING: i32 = 0;
pub const ROW_PAIR: i32 = 1;
pub const ROW_COVER: i32 = 2;
pub const ROW_CLIMATE: i32 = 3;
pub const ROW_MEDIA: i32 = 4;
pub const ROW_CAMERA: i32 = 5;

/// Row heights and spacing in rows.slint / main.slint (to know where the camera is).
pub fn row_height(kind: i32) -> f32 {
    match kind {
        ROW_HEADING => 44.0,
        ROW_PAIR | ROW_COVER => 84.0,
        ROW_CLIMATE => 124.0,
        ROW_MEDIA => 132.0,
        _ => 252.0,
    }
}
pub const ROW_SPACING: f32 = 8.0;
pub const LIST_PADDING_TOP: f32 = 4.0;

#[derive(Debug, Clone)]
pub struct CardRef {
    pub entity: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RowSpec {
    pub kind: i32,
    pub title: String,
    pub a: Option<CardRef>,
    pub b: Option<CardRef>,
}

enum Card {
    Heading(String),
    Tile(CardRef),
    Thermostat(CardRef),
    Media(CardRef),
    Camera(CardRef),
}

fn card_ref(v: &Value) -> Option<CardRef> {
    let entity = v.get("entity")?.as_str()?.to_string();
    let name = v.get("name").and_then(Value::as_str).map(str::to_string);
    Some(CardRef { entity, name })
}

fn collect(card: &Value, out: &mut Vec<Card>) {
    let kind = card.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "heading" => {
            if let Some(h) = card.get("heading").and_then(Value::as_str) {
                out.push(Card::Heading(h.to_string()));
            }
        }
        "tile" | "entity" | "button" | "light" | "sensor" | "gauge" | "weather-forecast" => {
            out.extend(card_ref(card).map(Card::Tile));
        }
        "thermostat" | "humidifier" => out.extend(card_ref(card).map(Card::Thermostat)),
        "media-control" => out.extend(card_ref(card).map(Card::Media)),
        "picture-entity" | "picture-glance" => {
            if let Some(r) = card_ref(card).or_else(|| {
                let cam = card.get("camera_image")?.as_str()?;
                Some(CardRef { entity: cam.to_string(), name: card.get("title").and_then(Value::as_str).map(str::to_string) })
            }) {
                out.push(Card::Camera(r));
            }
        }
        "entities" | "glance" => {
            if let Some(t) = card.get("title").and_then(Value::as_str) {
                out.push(Card::Heading(t.to_string()));
            }
            for item in card.get("entities").and_then(Value::as_array).into_iter().flatten() {
                let r = match item {
                    Value::String(s) => Some(CardRef { entity: s.clone(), name: None }),
                    other => card_ref(other),
                };
                out.extend(r.map(Card::Tile));
            }
        }
        "horizontal-stack" | "vertical-stack" | "grid" => {
            if let Some(t) = card.get("title").and_then(Value::as_str) {
                out.push(Card::Heading(t.to_string()));
            }
            for c in card.get("cards").and_then(Value::as_array).into_iter().flatten() {
                collect(c, out);
            }
        }
        _ => {}
    }
}

fn views(config: &Value) -> &[Value] {
    config.get("views").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// Titles of the dashboard's views (tabs on the panel).
pub fn view_titles(config: &Value) -> Vec<String> {
    views(config)
        .iter()
        .enumerate()
        .map(|(i, v)| v.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("Page {}", i + 1)))
        .collect()
}

/// Rows for one view of a dashboard config.
pub fn build_rows(config: &Value, view: usize) -> Vec<RowSpec> {
    let mut cards = Vec::new();
    if let Some(view) = views(config).get(view) {
        for c in view.get("cards").and_then(Value::as_array).into_iter().flatten() {
            collect(c, &mut cards);
        }
        for s in view.get("sections").and_then(Value::as_array).into_iter().flatten() {
            if let Some(t) = s.get("title").and_then(Value::as_str) {
                cards.push(Card::Heading(t.to_string()));
            }
            for c in s.get("cards").and_then(Value::as_array).into_iter().flatten() {
                collect(c, &mut cards);
            }
        }
    }

    let mut rows: Vec<RowSpec> = Vec::new();
    let mut pending: Option<CardRef> = None;
    let flush = |rows: &mut Vec<RowSpec>, pending: &mut Option<CardRef>| {
        if let Some(a) = pending.take() {
            rows.push(RowSpec { kind: ROW_PAIR, title: String::new(), a: Some(a), b: None });
        }
    };
    let full = |rows: &mut Vec<RowSpec>, kind: i32, r: CardRef| {
        rows.push(RowSpec { kind, title: String::new(), a: Some(r), b: None });
    };
    for card in cards {
        match card {
            Card::Heading(t) => {
                flush(&mut rows, &mut pending);
                rows.push(RowSpec { kind: ROW_HEADING, title: t, a: None, b: None });
            }
            Card::Tile(r) => match r.entity.split('.').next().unwrap_or("") {
                "cover" => {
                    flush(&mut rows, &mut pending);
                    full(&mut rows, ROW_COVER, r);
                }
                "climate" => {
                    flush(&mut rows, &mut pending);
                    full(&mut rows, ROW_CLIMATE, r);
                }
                "camera" => {
                    flush(&mut rows, &mut pending);
                    full(&mut rows, ROW_CAMERA, r);
                }
                _ => {
                    if let Some(a) = pending.take() {
                        rows.push(RowSpec { kind: ROW_PAIR, title: String::new(), a: Some(a), b: Some(r) });
                    } else {
                        pending = Some(r);
                    }
                }
            },
            Card::Thermostat(r) => {
                flush(&mut rows, &mut pending);
                full(&mut rows, ROW_CLIMATE, r);
            }
            Card::Media(r) => {
                flush(&mut rows, &mut pending);
                full(&mut rows, ROW_MEDIA, r);
            }
            Card::Camera(r) => {
                flush(&mut rows, &mut pending);
                full(&mut rows, ROW_CAMERA, r);
            }
        }
    }
    flush(&mut rows, &mut pending);
    rows
}

pub fn attr_f64(st: &EntityState, key: &str) -> Option<f64> {
    st.attrs.get(key).and_then(Value::as_f64)
}

pub fn attr_str<'a>(st: &'a EntityState, key: &str) -> Option<&'a str> {
    st.attrs.get(key).and_then(Value::as_str)
}

pub fn attr_list(st: &EntityState, key: &str) -> Vec<String> {
    st.attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

pub fn capitalise(s: &str) -> String {
    let s = s.replace(['_', '-'], " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn format_number(v: f64, unit: &str) -> String {
    let decimals = if unit == "%" || v.abs() >= 100.0 { 0 } else { 1 };
    let text = format!("{v:.decimals$}");
    if unit.is_empty() { text } else { format!("{text} {unit}") }
}

const AMBER: (u8, u8, u8) = (0xf5, 0xb0, 0x0b);
const BLUE: (u8, u8, u8) = (0x3b, 0x82, 0xf6);
const ORANGE: (u8, u8, u8) = (0xf9, 0x73, 0x16);
const CYAN: (u8, u8, u8) = (0x06, 0xb6, 0xd4);
const GREEN: (u8, u8, u8) = (0x22, 0xc5, 0x5e);
const VIOLET: (u8, u8, u8) = (0x8b, 0x5c, 0xf6);
const SKY: (u8, u8, u8) = (0x38, 0xbd, 0xf8);

fn rgb(c: (u8, u8, u8)) -> Color {
    Color::from_rgb_u8(c.0, c.1, c.2)
}

pub fn weather_icon(condition: &str) -> &'static str {
    match condition {
        "clear-night" => "weather-night",
        "cloudy" => "weather-cloudy",
        "fog" => "weather-fog",
        "hail" => "weather-hail",
        "lightning" => "weather-lightning",
        "lightning-rainy" => "weather-lightning-rainy",
        "partlycloudy" => "weather-partly-cloudy",
        "pouring" => "weather-pouring",
        "rainy" => "weather-rainy",
        "snowy" => "weather-snowy",
        "snowy-rainy" => "weather-snowy-rainy",
        "sunny" => "weather-sunny",
        "windy" => "weather-windy",
        "windy-variant" => "weather-windy-variant",
        _ => "weather-cloudy",
    }
}

pub fn weather_text(st: &EntityState) -> String {
    let condition = match st.state.as_str() {
        "partlycloudy" => "Partly cloudy".to_string(),
        "clear-night" => "Clear".to_string(),
        other => capitalise(other),
    };
    match attr_f64(st, "temperature") {
        Some(t) => format!("{} · {t:.0} {}", condition, attr_str(st, "temperature_unit").unwrap_or("°C")),
        None => condition,
    }
}

fn icon_glyph(domain: &str, st: &EntityState) -> SharedString {
    let device_class = attr_str(st, "device_class");
    let name = if domain == "weather" {
        weather_icon(&st.state).to_string()
    } else {
        attr_str(st, "icon")
            .map(str::to_string)
            .unwrap_or_else(|| icons::default_icon(domain, &st.state, device_class).to_string())
    };
    icons::glyph(&name)
        .or_else(|| icons::glyph(icons::default_icon(domain, &st.state, device_class)))
        .map(String::from)
        .unwrap_or_default()
        .into()
}

fn light_modes(st: &EntityState) -> Vec<String> {
    attr_list(st, "supported_color_modes")
}

pub fn light_has_brightness(st: &EntityState) -> bool {
    light_modes(st).iter().any(|m| m != "onoff" && m != "unknown")
}

fn light_has_color(st: &EntityState) -> bool {
    light_modes(st).iter().any(|m| matches!(m.as_str(), "hs" | "xy" | "rgb" | "rgbw" | "rgbww"))
}

/// Whether tapping the tile opens details (otherwise it toggles).
pub fn has_details(entity: &str, states: &HashMap<String, EntityState>) -> bool {
    let Some(st) = states.get(entity) else { return false };
    match entity.split('.').next().unwrap_or("") {
        "light" => light_has_brightness(st),
        "cover" | "climate" | "media_player" => true,
        _ => false,
    }
}

/// Tile contents for one entity.
pub fn tile(card: &CardRef, states: &HashMap<String, EntityState>) -> TileData {
    let domain = card.entity.split('.').next().unwrap_or("").to_string();
    let empty = EntityState { state: "unavailable".into(), ..Default::default() };
    let st = states.get(&card.entity).unwrap_or(&empty);
    let state = st.state.as_str();
    let available = !matches!(state, "unavailable" | "unknown");
    let name = card
        .name
        .clone()
        .or_else(|| attr_str(st, "friendly_name").map(str::to_string))
        .unwrap_or_else(|| card.entity.clone());

    let mut t = TileData {
        entity: card.entity.as_str().into(),
        name: name.into(),
        icon: icon_glyph(&domain, st),
        available,
        ..Default::default()
    };

    let unit = attr_str(st, "unit_of_measurement").unwrap_or("");
    match domain.as_str() {
        "light" => {
            t.kind = 1;
            t.on = state == "on";
            t.accent = rgb(AMBER);
            t.state = match (t.on, attr_f64(st, "brightness")) {
                (true, Some(b)) => format!("On · {:.0} %", b / 255.0 * 100.0),
                (true, None) => "On".into(),
                _ => capitalise(state),
            }
            .into();
        }
        "switch" | "input_boolean" | "fan" | "automation" => {
            t.kind = 1;
            t.on = state == "on";
            t.accent = rgb(BLUE);
            t.state = capitalise(state).into();
        }
        "cover" => {
            t.kind = 2;
            t.on = matches!(state, "open" | "opening" | "closing");
            t.accent = rgb(BLUE);
            t.state = match attr_f64(st, "current_position") {
                Some(p) if state == "open" && p < 100.0 => format!("Open · {p:.0} %"),
                _ => capitalise(state),
            }
            .into();
        }
        "climate" => {
            t.kind = 3;
            t.on = state != "off" && available;
            let action = attr_str(st, "hvac_action").unwrap_or("");
            t.accent = match state {
                "cool" => rgb(CYAN),
                "dry" | "fan_only" => rgb(BLUE),
                _ => rgb(ORANGE),
            };
            t.mode = capitalise(state).into();
            t.state = if action.is_empty() || state == "off" {
                capitalise(state)
            } else {
                format!("{} · {}", capitalise(state), capitalise(action))
            }
            .into();
            t.current = attr_f64(st, "current_temperature").map(|v| format!("{v:.1} °C")).unwrap_or_default().into();
            t.target = attr_f64(st, "temperature").map(|v| format!("{v:.1}°")).unwrap_or_else(|| "–".into()).into();
        }
        "media_player" => {
            t.kind = 4;
            t.on = !matches!(state, "off" | "unavailable" | "unknown" | "standby");
            t.playing = state == "playing";
            t.accent = rgb(GREEN);
            t.state = capitalise(state).into();
            t.title = attr_str(st, "media_title").unwrap_or("").into();
            t.artist = attr_str(st, "media_artist").or_else(|| attr_str(st, "app_name")).unwrap_or("").into();
            t.volume = attr_f64(st, "volume_level").map(|v| (v * 100.0).round() as i32).unwrap_or(0);
        }
        "camera" => {
            t.kind = 5;
            t.on = true;
            t.accent = rgb(VIOLET);
            t.state = capitalise(state).into();
        }
        "weather" => {
            t.kind = 0;
            t.on = available;
            t.accent = rgb(SKY);
            t.state = weather_text(st).into();
        }
        _ => {
            t.kind = 0;
            t.accent = rgb(match attr_str(st, "device_class") {
                Some("temperature") => ORANGE,
                Some("humidity") => CYAN,
                _ => BLUE,
            });
            t.on = available;
            t.state = match state.parse::<f64>() {
                Ok(v) => format_number(v, unit),
                Err(_) => capitalise(state),
            }
            .into();
        }
    }
    if !available {
        t.state = SharedString::from("Unavailable");
    }
    t
}

pub fn row_data(spec: &RowSpec, states: &HashMap<String, EntityState>) -> RowData {
    let mut row = RowData { kind: spec.kind, title: spec.title.as_str().into(), ..Default::default() };
    if let Some(a) = &spec.a {
        row.a = tile(a, states);
        if spec.kind == ROW_PAIR && row.a.kind == 4 {
            row.a.kind = 1; // TV/speaker tile: icon tap toggles power
        }
    }
    if let Some(b) = &spec.b {
        row.b = tile(b, states);
        if row.b.kind == 4 {
            row.b.kind = 1;
        }
        row.has_b = true;
    }
    row
}

fn labels(values: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(values.iter().map(|v| SharedString::from(capitalise(v))).collect::<Vec<_>>()))
}

fn index_of(values: &[String], current: Option<&str>) -> i32 {
    current.and_then(|c| values.iter().position(|v| v == c)).map(|i| i as i32).unwrap_or(-1)
}

/// Contents of the details layer for one entity.
pub fn more_info(entity: &str, name: Option<&str>, states: &HashMap<String, EntityState>) -> MoreInfoData {
    let domain = entity.split('.').next().unwrap_or("").to_string();
    let empty = EntityState { state: "unavailable".into(), ..Default::default() };
    let st = states.get(entity).unwrap_or(&empty);
    let tile = tile(&CardRef { entity: entity.to_string(), name: name.map(str::to_string) }, states);
    let mut d = MoreInfoData {
        entity: entity.into(),
        name: tile.name.clone(),
        icon: tile.icon.clone(),
        state: tile.state.clone(),
        accent: tile.accent,
        on: tile.on,
        domain: domain.as_str().into(),
        hvac_index: -1,
        fan_index: -1,
        preset_index: -1,
        swing_index: -1,
        source_index: -1,
        ..Default::default()
    };
    match domain.as_str() {
        "light" => {
            d.can_toggle = true;
            d.has_brightness = light_has_brightness(st);
            d.brightness = attr_f64(st, "brightness").map(|b| b / 255.0).unwrap_or(0.0) as f32;
            let modes = light_modes(st);
            if modes.iter().any(|m| m == "color_temp") {
                d.has_ct = true;
                let min = attr_f64(st, "min_color_temp_kelvin").unwrap_or(2000.0);
                let max = attr_f64(st, "max_color_temp_kelvin").unwrap_or(6500.0);
                d.ct_min = min as i32;
                d.ct_max = max as i32;
                d.ct = attr_f64(st, "color_temp_kelvin").map(|k| ((k - min) / (max - min)).clamp(0.0, 1.0)).unwrap_or(0.5) as f32;
            }
            if light_has_color(st) {
                d.has_color = true;
                d.hue = st
                    .attrs
                    .get("hs_color")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(Value::as_f64)
                    .map(|h| h / 360.0)
                    .unwrap_or(0.0) as f32;
            }
        }
        "switch" | "input_boolean" | "fan" => d.can_toggle = true,
        "cover" => {
            let features = attr_f64(st, "supported_features").unwrap_or(0.0) as u64;
            d.has_position = features & 4 != 0;
            d.position = attr_f64(st, "current_position").map(|p| p / 100.0).unwrap_or(0.0) as f32;
        }
        "climate" => {
            d.can_toggle = attr_list(st, "hvac_modes").iter().any(|m| m == "off");
            d.target = tile.target.clone();
            d.current = tile.current.clone();
            let hvac = attr_list(st, "hvac_modes");
            d.hvac_index = index_of(&hvac, Some(&st.state));
            d.hvac_labels = labels(&hvac);
            let fan = attr_list(st, "fan_modes");
            d.fan_index = index_of(&fan, attr_str(st, "fan_mode"));
            d.fan_labels = labels(&fan);
            let preset = attr_list(st, "preset_modes");
            d.preset_index = index_of(&preset, attr_str(st, "preset_mode"));
            d.preset_labels = labels(&preset);
            let swing = attr_list(st, "swing_modes");
            d.swing_index = index_of(&swing, attr_str(st, "swing_mode"));
            d.swing_labels = labels(&swing);
        }
        "media_player" => {
            d.can_toggle = true;
            d.title = tile.title.clone();
            d.artist = tile.artist.clone();
            d.playing = tile.playing;
            d.volume = attr_f64(st, "volume_level").unwrap_or(0.0) as f32;
            let sources = attr_list(st, "source_list");
            d.source_index = index_of(&sources, attr_str(st, "source"));
            // Source names are shown as Home Assistant reports them.
            d.source_labels = ModelRc::new(VecModel::from(sources.iter().map(SharedString::from).collect::<Vec<_>>()));
        }
        _ => {}
    }
    d
}
