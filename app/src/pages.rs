//! What the pages show, from Home Assistant: the page list, the clock, the
//! living weather (condition → mood, rain, wind, the real sun and moon), the
//! home widgets, music, timers and the people at home.

use std::collections::HashMap;

use serde_json::Value;
use slint::{Color, Model, ModelRc, SharedString, VecModel};

use crate::ha::EntityState;
use crate::{ClockData, HomeEvent, HomeItem, HomeMusic, PageSlot, Person, TimerInfo, TimerItem, WeatherData, WeatherDay, WeatherHour};

// ---------------------------------------------------------------- time helpers

pub fn now() -> i64 {
    // SAFETY: time(NULL).
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

pub fn local_format(epoch: i64, pattern: &str) -> String {
    // SAFETY: localtime_r/strftime on local buffers.
    unsafe {
        let t = epoch as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let mut buf = [0u8; 64];
        let pattern = std::ffi::CString::new(pattern).unwrap();
        let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), pattern.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

/// Seconds since the epoch of an ISO 8601 time ("2026-11-15T14:00:00+01:00").
pub fn parse_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> { s.get(from..from + len)?.parse().ok() };
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (h, mi) = if b.len() >= 16 { (num(11, 2)?, num(14, 2)?) } else { (0, 0) };
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

fn attr<'a>(st: &'a EntityState, key: &str) -> Option<&'a Value> {
    st.attrs.get(key)
}

fn attr_f64(st: &EntityState, key: &str) -> Option<f64> {
    attr(st, key).and_then(Value::as_f64)
}

fn attr_str<'a>(st: &'a EntityState, key: &str) -> Option<&'a str> {
    attr(st, key).and_then(Value::as_str)
}

fn glyph(name: &str) -> SharedString {
    crate::icons::glyph(name).map(|c| c.to_string()).unwrap_or_default().into()
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

// ---------------------------------------------------------------- pages

/// The pages in their order. Until the integration sets a list: the home page
/// (living weather + widgets), the weather, music and timers.
pub fn page_slots(config: Option<&Value>) -> Vec<PageSlot> {
    let list: Vec<PageSlot> = config
        .and_then(|c| c.get("pages"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let kind = p.get("kind").and_then(Value::as_str)?;
                    let opts = p.get("options");
                    KINDS.contains(&kind).then(|| PageSlot {
                        kind: kind.into(),
                        variant: p.get("variant").and_then(Value::as_i64).unwrap_or(0) as i32,
                        theme: if kind == "home" {
                            opts.and_then(|o| o.get("theme")).and_then(Value::as_i64).map(|t| t.clamp(0, 9) as i32).unwrap_or(-1)
                        } else {
                            -1
                        },
                        layout: -1,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !list.is_empty() {
        // the weather pages after the first with widgets of their own: their layout's index
        let mut n = 0;
        let mut list = list;
        let pages = config.and_then(|c| c.get("pages")).and_then(Value::as_array);
        let mut homes = 0;
        for (slot, p) in list.iter_mut().zip(pages.into_iter().flatten().filter(|p| p.get("kind").and_then(Value::as_str).is_some_and(|k| KINDS.contains(&k)))) {
            if slot.kind == "home" {
                homes += 1;
                if homes > 1 && p.get("options").and_then(|o| o.get("items")).and_then(Value::as_array).is_some() {
                    slot.layout = n;
                    n += 1;
                }
            }
        }
        return list;
    }
    ["home", "weather", "music", "timers"].iter().map(|k| PageSlot { kind: (*k).into(), variant: 0, theme: -1, layout: -1 }).collect()
}

/// The widget layouts of the weather pages after the first that have their own (in page order).
pub fn home_layouts(config: Option<&Value>, accent: Color) -> Vec<Vec<HomeItem>> {
    let Some(pages) = config.and_then(|c| c.get("pages")).and_then(Value::as_array) else { return Vec::new() };
    pages
        .iter()
        .filter(|p| p.get("kind").and_then(Value::as_str) == Some("home"))
        .skip(1)
        .filter_map(|p| p.get("options").and_then(|o| o.get("items")).and_then(Value::as_array))
        .map(|a| items_from(a, accent))
        .collect()
}

fn items_from(a: &[Value], accent: Color) -> Vec<HomeItem> {
    a.iter()
        .map(|i| {
            let n = |k: &str, d: i64| i.get(k).and_then(Value::as_i64).unwrap_or(d) as i32;
            HomeItem {
                kind: n("kind", 0),
                col: n("col", 0),
                row: n("row", 0),
                w: n("w", 4),
                h: n("h", 1),
                style: n("style", 0),
                accent: i.get("accent").and_then(Value::as_str).and_then(parse_color).unwrap_or(accent),
                weight: n("weight", 600),
                tap: i.get("tap").and_then(Value::as_bool).unwrap_or(true),
                opts: n("opts", 0),
            }
        })
        .collect()
}

/// Every page kind the panel can show.
pub const KINDS: &[&str] = &["home", "weather", "clock", "music", "timers", "thermostat", "quick", "camera", "calendar", "todo", "info"];

// ---------------------------------------------------------------- clock

/// The clock as the clock faces and widgets use it; `seconds` moves smoothly
/// on the panel (the app sets it once a second, the UI interpolates).
pub fn clock_data(weather: &WeatherData, next: &str, prev_digits: &[String; 4], flip: f32) -> (ClockData, [String; 4]) {
    if !crate::clock::valid() {
        let d = ["-".to_string(), "-".to_string(), "-".to_string(), "-".to_string()];
        let data = ClockData {
            hh: "--".into(),
            mm: "--".into(),
            d1: "-".into(),
            d2: "-".into(),
            d3: "-".into(),
            d4: "-".into(),
            o1: "-".into(),
            o2: "-".into(),
            o3: "-".into(),
            o4: "-".into(),
            flip: 1.0,
            ..Default::default()
        };
        return (data, d);
    }
    let t = now();
    let hh = local_format(t, "%H");
    let mm = local_format(t, "%M");
    let sec: f32 = local_format(t, "%S").parse().unwrap_or(0.0);
    let digits: [String; 4] = [hh[0..1].to_string(), hh[1..2].to_string(), mm[0..1].to_string(), mm[1..2].to_string()];
    let data = ClockData {
        hh: hh.clone().into(),
        mm: mm.clone().into(),
        seconds: sec,
        date: local_format(t, "%A, %e %B").split_whitespace().collect::<Vec<_>>().join(" ").into(),
        weekday: local_format(t, "%a").to_uppercase().into(),
        day: local_format(t, "%e").trim().to_string().into(),
        weather_icon: weather.icon.clone(),
        temperature: weather.temperature.clone(),
        condition: weather.condition.clone(),
        high_low: weather.high_low.clone(),
        next: next.into(),
        d1: digits[0].clone().into(),
        d2: digits[1].clone().into(),
        d3: digits[2].clone().into(),
        d4: digits[3].clone().into(),
        o1: prev_digits[0].clone().into(),
        o2: prev_digits[1].clone().into(),
        o3: prev_digits[2].clone().into(),
        o4: prev_digits[3].clone().into(),
        flip,
    };
    (data, digits)
}

// ---------------------------------------------------------------- weather

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

pub fn condition_text(state: &str) -> String {
    match state {
        "partlycloudy" => "Partly cloudy".into(),
        "clear-night" => "Clear".into(),
        "lightning" => "Thunder".into(),
        "lightning-rainy" => "Thunderstorm".into(),
        "snowy-rainy" => "Sleet".into(),
        "windy-variant" => "Windy".into(),
        "pouring" => "Pouring".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

/// The photo's mood for a Home Assistant condition (day moods; with the real
/// sun the photo turns to its night counterpart by the sun's elevation) and
/// the rain over it (0 from the mood, 1 drizzle, 2 rain, 3 pouring).
pub fn mood_for(condition: &str) -> (i32, i32) {
    match condition {
        "sunny" => (0, 0),
        "partlycloudy" => (1, 0),
        "cloudy" => (2, 0),
        "rainy" => (3, 2),
        "pouring" => (3, 3),
        "snowy" | "snowy-rainy" => (4, 0),
        "fog" => (5, 0),
        "windy" | "windy-variant" => (6, 0),
        "clear-night" => (7, 0),
        "lightning" => (8, 0),
        "lightning-rainy" => (8, 2),
        "hail" => (8, 3),
        "exceptional" => (9, 0),
        _ => (2, 0),
    }
}

/// The moon seen from (lat, lon) at `t`: azimuth (from north, clockwise) and
/// elevation in degrees, and the phase (0 new, 0.5 full). Low-precision
/// formulas (about a degree), enough for a picture of the sky.
pub fn moon(lat: f64, lon: f64, t: i64) -> (f64, f64, f64) {
    let rad = std::f64::consts::PI / 180.0;
    let d = t as f64 / 86400.0 + 2440587.5 - 2451545.0;
    let e = rad * 23.4397;
    let l0 = rad * (218.316 + 13.176396 * d);
    let m = rad * (134.963 + 13.064993 * d);
    let f = rad * (93.272 + 13.229350 * d);
    let l = l0 + rad * 6.289 * m.sin();
    let b = rad * 5.128 * f.sin();
    let ra = (l.sin() * e.cos() - b.tan() * e.sin()).atan2(l.cos());
    let dec = (b.sin() * e.cos() + b.cos() * e.sin() * l.sin()).asin();
    let lw = rad * -lon;
    let phi = rad * lat;
    let sidereal = rad * (280.16 + 360.9856235 * d) - lw;
    let h = sidereal - ra;
    let alt = (phi.sin() * dec.sin() + phi.cos() * dec.cos() * h.cos()).asin();
    let az_south = h.sin().atan2(h.cos() * phi.sin() - dec.tan() * phi.cos());
    let az = (az_south / rad + 180.0).rem_euclid(360.0);
    // Phase from the synodic month (new moon 2000-01-06 18:14 UTC).
    let phase = ((d + 2451545.0 - 2451550.26) / 29.530588853).rem_euclid(1.0);
    (az, alt / rad, phase)
}

pub struct WeatherInput<'a> {
    pub states: &'a HashMap<String, EntityState>,
    pub entity: &'a str,
    pub hourly: &'a [Value],
    pub daily: &'a [Value],
    /// Photo / drawn theme (0..7), 8 = the Fast Lane scene.
    pub theme: i32,
}

pub fn weather_data(w: &WeatherInput) -> WeatherData {
    let mut data = WeatherData { theme: w.theme, real_sun: true, sun_az: 180.0, sun_el: 40.0, moon_el: -30.0, moon_phase: 0.5, ..Default::default() };
    if let Some(sun) = w.states.get("sun.sun") {
        data.sun_az = attr_f64(sun, "azimuth").unwrap_or(180.0) as f32;
        data.sun_el = attr_f64(sun, "elevation").unwrap_or(40.0) as f32;
        data.sunrise = attr_str(sun, "next_rising").and_then(parse_iso).map(|t| local_format(t, "%H:%M")).unwrap_or_default().into();
        data.sunset = attr_str(sun, "next_setting").and_then(parse_iso).map(|t| local_format(t, "%H:%M")).unwrap_or_default().into();
    }
    if let Some(home) = w.states.get("zone.home") {
        if let (Some(lat), Some(lon)) = (attr_f64(home, "latitude"), attr_f64(home, "longitude")) {
            let (az, el, phase) = moon(lat, lon, now());
            data.moon_az = az as f32;
            data.moon_el = el as f32;
            data.moon_phase = phase as f32;
        }
    }
    let Some(st) = w.states.get(w.entity) else {
        data.mood = 2;
        return data;
    };
    let cond = st.state.as_str();
    let (mood, rain) = mood_for(cond);
    data.mood = mood;
    data.rain = rain;
    data.icon = glyph(weather_icon(cond));
    data.condition = condition_text(cond).into();
    let unit = attr_str(st, "temperature_unit").unwrap_or("°C");
    let deg = |v: f64| format!("{}°", v.round() as i64);
    if let Some(t) = attr_f64(st, "temperature") {
        data.temperature = deg(t).into();
    }
    if let Some(t) = attr_f64(st, "apparent_temperature") {
        data.feels = format!("Feels like {}", deg(t)).into();
    }
    let speed_unit = attr_str(st, "wind_speed_unit").unwrap_or("km/h");
    let to_kmh = |v: f64| match speed_unit {
        "m/s" => v * 3.6,
        "mph" => v * 1.609,
        "kn" => v * 1.852,
        _ => v,
    };
    if let Some(v) = attr_f64(st, "wind_speed") {
        data.wind = format!("{} {speed_unit}", v.round() as i64).into();
        data.wind_kmh = to_kmh(v) as f32;
    }
    data.gust_kmh = attr_f64(st, "wind_gust_speed").map(to_kmh).unwrap_or(data.wind_kmh as f64 * 1.4) as f32;
    if let Some(h) = attr_f64(st, "humidity") {
        data.humidity = format!("{} %", h.round() as i64).into();
    }
    let _ = unit;
    // Hours and days from the forecasts.
    let t_now = now();
    let hours: Vec<WeatherHour> = w
        .hourly
        .iter()
        .filter(|f| f.get("datetime").and_then(Value::as_str).and_then(parse_iso).is_some_and(|t| t + 3600 > t_now))
        .take(12)
        .map(|f| {
            let t = f.get("datetime").and_then(Value::as_str).and_then(parse_iso).unwrap_or(t_now);
            WeatherHour {
                label: local_format(t, "%H").into(),
                icon: glyph(weather_icon(f.get("condition").and_then(Value::as_str).unwrap_or(""))),
                temp: f.get("temperature").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                precip: f.get("precipitation_probability").and_then(Value::as_f64).unwrap_or(0.0).round() as i32,
            }
        })
        .collect();
    if let Some(p) = hours.first() {
        data.precip = format!("{} %", p.precip).into();
    }
    let days: Vec<WeatherDay> = w
        .daily
        .iter()
        .take(7)
        .map(|f| {
            let t = f.get("datetime").and_then(Value::as_str).and_then(parse_iso).unwrap_or(t_now);
            let today = local_format(t, "%Y%m%d") == local_format(t_now, "%Y%m%d");
            WeatherDay {
                label: if today { "Today".into() } else { local_format(t, "%a").into() },
                icon: glyph(weather_icon(f.get("condition").and_then(Value::as_str).unwrap_or(""))),
                high: f.get("temperature").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                low: f.get("templow").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                precip: f.get("precipitation_probability").and_then(Value::as_f64).unwrap_or(0.0).round() as i32,
            }
        })
        .collect();
    if let Some(d) = days.first() {
        data.high_low = format!("H {} · L {}", deg(d.high as f64), deg(d.low as f64)).into();
    }
    data.hours = model(hours);
    data.days = model(days);
    data
}

// ---------------------------------------------------------------- home widgets

/// The home page's widgets until the integration sets them: big digits with
/// the seconds line, the forecast curve and the music widget.
pub fn home_items(config: Option<&Value>, accent: Color) -> Vec<HomeItem> {
    // the first weather page's own widgets, else the panel's
    let first = config
        .and_then(|c| c.get("pages"))
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|p| p.get("kind").and_then(Value::as_str) == Some("home")))
        .and_then(|p| p.get("options"))
        .and_then(|o| o.get("items"))
        .and_then(Value::as_array);
    let from_config: Vec<HomeItem> = first
        .or_else(|| config.and_then(|c| c.get("home_items")).and_then(Value::as_array))
        .map(|a| items_from(a, accent))
        .unwrap_or_default();
    if !from_config.is_empty() {
        return from_config;
    }
    vec![
        HomeItem { kind: 0, col: 0, row: 0, w: 4, h: 1, style: 1, accent, weight: 600, tap: true, opts: 1 },
        HomeItem { kind: 4, col: 0, row: 1, w: 4, h: 1, style: 0, accent, weight: 600, tap: true, opts: 3 },
        HomeItem { kind: 7, col: 0, row: 3, w: 4, h: 1, style: 0, accent, weight: 600, tap: true, opts: 0 },
    ]
}

pub fn parse_color(s: &str) -> Option<Color> {
    let h = s.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// The music widget's data (and the music page's) for a media player.
pub fn home_music(states: &HashMap<String, EntityState>, player: &str, art: Option<&slint::Image>, accent: Color) -> HomeMusic {
    let Some(p) = states.get(player) else { return HomeMusic { accent, ..Default::default() } };
    let active = matches!(p.state.as_str(), "playing" | "paused" | "buffering");
    let mut pos = attr_f64(p, "media_position").unwrap_or(0.0);
    if p.state == "playing" {
        // The position is reported at its update time; it moves on since.
        if let Some(at) = attr_str(p, "media_position_updated_at").and_then(parse_iso) {
            pos += (now() - at).max(0) as f64;
        }
    }
    let dur = attr_f64(p, "media_duration").unwrap_or(0.0);
    HomeMusic {
        title: attr_str(p, "media_title").unwrap_or("").into(),
        artist: attr_str(p, "media_artist").unwrap_or("").into(),
        player: attr_str(p, "friendly_name").unwrap_or(player).into(),
        active,
        playing: p.state == "playing",
        pos: if dur > 0.0 { pos.min(dur) as f32 } else { pos as f32 },
        dur: dur as f32,
        art: art.cloned().unwrap_or_default(),
        has_art: art.is_some(),
        accent,
    }
}

/// The next calendar event for the widgets (none until the calendar page is set up).
pub fn home_event() -> HomeEvent {
    HomeEvent::default()
}

// ---------------------------------------------------------------- people

const PERSON_TINTS: [(u8, u8, u8); 6] = [(0x3b, 0x82, 0xf6), (0x22, 0xa0, 0x6b), (0xa8, 0x55, 0xf7), (0xf5, 0x9e, 0x0b), (0xef, 0x44, 0x44), (0x06, 0xb6, 0xd4)];

/// Small heads of the people in Home Assistant (person.*), home or away.
pub fn people(states: &HashMap<String, EntityState>, wanted: &[String]) -> Vec<Person> {
    let mut ids: Vec<&String> = if wanted.is_empty() {
        states.keys().filter(|k| k.starts_with("person.")).collect()
    } else {
        wanted.iter().filter(|k| states.contains_key(*k)).collect()
    };
    ids.sort();
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            let st = &states[*id];
            let name = attr_str(st, "friendly_name").unwrap_or(id.trim_start_matches("person."));
            let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
            let (r, g, b) = PERSON_TINTS[i % PERSON_TINTS.len()];
            Person { initials: initials.into(), home: st.state == "home", tint: Color::from_rgb_u8(r, g, b) }
        })
        .collect()
}

// ---------------------------------------------------------------- timers

pub struct Timer {
    pub name: String,
    pub active: bool,
    pub end: std::time::Instant,
    pub left: u32,
    pub total: u32,
    /// Order of arrival: the timers page keeps them in this order.
    pub seq: u64,
    /// The panel runs it: started here, or paused / changed here (Home
    /// Assistant's own end of it no longer counts).
    pub own: bool,
}

impl Timer {
    pub fn remaining(&self) -> u32 {
        if self.active {
            self.end.saturating_duration_since(std::time::Instant::now()).as_secs() as u32
        } else {
            self.left
        }
    }
}

fn mmss(s: u32) -> String {
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The status bar's timer chips (nearest first).
pub fn timer_chips(timers: &HashMap<String, Timer>) -> Vec<TimerInfo> {
    let mut list: Vec<&Timer> = timers.values().collect();
    list.sort_by_key(|t| t.remaining());
    list.iter()
        .map(|t| {
            let left = t.remaining();
            TimerInfo { label: t.name.clone().into(), text: mmss(left).into(), urgent: left < 60 }
        })
        .collect()
}

/// The timers in the order the timers page shows them (oldest first): the
/// page's index of a timer is its place here.
pub fn ordered_timers(timers: &HashMap<String, Timer>) -> Vec<(&String, &Timer)> {
    let mut list: Vec<(&String, &Timer)> = timers.iter().collect();
    list.sort_by_key(|(_, t)| t.seq);
    list
}

/// Colourful timers: each its own vibrant colour, by order of arrival.
const TIMER_TINTS: [u32; 6] = [0xff5a36, 0xffb21e, 0x3d9bff, 0x2fd27a, 0xc86bfa, 0xff4fa0];

/// The timers page's list.
pub fn timer_items(timers: &HashMap<String, Timer>, accent: Color, colourful: bool) -> Vec<TimerItem> {
    ordered_timers(timers)
        .iter()
        .map(|(_, t)| {
            let c = TIMER_TINTS[(t.seq as usize) % TIMER_TINTS.len()];
            TimerItem {
                label: t.name.clone().into(),
                remaining: t.remaining() as f32,
                total: t.total.max(1) as f32,
                running: t.active,
                tint: if colourful { Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8) } else { accent },
            }
        })
        .collect()
}

/// Rows of a model, for comparisons before setting a property again.
#[allow(dead_code)]
pub fn same_len<T: Clone + 'static>(m: &ModelRc<T>, n: usize) -> bool {
    m.row_count() == n
}

/// New rows for a model: changed rows are set in place while the number of
/// rows stays (the elements drawn for them stay too); a new model only when
/// the count changes. Returns the model to set, if a new one is needed.
pub fn update_rows<T: Clone + PartialEq + 'static>(current: &ModelRc<T>, rows: Vec<T>) -> Option<ModelRc<T>> {
    if let Some(vec) = current.as_any().downcast_ref::<VecModel<T>>() {
        if vec.row_count() == rows.len() {
            for (i, row) in rows.into_iter().enumerate() {
                if vec.row_data(i).as_ref() != Some(&row) {
                    vec.set_row_data(i, row);
                }
            }
            return None;
        }
    }
    Some(ModelRc::new(VecModel::from(rows)))
}
