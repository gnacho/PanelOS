//! NSPanel Pro panel app: the assistant view's pages (living weather home,
//! music, timers, ...) drawn natively, the Home Assistant dashboard in the web
//! view as the second display mode, the voice satellite, and the panel's own
//! device in Home Assistant.

#![allow(clippy::too_many_arguments)]

mod api_link;
mod alarms;
mod alerts;
mod artwork;
mod auth;
mod cal;
mod cam;
// Camera pictures: JPEG decode, frames through Home Assistant's camera proxy.
#[allow(dead_code)]
mod camera;
mod clock;
mod config;
mod display;
mod draw;
mod esphome;
#[cfg(feature = "gpu")]
mod gpu;
mod ha;
mod hw;
mod icons;
mod info;
mod loading_text;
mod mdns;
mod mus;
mod pages;
mod quick;
mod pairing;
mod panel_device;
mod platform;
mod roam;
mod settings;
mod shade;
mod synth;
mod texts;
mod thermo;
mod todo;
mod touch;
mod voice;
mod web;
mod webstats;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
#[cfg(not(feature = "gpu"))]
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use auth::Auth;
use config::{AppConfig, PERSIST_DIR};
use ha::{Command, EntityState, Event};
use settings::Settings;

slint::include_modules!();

/// Budapest time as a POSIX TZ rule (works without zoneinfo files).
const DEFAULT_TZ: &str = "CET-1CEST,M3.5.0,M10.5.0/3";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Pull to refresh: a pull counts after this many px down, and reloads the
/// page after this many more.
const PULL_START: f32 = 16.0;
const PULL_DISTANCE: f32 = 110.0;

extern "C" {
    // POSIX; not exported by the libc crate for this target.
    fn tzset();
}

pub struct UiState {
    config: Option<AppConfig>,
    auth: Option<Arc<Auth>>,
    shared: Arc<ha::Shared>,
    commands: Option<Sender<Command>>,
    settings: Settings,
    states: HashMap<String, EntityState>,
    pairing_started: bool,
    dashboards: Vec<(String, String)>,
    connected: bool,
    /// Connected at least once since the start (offline screen vs connecting screen).
    ever_connected: bool,
    /// Home Assistant away since (the offline screen comes after a grace period).
    offline_since: Option<Instant>,
    offline_text: String,
    screensaver: bool,
    /// The screensaver started by itself (idle), not by Home Assistant: it ends when its time window does.
    saver_auto: bool,
    /// Status screens (newest last) and the critical alert sent by the integration; whether the strip is theirs.
    statuses: Vec<alerts::Status>,
    critical: Option<alerts::Critical>,
    alert_strip: bool,
    backlight: hw::Backlight,
    light: hw::LightSensor,
    proximity: hw::ProximityWake,
    /// Proximity test view on: the integration time asked for in
    /// PROXIMITY_TEST_FILE (empty = the default one).
    proximity_test: Option<String>,
    /// Backlight now and where it is heading, per mille of the PWM range;
    /// `fade_backlight()` moves it there smoothly.
    brightness: f64,
    brightness_target: f64,
    lux: Option<f64>,
    dark: Option<bool>,
    /// Auto theme: the other look the light sensor asks for, and since when.
    theme_pending: Option<(bool, Instant)>,
    device: Arc<std::sync::Mutex<esphome::DeviceState>>,
    device_notify: Sender<()>,
    last_reported_lux: Option<f64>,
    last_report: Instant,
    /// Five-minute averages of the once-a-minute readings Home Assistant gets.
    soc_temp_avg: Option<f64>,
    wifi_avg: Option<f64>,
    /// Dashboard mode in the web view (None without the web bundle).
    web: Option<Arc<web::WebView>>,
    /// A swipe down from the top edge of the web view (start y): opens the
    /// panel's quick settings instead of going to the page.
    web_swipe: Option<f32>,
    /// Touch test: on, the point to touch now, where and when the finger came down.
    touch_test: bool,
    touch_test_step: usize,
    /// The touch test started from the settings or the integration (not by its file), and its offsets.
    touch_test_manual: bool,
    touch_offsets: Vec<(f32, f32)>,
    touch_result: Option<String>,
    touch_close_at: Option<Instant>,
    touch_started: Option<Instant>,
    touch_test_down: Option<(Instant, f32, f32, u32)>,
    /// Pull to refresh: where the finger came down, and how far it pulled (0..1, None: not pulling).
    web_pull: Option<(f32, f32)>,
    web_pulled: Option<f32>,
    timers: HashMap<String, pages::Timer>,
    /// Arrival counter of the timers (their order and colour).
    timer_seq: u64,
    /// Home Assistant timers removed on the panel: their later events are ignored.
    dismissed_timers: std::collections::HashSet<String>,
    /// Stopwatch: the lap times (newest first) and the time at the last lap.
    laps: Vec<f32>,
    lap_at: f32,
    ringing: Option<Instant>,
    /// Same as `ringing.is_some()`, for the voice thread.
    ringing_flag: Arc<std::sync::atomic::AtomicBool>,
    mic_muted: Arc<std::sync::atomic::AtomicBool>,
    wake_words: Arc<std::sync::Mutex<Vec<String>>>,
    player: Arc<voice::Player>,
    voice_hide_at: Option<Instant>,
    /// The integration's configuration of this panel (pages, widgets, entities).
    panel_config: Option<Value>,
    forecast_hourly: Vec<Value>,
    forecast_daily: Vec<Value>,
    wake_phrase: String,
    /// The status bar's wake hint: for which wake phrase, and its text.
    wake_hint: (String, String),
    /// Quick controls: the title picked for this run.
    quick_title: String,
    art_url: Option<String>,
    art_tx: Option<Sender<Option<String>>>,
    art: Option<slint::Image>,
    /// When a layer opened by a show request closes by itself.
    show_until: Option<Instant>,
    /// Split-flap clock: the digits shown before the last change, and when it changed.
    digits: [String; 4],
    prev_digits: [String; 4],
    flip_at: Option<Instant>,
    /// Weather and forecast labels are refreshed once a minute (and on changes).
    weather_refreshed: Instant,
    /// Development: the system screens were set from the dev input; the
    /// connection state does not change them meanwhile.
    dev_screens: bool,
    /// Do not disturb (quick shade).
    dnd: bool,
    /// Development: settings from the dev input, kept over Home Assistant's.
    dev_config: Option<Value>,
    /// A temporary page shown (its place in the pager).
    temp_page: Option<usize>,
    /// The alarm open in the editor (None: a new one), the minute last checked for alarms.
    alarm_edit: Option<usize>,
    alarm_minute: i64,
    /// Screen off from the quick shade: dark until the next touch or approach.
    screen_off: bool,
    /// What the weather shown last looked like (see weather_key).
    weather_key: String,
    /// Development: weather values forced from the dev input.
    dev_wx: Option<Value>,
}

thread_local! {
    static STATE: RefCell<Option<UiState>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut UiState) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("state")))
}

/// The panel's accent (the brand's Signal colour unless the integration sets one).
fn accent(s: &UiState) -> slint::Color {
    s.panel_config
        .as_ref()
        .and_then(|c| c.get("accent"))
        .and_then(Value::as_str)
        .and_then(pages::parse_color)
        .unwrap_or_else(|| slint::Color::from_rgb_u8(0xFF, 0x4A, 0x1C))
}

fn config_str(s: &UiState, key: &str) -> String {
    s.panel_config.as_ref().and_then(|c| c.get(key)).and_then(Value::as_str).unwrap_or("").to_string()
}

/// The entities a page of `kind` shows (the integration's page list).
fn page_entities(s: &UiState, kind: &str) -> Vec<String> {
    s.panel_config
        .as_ref()
        .and_then(|c| c.get("pages"))
        .and_then(Value::as_array)
        .and_then(|pages| pages.iter().find(|p| p.get("kind").and_then(Value::as_str) == Some(kind)))
        .and_then(|p| p.get("entities"))
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// A switch of the first page of `kind` (the integration's page list).
/// The looks of the pages of a kind on the list (a kind may be there more than once).
pub fn page_looks(ui: &AppWindow, kind: &str) -> Vec<i32> {
    let pages = ui.get_pages();
    (0..pages.row_count()).filter_map(|i| pages.row_data(i)).filter(|p| p.kind == kind).map(|p| p.variant).collect()
}

/// Do not disturb: switched on in the quick shade, or by the integration (on, or on a schedule).
pub fn dnd_active(s: &UiState) -> bool {
    if s.dnd {
        return true;
    }
    let cfg = s.panel_config.as_ref().and_then(|c| c.get("dnd"));
    let get = |k: &str| cfg.and_then(|c| c.get(k));
    match get("mode").and_then(Value::as_str).unwrap_or("off") {
        "on" => true,
        "schedule" => {
            let mins = |k: &str, d: i64| {
                get(k)
                    .and_then(Value::as_str)
                    .and_then(|t| {
                        let (h, m) = t.split_once(':')?;
                        Some(h.trim().parse::<i64>().ok()? * 60 + m.trim().parse::<i64>().ok()?)
                    })
                    .unwrap_or(d)
            };
            let (from, to) = (mins("from", 22 * 60), mins("to", 7 * 60));
            let t = pages::now();
            let now = pages::local_format(t, "%H").parse::<i64>().unwrap_or(0) * 60 + pages::local_format(t, "%M").parse::<i64>().unwrap_or(0);
            if from <= to {
                now >= from && now < to
            } else {
                now >= from || now < to
            }
        }
        _ => false,
    }
}

/// What comes through do not disturb (alarms, timers, status, ring, voice, calls; all by default).
/// Critical alerts always do.
pub fn dnd_allows(s: &UiState, what: &str) -> bool {
    !dnd_active(s)
        || s.panel_config
            .as_ref()
            .and_then(|c| c.get("dnd"))
            .and_then(|d| d.get("allow"))
            .and_then(|a| a.get(what))
            .and_then(Value::as_bool)
            .unwrap_or(true)
}

/// A new notification from Home Assistant: a strip on top for a few seconds (the integration's
/// "New notifications": strip / card / off, sound, seconds); silent and none in do not disturb.
fn popup(ui: &AppWindow, title: &str, message: &str) {
    let (kind, sound, seconds, quiet) = with_state(|s| {
        let p = s.panel_config.as_ref().and_then(|c| c.get("popups"));
        let kind = p.and_then(|p| p.get("kind")).and_then(Value::as_str).unwrap_or("strip").to_string();
        let sound = p.and_then(|p| p.get("sound")).and_then(Value::as_bool).unwrap_or(true);
        let seconds = p.and_then(|p| p.get("seconds")).and_then(Value::as_u64).unwrap_or(10).clamp(3, 120);
        (kind, sound, seconds, dnd_active(s) || s.screensaver || s.alert_strip)
    });
    if kind == "off" || quiet {
        return;
    }
    ui.set_strip_title(if title.is_empty() { "Home Assistant".into() } else { title.into() });
    ui.set_strip_sub(message.into());
    ui.set_strip_icon(icons::glyph("bell-outline").map(String::from).unwrap_or_default().into());
    ui.set_strip_has_picto(false);
    ui.set_strip_critical(false);
    ui.set_strip_tint(slint::Color::from_rgb_u8(0x2a, 0x2c, 0x2f));
    ui.set_strip_on(true);
    with_state(|s| {
        s.show_until = Some(Instant::now() + Duration::from_secs(seconds));
        if sound {
            s.player.play(voice::Sound::Chime);
        }
    });
}

fn page_option(s: &UiState, kind: &str, key: &str) -> Option<Value> {
    s.panel_config
        .as_ref()?
        .get("pages")?
        .as_array()?
        .iter()
        .find(|p| p.get("kind").and_then(Value::as_str) == Some(kind))?
        .get("options")?
        .get(key)
        .cloned()
}

fn weather_entity(s: &UiState) -> String {
    if let Some(e) = page_entities(s, "weather").into_iter().next() {
        return e;
    }
    let e = config_str(s, "weather_entity");
    if e.is_empty() { s.settings.weather_entity.clone() } else { e }
}

fn media_entity(s: &UiState) -> String {
    // The player picked on the panel (remembered), else the music page's, else the integration's.
    if !s.settings.music_player.is_empty() {
        return s.settings.music_player.clone();
    }
    page_entities(s, "music").into_iter().next().unwrap_or_else(|| config_str(s, "media_player_entity"))
}

/// Entities the panel follows besides the dashboard's.
fn extra_entities(s: &UiState) -> Vec<String> {
    let mut out = vec!["sun.sun".to_string(), "zone.home".to_string(), weather_entity(s), media_entity(s)];
    out.extend(s.states.keys().filter(|k| k.starts_with("person.")).cloned());
    if let Some(list) = s.panel_config.as_ref().and_then(|c| c.get("entities")).and_then(Value::as_array) {
        out.extend(list.iter().filter_map(Value::as_str).map(str::to_string));
    }
    // Every page's own entities (thermostats, tiles, cameras, calendars, lists).
    if let Some(pages) = s.panel_config.as_ref().and_then(|c| c.get("pages")).and_then(Value::as_array) {
        for p in pages {
            if let Some(list) = p.get("entities").and_then(Value::as_array) {
                out.extend(list.iter().filter_map(Value::as_str).map(str::to_string));
            }
            // entities named in a page's options (the doorbell, the door lock)
            if let Some(o) = p.get("options").and_then(Value::as_object) {
                for key in ["ring", "lock", "player"] {
                    if let Some(e) = o.get(key).and_then(Value::as_str).filter(|e| e.contains('.')) {
                        out.push(e.to_string());
                    }
                }
            }
        }
    }
    // the screensaver's light sensor from Home Assistant (panels without their own)
    if let Some(e) = s.panel_config.as_ref().and_then(|c| c.get("screensaver")).and_then(|v| v.get("light_entity")).and_then(Value::as_str) {
        out.push(e.to_string());
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|e| !e.is_empty() && seen.insert(e.clone()));
    out
}

// ---------------------------------------------------------------- clock and pages

/// Whether a model already holds these rows. A new model makes every element of
/// its `for` loop be created again (with its layers and text), so models are set
/// only when their rows change.
fn same_rows<T: Clone + PartialEq + 'static>(old: &ModelRc<T>, new: &[T]) -> bool {
    old.row_count() == new.len() && new.iter().enumerate().all(|(i, n)| old.row_data(i).as_ref() == Some(n))
}

/// What the weather shows, to tell whether a new WeatherData changes anything
/// (its hour and day lists are models, which compare by identity).
fn weather_key(w: &WeatherData) -> String {
    let hours: Vec<String> = w.hours.iter().map(|h| format!("{}{}{}{}", h.label, h.icon, h.temp, h.precip)).collect();
    let days: Vec<String> = w.days.iter().map(|d| format!("{}{}{}{}{}", d.label, d.icon, d.high, d.low, d.precip)).collect();
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:.1}|{:.1}|{:.1}|{:.1}|{:.1}|{:.1}|{:.2}|{}|{}",
        w.icon, w.temperature, w.condition, w.feels, w.wind, w.humidity, w.precip, w.high_low, w.sunrise, w.sunset,
        w.mood, w.rain, w.theme, w.real_sun, w.wind_kmh.round(), w.gust_kmh.round(), w.sun_az, w.sun_el, w.moon_az, w.moon_el,
        w.dusk, w.dawn, w.moon_phase, hours.join(","), days.join(",")
    )
}

fn update_clock(ui: &AppWindow) {
    let beep = STATE.with(|st| {
        let mut guard = st.borrow_mut();
        let s = guard.as_mut()?;
        let weather = ui.get_weather();
        let (mut data, digits) = pages::clock_data(&weather, &ui.get_home_event().title, &s.prev_digits, 1.0);
        if digits != s.digits {
            s.prev_digits = std::mem::replace(&mut s.digits, digits);
            s.flip_at = Some(Instant::now());
        }
        // The struct changes once a minute (a change re-lays out every text bound
        // to it); the seconds move in the UI from the animation clock, set here
        // when the minute turns (and at the start).
        data.seconds = 0.0;
        if ui.get_clock() != data {
            ui.set_clock(data);
            let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            ui.invoke_sync_seconds(wall.rem_euclid(60.0) as f32);
        }
        // Timer chips and the timers page only while there are timers (they count down).
        if !s.timers.is_empty() || ui.get_timers().row_count() > 0 {
            // Rows change in place each second (no element is built again).
            if let Some(m) = pages::update_rows(&ui.get_timers(), pages::timer_chips(&s.timers)) {
                ui.set_timers(m);
            }
            if let Some(m) = pages::update_rows(&ui.get_timers_list(), pages::timer_items(&s.timers, accent(s), ui.get_timers_colourful())) {
                ui.set_timers_list(m);
            }
        }
        // Timers the panel runs itself ring here when they reach zero.
        let done: Vec<String> =
            s.timers.iter().filter(|(_, t)| t.own && t.active && t.remaining() == 0).map(|(id, _)| id.clone()).collect();
        for id in done {
            if let Some(t) = s.timers.remove(&id) {
                ring_timer(ui, s, t.name);
                RANG.with(|r| r.set(true));
            }
        }
        // The music position moves on while a player plays: its own properties;
        // the rest of the widget's data only when it changed.
        let music = pages::home_music(&s.states, &media_entity(s), s.art.as_ref(), accent(s));
        if music.playing || ui.get_home_music_pos() != music.pos {
            ui.set_music_pos(music.pos);
            ui.set_home_music_pos(music.pos);
        }
        let mut same = ui.get_home_music();
        same.pos = music.pos;
        if same != music {
            ui.set_home_music(music);
        }
        // The synthwave scenes in use: their view, colour and the next car.
        synth::tick(ui, s);
        // Alarms: when the minute turns, the ones due ring; the list's "next" line follows.
        let minute = pages::now() / 60;
        if minute != s.alarm_minute {
            let first = s.alarm_minute == 0;
            s.alarm_minute = minute;
            if !first {
                if let Some((time, label)) = alarms::due(s) {
                    s.ringing = Some(Instant::now());
                    s.ringing_flag.store(true, Ordering::SeqCst);
                    ui.set_timers_ring_alarm(true);
                    ui.set_timers_ring_time(time.into());
                    ui.set_timers_ring_label(label.into());
                    ui.set_timers_ringing(true);
                    RANG.with(|r| r.set(true));
                }
            }
            alarms::refresh(ui, s);
        }
        // Voice view: closes a few seconds after the end.
        if s.voice_hide_at.is_some_and(|t| Instant::now() >= t) {
            s.voice_hide_at = None;
            ui.set_voice_on(false);
        }
        // Ringing timer: beep until stopped, at most a minute.
        match s.ringing {
            Some(started) if started.elapsed() > Duration::from_secs(60) => {
                s.ringing = None;
                s.ringing_flag.store(false, Ordering::SeqCst);
                ui.set_timers_ringing(false);
                None
            }
            Some(started) => {
                ui.set_timers_ring_over(started.elapsed().as_secs_f32());
                // do not disturb may keep alarms or timers silent (they still show)
                let what = if ui.get_timers_ring_alarm() { "alarms" } else { "timers" };
                dnd_allows(s, what).then(|| s.player.clone())
            }
            None => None,
        }
    });
    // status screens and the critical alert: their times, the strip; the alert sounds until held
    let beep = if alerts::refresh(ui) { beep.or_else(|| Some(with_state(|s| s.player.clone()))) } else { beep };
    if let Some(player) = beep {
        player.play(voice::Sound::Beep);
    }
    if RANG.with(|r| r.replace(false)) {
        wake(ui);
        // the ringing screen is the timers page's: a temporary one when the list has none
        if !go_to_page(ui, "timers") {
            open_temporary_page(ui, "timers");
        }
    }
}

thread_local! {
    /// A timer the panel runs reached zero during this clock update.
    static RANG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Weather, home widgets, music page and people from the current states.
fn refresh_pages(ui: &AppWindow) {
    let (art, music_entity) = with_state(|s| {
        let theme = page_option(s, "home", "theme")
            .and_then(|v| v.as_i64())
            .or_else(|| s.panel_config.as_ref().and_then(|c| c.get("home_theme")).and_then(Value::as_i64))
            .unwrap_or(0)
            .clamp(0, 8) as i32;
        let entity = weather_entity(s);
        let weather = pages::weather_data(&pages::WeatherInput {
            states: &s.states,
            entity: &entity,
            hourly: &s.forecast_hourly,
            daily: &s.forecast_daily,
            theme,
        });
        let mut weather = weather;
        // the weather page's switches: the real sun and moon (photo and drawn backgrounds), cars and
        // neon letters (synthwave)
        if let Some(b) = page_option(s, "home", "real_sun").and_then(|v| v.as_bool()) {
            weather.real_sun = b;
        }
        let cars = page_option(s, "home", "cars").and_then(|v| v.as_bool()).unwrap_or(true);
        if ui.get_home_fl_cars_on() != cars {
            ui.set_home_fl_cars_on(cars);
        }
        let neon = page_option(s, "home", "neon").and_then(|v| v.as_bool()).unwrap_or(ui.get_home_fl_neon());
        if ui.get_home_fl_neon() != neon {
            ui.set_home_fl_neon(neon);
        }
        // Development: weather values forced from the dev input (measurements).
        if let Some(o) = s.dev_wx.as_ref() {
            let f = |k: &str| o.get(k).and_then(Value::as_f64);
            if let Some(v) = f("mood") { weather.mood = v as i32; }
            if let Some(v) = f("rain") { weather.rain = v as i32; }
            if let Some(v) = f("wind") { weather.wind_kmh = v as f32; }
            if let Some(v) = f("gust") { weather.gust_kmh = v as f32; }
            if let Some(v) = f("theme") { weather.theme = v as i32; }
            if let Some(v) = f("sun_el") { weather.sun_el = v as f32; }
        }
        let key = weather_key(&weather);
        if key != s.weather_key {
            s.weather_key = key;
            ui.set_weather(weather);
        }
        let a = accent(s);
        if ui.get_clock_accent() != a {
            ui.set_clock_accent(a);
        }
        let items = pages::home_items(s.panel_config.as_ref(), a);
        if !same_rows(&ui.get_home_items(), &items) {
            ui.set_home_items(ModelRc::new(VecModel::from(items)));
        }
        let lays = pages::home_layouts(s.panel_config.as_ref(), a);
        let shown = ui.get_home_layouts();
        let same = shown.row_count() == lays.len() && lays.iter().enumerate().all(|(i, l)| shown.row_data(i).is_some_and(|o| same_rows(&o.items, l)));
        if !same {
            let lays: Vec<HomeLayout> = lays.into_iter().map(|items| HomeLayout { items: ModelRc::new(VecModel::from(items)) }).collect();
            ui.set_home_layouts(ModelRc::new(VecModel::from(lays)));
        }
        let event = pages::home_event();
        if ui.get_home_event() != event {
            ui.set_home_event(event);
        }
        let t = pages::now();
        if clock::valid() {
            ui.set_home_weekday(pages::local_format(t, "%A").into());
            ui.set_home_day_month(pages::local_format(t, "%e %B").trim().to_string().into());
            ui.set_home_year(pages::local_format(t, "%Y").into());
            ui.set_home_week(format!("Week {}", pages::local_format(t, "%V").trim_start_matches('0')).into());
        }
        let wanted: Vec<String> = s
            .panel_config
            .as_ref()
            .and_then(|c| c.get("people"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        let people = pages::people(&s.states, &wanted);
        if !same_rows(&ui.get_people(), &people) {
            ui.set_people(ModelRc::new(VecModel::from(people)));
        }
        ui.set_show_people(s.panel_config.as_ref().and_then(|c| c.get("show_people")).and_then(Value::as_bool).unwrap_or(true));
        // The wake hint: one of its lines, kept until the wake word changes.
        let hint = if s.settings.mic_muted || s.wake_phrase.is_empty() {
            String::new()
        } else {
            if s.wake_hint.0 != s.wake_phrase {
                s.wake_hint = (s.wake_phrase.clone(), texts::fresh("voice.wake").replace("{wake}", &s.wake_phrase));
            }
            s.wake_hint.1.clone()
        };
        if ui.get_wake_hint().as_str() != hint {
            ui.set_wake_hint(hint.into());
        }
        // Music page
        let player = media_entity(s);
        let music = pages::home_music(&s.states, &player, s.art.as_ref(), a);
        ui.set_music_title(music.title.clone());
        ui.set_music_artist(music.artist.clone());
        ui.set_music_playing(music.playing);
        ui.set_music_pos(music.pos);
        ui.set_music_dur(music.dur);
        ui.set_music_player(music.player.clone());
        ui.set_music_has_art(music.has_art);
        if let Some(art) = &s.art {
            ui.set_music_art(art.clone());
            ui.set_music_art_bg(art.clone());
        }
        ui.set_music_accent(a);
        if let Some(p) = s.states.get(&player) {
            ui.set_music_album(p.attrs.get("media_album_name").and_then(Value::as_str).unwrap_or("").into());
            ui.set_music_volume(p.attrs.get("volume_level").and_then(Value::as_f64).unwrap_or(0.0) as f32);
            ui.set_music_shuffle(p.attrs.get("shuffle").and_then(Value::as_bool).unwrap_or(false));
            ui.set_music_repeat(match p.attrs.get("repeat").and_then(Value::as_str) {
                Some("all") => 1,
                Some("one") => 2,
                _ => 0,
            });
        }
        ui.set_home_music(music);
        let url = s.states.get(&player).and_then(|p| {
            matches!(p.state.as_str(), "playing" | "paused" | "buffering")
                .then(|| p.attrs.get("entity_picture").and_then(Value::as_str).filter(|u| !u.is_empty()).map(str::to_string))
                .flatten()
        });
        let art = if url != s.art_url {
            s.art_url = url.clone();
            s.art_tx.clone().map(|tx| (tx, url))
        } else {
            None
        };
        s.weather_refreshed = Instant::now();
        (art, player)
    });
    let _ = music_entity;
    // The looks of the pages that take theirs from the page list.
    let pages = ui.get_pages();
    let variant = |kind: &str| (0..pages.row_count()).filter_map(|i| pages.row_data(i)).find(|p| p.kind == kind).map(|p| p.variant);
    if let Some(v) = variant("timers") {
        if ui.get_timers_theme() != v {
            ui.set_timers_theme(v);
        }
    }
    if let Some(v) = variant("music") {
        if ui.get_music_style() != v {
            ui.set_music_style(v);
        }
    }
    with_state(|s| {
        // The timers page's functions (alarm, timer, stopwatch) and its colourful timers.
        if let Some(list) = page_option(s, "timers", "functions").and_then(|v| v.as_array().cloned()) {
            let on = |k: &str| list.iter().any(|v| v.as_str() == Some(k));
            let (a, t, w) = (on("alarm"), on("timer"), on("stopwatch"));
            if a || t || w {
                ui.set_timers_alarm_on(a);
                ui.set_timers_timer_on(t);
                ui.set_timers_watch_on(w);
                let mode = ui.get_timers_mode();
                if ![a, t, w][mode.clamp(0, 2) as usize] {
                    ui.set_timers_mode(if t { 1 } else if a { 0 } else { 2 });
                }
            }
        }
        if let Some(c) = page_option(s, "timers", "colourful").and_then(|v| v.as_bool()) {
            ui.set_timers_colourful(c);
        }
        quick::refresh(ui, s);
        thermo::refresh(ui, s);
        cam::refresh(ui, s);
        cal::refresh(ui, s);
        todo::refresh(ui, s);
        info::refresh(ui, s);
    });
    if let Some((tx, url)) = art {
        let _ = tx.send(url);
    }
}

/// Keys of `top` replace those of `base` (a missing base becomes `top`).
fn overlay(base: &mut Option<Value>, top: &Value) {
    let b = base.get_or_insert_with(|| json!({}));
    if let (Some(b), Some(t)) = (b.as_object_mut(), top.as_object()) {
        for (k, v) in t {
            b.insert(k.clone(), v.clone());
        }
    }
}

thread_local! {
    /// Draws the pages once after the page list changed.
    static REWARM: slint::Timer = slint::Timer::default();
}

/// Every page drawn once, one per frame, under the page in view, so the first
/// swipe to a new page costs no more than any other.
fn rewarm(ui: &AppWindow) {
    let weak = ui.as_weak();
    let next = std::cell::Cell::new(0i32);
    REWARM.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_millis(40), move || {
            let Some(ui) = weak.upgrade() else { return };
            let k = next.get();
            if k < ui.get_pages().row_count() as i32 {
                ui.set_warm_page(if k == ui.get_page() { -1 } else { k });
                next.set(k + 1);
            } else {
                ui.set_warm_page(-1);
                REWARM.with(|t| t.stop());
            }
        })
    });
}

/// The page list and what the integration's configuration changes.
fn apply_panel_config(ui: &AppWindow) {
    let reload = with_state(|s| {
        let slots = pages::page_slots(s.panel_config.as_ref());
        let current = ui.get_page();
        // A new model rebuilds every page: only when the list really changed.
        let shown = ui.get_pages();
        let same = shown.row_count() == slots.len() && slots.iter().enumerate().all(|(i, p)| shown.row_data(i).as_ref() == Some(p));
        if !same && s.temp_page.is_none() {
            ui.set_pages(ModelRc::new(VecModel::from(slots.clone())));
            rewarm(ui);
        }
        if current as usize >= slots.len() {
            ui.set_page(0);
        }
        ui.set_dashboard_mode(s.settings.display_mode == "dashboard" && s.web.is_some());
        let mut changed = false;
        let extra = extra_entities(s);
        {
            let mut e = s.shared.extra_entities.lock().unwrap();
            if *e != extra {
                *e = extra;
                changed = true;
            }
        }
        {
            let entity = weather_entity(s);
            let mut f = s.shared.forecast_entity.lock().unwrap();
            if *f != entity {
                *f = entity;
                s.forecast_hourly.clear();
                s.forecast_daily.clear();
                changed = true;
            }
        }
        changed
    });
    refresh_pages(ui);
    if reload {
        send(Command::Reload);
    }
}

/// Goes to the first page of `kind`; returns false if the panel has none.
fn go_to_page(ui: &AppWindow, kind: &str) -> bool {
    let pages = ui.get_pages();
    match (0..pages.row_count()).find(|&i| pages.row_data(i).is_some_and(|p| p.kind == kind)) {
        Some(i) => {
            ui.set_page(i as i32);
            true
        }
        None => false,
    }
}

/// A page the panel's list lacks (a home widget's view): slides in next to
/// the current one, without a dot; a swipe back to the right returns, and it
/// is gone once left.
fn open_temporary_page(ui: &AppWindow, kind: &str) {
    if with_state(|s| s.temp_page.is_some()) {
        return;
    }
    let pages = ui.get_pages();
    let Some(vec) = pages.as_any().downcast_ref::<VecModel<PageSlot>>() else { return };
    let at = (ui.get_page().max(0) as usize + 1).min(vec.row_count());
    vec.insert(at, PageSlot { kind: kind.into(), variant: 0, theme: -1, layout: -1 });
    with_state(|s| s.temp_page = Some(at));
    ui.set_dots_hidden(true);
    ui.set_page(at as i32);
    refresh_pages(ui);
}

/// After a swipe: the temporary page goes once it is off screen (the pages
/// after it move up by one, with no visible slide).
fn leave_temporary_page(ui: &AppWindow, now: i32) {
    let Some(t) = with_state(|s| s.temp_page) else { return };
    if now as usize == t {
        return;
    }
    let weak = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(320), move || {
        let Some(ui) = weak.upgrade() else { return };
        if with_state(|s| s.temp_page.take()) != Some(t) {
            return;
        }
        let pages = ui.get_pages();
        let Some(vec) = pages.as_any().downcast_ref::<VecModel<PageSlot>>() else { return };
        if t < vec.row_count() {
            ui.set_instant(true);
            vec.remove(t);
            let page = ui.get_page();
            if page as usize > t {
                ui.set_page(page - 1);
            }
            let weak = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(50), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_instant(false);
                }
            });
        }
        ui.set_dots_hidden(false);
    });
}

// ---------------------------------------------------------------- timers and voice

fn handle_timer(ui: &AppWindow, kind: i32, id: String, name: String, left: u32, total: u32, active: bool) {
    let finished = with_state(|s| {
        if s.dismissed_timers.contains(&id) {
            if kind >= 2 {
                s.dismissed_timers.remove(&id);
            }
            return false;
        }
        match kind {
            // started, updated (unless the panel runs it now)
            0 | 1 => {
                if s.timers.get(&id).is_some_and(|t| t.own) {
                    return false;
                }
                let total = if total > 0 { total } else { s.timers.get(&id).map(|t| t.total).unwrap_or(left) };
                let seq = match s.timers.get(&id) {
                    Some(t) => t.seq,
                    None => {
                        s.timer_seq += 1;
                        s.timer_seq
                    }
                };
                s.timers.insert(
                    id,
                    pages::Timer { name, active, end: Instant::now() + Duration::from_secs(left as u64), left, total, seq, own: false },
                );
                false
            }
            // cancelled
            2 => {
                if !s.timers.get(&id).is_some_and(|t| t.own) {
                    s.timers.remove(&id);
                }
                false
            }
            // finished (a timer the panel runs rings by its own clock)
            3 => {
                if s.timers.get(&id).is_some_and(|t| t.own) {
                    return false;
                }
                let name = s.timers.remove(&id).map(|t| t.name).unwrap_or(name);
                ring_timer(ui, s, name);
                true
            }
            _ => false,
        }
    });
    if finished {
        wake(ui);
        if !go_to_page(ui, "timers") {
            open_temporary_page(ui, "timers");
        }
    }
    update_clock(ui);
}

/// A timer reached zero: the ringing screen and the beeps.
fn ring_timer(ui: &AppWindow, s: &mut UiState, name: String) {
    s.ringing = Some(Instant::now());
    s.ringing_flag.store(true, Ordering::SeqCst);
    ui.set_timers_ring_alarm(false);
    ui.set_timers_ring_label(name.into());
    ui.set_timers_ringing(true);
}

/// A timer started on the panel (a quick start or one of one's own).
fn start_timer(ui: &AppWindow, secs: u32, name: String) {
    with_state(|s| {
        s.timer_seq += 1;
        let seq = s.timer_seq;
        s.timers.insert(
            format!("panel-{seq}"),
            pages::Timer {
                name,
                active: true,
                end: Instant::now() + Duration::from_secs(secs as u64),
                left: secs,
                total: secs,
                seq,
                own: true,
            },
        );
    });
    ui.set_timers_quick(false);
    ui.set_timers_mode(1);
}

/// Taps on the timers page (`timers.*`, `watch.*`).
fn timers_act(ui: &AppWindow, what: &str, value: &str) {
    let n: i64 = value.parse().unwrap_or(0);
    // The timer at the page's index `n`.
    let id_at = |s: &UiState| pages::ordered_timers(&s.timers).get(n as usize).map(|(id, _)| (*id).clone());
    match what {
        "timers.mode" => ui.set_timers_mode(n as i32),
        "timers.pick" => ui.set_timers_focus(n as i32),
        "timers.quick" => ui.set_timers_quick(true),
        "timers.quick-close" => ui.set_timers_quick(false),
        "timers.new" => {
            // a timer of one's own: the editor with hours and minutes (10 min to start from)
            let tint = with_state(|s| accent(s));
            ui.set_timers_edit_timer(true);
            ui.set_timers_edit_new(true);
            ui.set_timers_edit_h(0);
            ui.set_timers_edit_m(10);
            ui.set_timers_edit_label(Default::default());
            ui.set_timers_edit_tint(tint);
            ui.set_timers_edit_keyboard(false);
            ui.set_timers_edit_info(false);
            ui.set_timers_edit(true);
        }
        "timers.create" => {
            let secs = (ui.get_timers_edit_h() as i64 * 3600 + ui.get_timers_edit_m() as i64 * 60).clamp(0, 24 * 3600) as u32;
            if secs == 0 {
                return;
            }
            let name = ui.get_timers_edit_label().trim().to_string();
            ui.set_timers_edit(false);
            ui.set_timers_edit_timer(false);
            start_timer(ui, secs, name);
        }
        "timers.start" => start_timer(ui, n.clamp(1, 24 * 3600) as u32, String::new()),
        "timers.toggle" | "timers.add" | "timers.cancel" => with_state(|s| {
            let Some(id) = id_at(s) else { return };
            match what {
                "timers.cancel" => {
                    if let Some(t) = s.timers.remove(&id) {
                        if !t.own || !id.starts_with("panel-") {
                            s.dismissed_timers.insert(id);
                        }
                    }
                }
                _ => {
                    let Some(t) = s.timers.get_mut(&id) else { return };
                    t.own = true;
                    if what == "timers.add" {
                        if t.active {
                            t.end += Duration::from_secs(60);
                        } else {
                            t.left += 60;
                        }
                        t.total += 60;
                    } else if t.active {
                        t.left = t.remaining();
                        t.active = false;
                    } else {
                        t.end = Instant::now() + Duration::from_secs(t.left as u64);
                        t.active = true;
                    }
                }
            }
        }),
        "watch.toggle" => {
            if ui.get_timers_watch_running() {
                ui.invoke_watch_stop();
            } else {
                ui.invoke_watch_start();
            }
        }
        "watch.lap" => {
            let now = ui.get_timers_watch();
            let laps = with_state(|s| {
                s.laps.insert(0, now - s.lap_at);
                s.lap_at = now;
                s.laps.clone()
            });
            ui.set_timers_laps(ModelRc::new(VecModel::from(laps)));
        }
        "watch.reset" => {
            ui.invoke_watch_stop();
            ui.set_timers_watch_base(0.0);
            with_state(|s| {
                s.laps.clear();
                s.lap_at = 0.0;
            });
            ui.set_timers_laps(ModelRc::new(VecModel::from(Vec::<f32>::new())));
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled yet)"),
    }
    update_clock(ui);
}

fn stop_ringing(ui: &AppWindow) {
    with_state(|s| {
        s.ringing = None;
        s.ringing_flag.store(false, Ordering::SeqCst);
    });
    ui.set_timers_ringing(false);
}

/// The voice view's status line for a state (listen, think, reply, more,
/// again, fail), in the variant of the look: comic captions in capitals, the
/// terminal and the oscilloscope in their mono wording.
fn voice_line(ui: &AppWindow, state: &str) -> String {
    let place = match ui.get_voice_look() {
        1 | 3 => format!("voice.{state}.mono"),
        2 => format!("voice.{state}.comic"),
        _ => format!("voice.{state}"),
    };
    let line = texts::fresh(&place);
    if line.is_empty() {
        texts::fresh(&format!("voice.{state}"))
    } else {
        line
    }
}

thread_local! {
    /// Feeds the voice view's halo and trace while the microphone streams.
    static METER: slint::Timer = slint::Timer::default();
    static TRACE: RefCell<std::collections::VecDeque<f32>> = RefCell::new(std::collections::VecDeque::new());
}

/// The voice view's look (integration), its pictures and its state colours.
fn voice_view(ui: &AppWindow, state: i32) {
    let look = with_state(|s| s.panel_config.as_ref().and_then(|c| c.get("voice_look")).and_then(Value::as_i64).unwrap_or(0)) as i32;
    ui.set_voice_look(look);
    if look == 1 && ui.get_voice_scan().size().width == 0 {
        ui.set_voice_scan(quick::picture("voice/scan.png"));
    }
    if look == 2 {
        let dots = match state {
            1 | 5 => "voice/dots-blue.png",
            3 => "voice/dots-green.png",
            4 => "voice/dots-red.png",
            6 => "voice/dots-violet.png",
            _ => "voice/dots.png",
        };
        ui.set_voice_dots(quick::picture(dots));
    }
}

/// Starts or stops the microphone meter (level 30 times a second, smoothed;
/// the oscilloscope trace from the last 2 s of it).
fn voice_meter(ui: &AppWindow, on: bool) {
    METER.with(|t| {
        if !on {
            t.stop();
            ui.set_voice_level(0.0);
            TRACE.with(|tr| tr.borrow_mut().clear());
            return;
        }
        let weak = ui.as_weak();
        t.start(slint::TimerMode::Repeated, Duration::from_millis(33), move || {
            let Some(ui) = weak.upgrade() else { return };
            let raw = f32::from_bits(voice::MIC_LEVEL.load(Ordering::Relaxed));
            let level = ui.get_voice_level() * 0.6 + raw * 0.4;
            ui.set_voice_level(level);
            if ui.get_voice_look() == 3 {
                // 60 points across a 400 × 160 box, the newest on the right.
                let path = TRACE.with(|tr| {
                    let mut tr = tr.borrow_mut();
                    tr.push_back(raw);
                    while tr.len() > 60 {
                        tr.pop_front();
                    }
                    let mut p = String::new();
                    for (i, v) in tr.iter().enumerate() {
                        let x = 400.0 * i as f32 / 59.0;
                        let swing = if i % 2 == 0 { 1.0 } else { -1.0 };
                        let y = 80.0 - swing * v * 70.0;
                        p.push_str(&format!("{}{x:.1} {y:.1} ", if i == 0 { "M " } else { "L " }));
                    }
                    p
                });
                ui.set_voice_wave(path.into());
            }
        });
    });
}

fn handle_voice_ui(ui: &AppWindow, event: voice::VoiceUi) {
    use voice::VoiceUi;
    let hide_after = |secs: u64| with_state(|s| s.voice_hide_at = Some(Instant::now() + Duration::from_secs(secs)));
    let show = with_state(|s| s.panel_config.as_ref().and_then(|c| c.get("show_conversation")).and_then(Value::as_bool).unwrap_or(true));
    match event {
        VoiceUi::Listening => {
            wake(ui);
            with_state(|s| s.voice_hide_at = None);
            ui.set_voice_heard("".into());
            ui.set_voice_answer("".into());
            ui.set_voice_detail("".into());
            voice_view(ui, 1);
            ui.set_voice_line(voice_line(ui, "listen").into());
            ui.set_voice_state(1);
            voice_meter(ui, true);
            ui.set_voice_on(show);
        }
        VoiceUi::Heard(text) => ui.set_voice_heard(text.into()),
        VoiceUi::Thinking => {
            voice_meter(ui, false);
            voice_view(ui, 2);
            ui.set_voice_line(voice_line(ui, "think").into());
            ui.set_voice_state(2);
        }
        VoiceUi::Answer(text) => {
            ui.set_voice_answer(text.into());
            voice_meter(ui, false);
            voice_view(ui, 3);
            ui.set_voice_line(voice_line(ui, "reply").into());
            ui.set_voice_state(3);
        }
        VoiceUi::Idle => {
            voice_meter(ui, false);
            if ui.get_voice_on() {
                hide_after(5);
            }
        }
        VoiceUi::Error(message) => {
            ui.set_voice_detail(message.into());
            voice_meter(ui, false);
            voice_view(ui, 4);
            ui.set_voice_line(voice_line(ui, "fail").into());
            ui.set_voice_state(4);
            hide_after(5);
        }
        VoiceUi::Stopped => {
            stop_ringing(ui);
            if ui.get_voice_on() {
                hide_after(1);
            }
        }
    }
    platform::note_activity();
}

// ---------------------------------------------------------------- start-up

const SPLASH_PIDFILE: &str = "/var/run/nspanel-splash.pid";

const SPLASH_DONE: &str = "/var/run/nspanel-splash.done";

/// Longest wait for the clock and the first Home Assistant data before the UI
/// takes the display anyway (Home Assistant or the network is down).
const READY_TIMEOUT: Duration = Duration::from_secs(25);

/// Holds the UI back until it has something current to show: the clock is
/// set and the first full state list from Home Assistant is in (a short
/// grace lets the panel settings and forecasts follow). Pairing needed:
/// at once, the QR code is the content. Meanwhile the boot splash keeps its
/// light band running. Returns the events received so far, for the UI.
fn wait_until_ready(events: &mpsc::Receiver<Event>) -> Vec<Event> {
    let started = Instant::now();
    let mut buffered = Vec::new();
    let mut states_at: Option<Instant> = None;
    let mut connected_at: Option<Instant> = None;
    let mut clock_at: Option<Instant> = None;
    loop {
        if clock_at.is_none() && clock::valid() {
            clock_at = Some(Instant::now());
        }
        let data = states_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(300))
            || connected_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(2));
        if (data && clock::valid()) || started.elapsed() >= READY_TIMEOUT {
            break;
        }
        match events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                let pairing = matches!(event, Event::NeedPairing);
                match &event {
                    Event::States(_) => {
                        states_at.get_or_insert_with(Instant::now);
                    }
                    Event::Status { connected: true, .. } => {
                        connected_at.get_or_insert_with(Instant::now);
                    }
                    _ => {}
                }
                buffered.push(event);
                if pairing {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let at = |t: Option<Instant>| t.map(|t| format!("{} ms", (t - started).as_millis())).unwrap_or_else(|| "-".into());
    eprintln!(
        "nspanel-app: ready for the display after {} ms (clock set at {}, connected at {}, states at {}, {} events)",
        started.elapsed().as_millis(),
        at(clock_at),
        at(connected_at),
        at(states_at),
        buffered.len()
    );
    buffered
}

/// Asks the boot splash (nspanel-splash) to hand the display over and waits
/// until it has (at most 40 s). Its pid.
fn splash_handoff() -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(SPLASH_PIDFILE).ok()?.trim().parse().ok()?;
    // SAFETY: plain kill(2).
    if unsafe { libc::kill(pid, libc::SIGUSR1) } != 0 {
        return None;
    }
    let started = Instant::now();
    while !std::path::Path::new(SPLASH_DONE).exists() && started.elapsed() < Duration::from_secs(40) {
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!("nspanel-app: display taken over from the boot splash after {} ms", started.elapsed().as_millis());
    Some(pid)
}

/// Home Assistant URLs to try for what was typed: as given with a scheme or a
/// port, otherwise port 8123 (older and Docker installs) and then port 80
/// (Home Assistant OS installed since 2026.8).
fn setup_candidates(input: &str) -> Vec<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        Vec::new()
    } else if s.starts_with("http://") || s.starts_with("https://") {
        vec![s.to_string()]
    } else if s.contains(':') {
        vec![format!("http://{s}")]
    } else {
        vec![format!("http://{s}:8123"), format!("http://{s}")]
    }
}

/// True if Home Assistant answers at `url`: `/auth/providers` needs no login
/// and only exists there.
fn is_home_assistant(url: &str) -> bool {
    match ureq::get(&format!("{url}/auth/providers")).timeout(Duration::from_secs(4)).call() {
        Ok(r) => r.into_string().is_ok_and(|body| serde_json::from_str::<Value>(&body).is_ok()),
        Err(_) => false,
    }
}

/// First start without settings: the setup screen lists Home Assistant found
/// on the network (mDNS) and takes an address on a keypad. Once Home Assistant
/// answers at the address, it is saved to app.json and the app restarts (the
/// supervisor starts it again), which then shows the pairing QR code.
fn start_setup(ui: &AppWindow, browser: Arc<mdns::Browser>, reason: &str) {
    ui.set_setup(true);
    if !reason.contains("No such file") {
        ui.set_setup_message(format!("The saved settings could not be read ({reason}); set them again.").into());
        ui.set_setup_message_kind(1);
    }
    if let Some(ip) = pairing::local_ip_towards("8.8.8.8:53") {
        ui.set_net_ip(ip.to_string().into());
    }
    browser.active.store(true, Ordering::Relaxed);
    // Found instances, refreshed every second; the timer lives as long as the app.
    let found_timer = slint::Timer::default();
    let weak = ui.as_weak();
    found_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = weak.upgrade() else { return };
        let found: Vec<FoundHa> = browser
            .found
            .lock()
            .unwrap()
            .iter()
            .map(|h| FoundHa { name: h.name.clone().into(), url: h.url.clone().into() })
            .collect();
        if ui.get_setup_found().row_count() != found.len() {
            ui.set_setup_found(ModelRc::new(VecModel::from(found)));
        }
    });
    std::mem::forget(found_timer);

    let weak = ui.as_weak();
    ui.on_setup_key(move |k| {
        let Some(ui) = weak.upgrade() else { return };
        let mut text = ui.get_setup_address().to_string();
        if k == "back" {
            text.pop();
        } else if text.len() < 40 {
            text.push_str(&k);
        }
        ui.set_setup_address(text.into());
    });
    let weak = ui.as_weak();
    ui.on_setup_pick(move |url| {
        let Some(ui) = weak.upgrade() else { return };
        ui.set_setup_pick_open(false);
        ui.set_setup_address(url);
        ui.invoke_setup_connect();
    });
    let weak = ui.as_weak();
    ui.on_setup_choose_many(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_setup_pick_open(true);
        }
    });
    let weak = ui.as_weak();
    ui.on_setup_pick_back(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_setup_pick_open(false);
        }
    });
    let weak = ui.as_weak();
    ui.on_setup_connect(move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_setup_busy() {
            return;
        }
        let candidates = setup_candidates(&ui.get_setup_address());
        if candidates.is_empty() {
            ui.set_setup_message("Type the address first.".into());
            ui.set_setup_message_kind(1);
            return;
        }
        ui.set_setup_busy(true);
        ui.set_setup_message("Looking for Home Assistant…".into());
        ui.set_setup_message_kind(0);
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let found = candidates.iter().find(|u| is_home_assistant(u)).cloned();
            let saved = found.as_ref().map(|url| {
                let _ = std::fs::create_dir_all(PERSIST_DIR);
                let text = serde_json::to_string_pretty(&json!({ "ha_url": url })).unwrap_or_default();
                std::fs::write(format!("{PERSIST_DIR}/app.json"), text + "\n")
            });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                ui.set_setup_busy(false);
                match (found, saved) {
                    (Some(url), Some(Ok(()))) => {
                        eprintln!("nspanel-app: setup: Home Assistant at {url} saved, restarting");
                        ui.set_setup_message(format!("Home Assistant found at {url}. Starting…").into());
                        ui.set_setup_message_kind(2);
                        // The supervisor starts the app again, now with settings.
                        slint::Timer::single_shot(Duration::from_millis(800), || std::process::exit(0));
                    }
                    (Some(_), Some(Err(e))) => {
                        ui.set_setup_message(format!("Could not save the settings: {e}").into());
                        ui.set_setup_message_kind(1);
                    }
                    _ => {
                        ui.set_setup_message(
                            "Home Assistant did not answer at this address. Check the IP address and the port (:8123 or none).".into(),
                        );
                        ui.set_setup_message_kind(1);
                    }
                }
            });
        });
    });
}

fn wake_phrase(ids: &[String]) -> String {
    let Some(id) = ids.first() else { return String::new() };
    voice::available_wake_words().into_iter().find(|w| w.id == *id).map(|w| w.phrase).unwrap_or_default()
}

/// show_view / show_message from Home Assistant (PanelOS integration).
fn handle_show(ui: &AppWindow, v: &Value) {
    if v["type"].as_str() == Some("touch_test") {
        start_touch_test(ui);
        return;
    }
    // status screens and critical alerts (the integration's house-wide rules)
    if matches!(v["type"].as_str(), Some("status" | "status_clear" | "critical" | "critical_clear")) {
        eprintln!("nspanel-app: {} {}", v["type"].as_str().unwrap_or(""), v["id"].as_str().unwrap_or(""));
        alerts::show(ui, v);
        return;
    }
    let kind = v["type"].as_str().unwrap_or("view");
    let seconds = v["seconds"].as_u64().unwrap_or(30);
    eprintln!("nspanel-app: show request {v}");
    wake(ui);
    ui.set_set_view(0);
    let chime = with_state(|s| {
        s.show_until = (seconds > 0).then(|| Instant::now() + Duration::from_secs(seconds));
        v["chime"].as_bool().unwrap_or(false).then(|| s.player.clone())
    });
    match kind {
        "message" => {
            ui.set_strip_title(v["title"].as_str().unwrap_or("").into());
            ui.set_strip_sub(v["message"].as_str().unwrap_or("").into());
            let icon = v["icon"].as_str().unwrap_or("mdi:information-outline").trim_start_matches("mdi:");
            ui.set_strip_icon(icons::glyph(icon).map(String::from).unwrap_or_default().into());
            ui.set_strip_has_picto(false);
            ui.set_strip_critical(false);
            ui.set_strip_on(true);
        }
        _ => match v["view"].as_str().unwrap_or("home") {
            "dashboard" => {
                if ui.get_web_active() {
                    ui.set_dashboard_mode(true);
                }
            }
            "media" => {
                go_to_page(ui, "music");
            }
            other => {
                ui.set_dashboard_mode(with_state(|s| s.settings.display_mode == "dashboard") && ui.get_web_active());
                if !go_to_page(ui, other) {
                    go_to_page(ui, "home");
                }
            }
        },
    }
    if let Some(player) = chime {
        player.play(voice::Sound::Chime);
    }
}

fn send(cmd: Command) {
    let tx = with_state(|s| s.commands.clone());
    match tx {
        Some(tx) => {
            let _ = tx.send(cmd);
        }
        None => {
            if let Command::CallService { domain, service, entity_id, data } = cmd {
                eprintln!("nspanel-app: (no Home Assistant) {domain}.{service} {entity_id} {data}");
            }
        }
    }
}

fn call(domain: &str, service: &str, entity_id: &str, data: Value) {
    send(Command::CallService { domain: domain.into(), service: service.into(), entity_id: entity_id.into(), data });
}

fn start_pairing(ui: &AppWindow) {
    let (config, auth) = with_state(|s| (s.config.clone(), s.auth.clone()));
    let (Some(config), Some(auth)) = (config, auth) else { return };
    let weak = ui.as_weak();
    let on_paired = move || {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_pair_state(1);
                ui.set_paired(true);
                ui.set_connecting(true);
            }
        });
    };
    ui.set_pair_ha_url(config.ha_url.clone().into());
    match pairing::start(&config.ha_url, &config.ha_host_port(), config.http_port, auth, on_paired) {
        Ok(p) => {
            if let Some(img) = pairing::qr_image(&p.authorize_url, 254) {
                ui.set_qr(img);
            }
            ui.set_pair_url(p.authorize_url.clone().into());
            ui.set_pair_hint(
                format!("Scan with your phone and log in to Home Assistant ({}). The panel connects by itself.", config.ha_url)
                    .into(),
            );
            eprintln!("nspanel-app: pairing page at {}, login URL {}", p.panel_url, p.authorize_url);
        }
        Err(e) => {
            ui.set_pair_state(2);
            ui.set_pair_hint(format!("Cannot start pairing: {e}").into());
        }
    }
}

fn handle_event(ui: &AppWindow, event: Event) {
    match event {
        Event::Status { text, connected } => {
            ui.set_connected(connected);
            with_state(|s| {
                s.connected = connected;
                if connected {
                    s.ever_connected = true;
                    s.offline_since = None;
                } else if s.offline_since.is_none() {
                    s.offline_since = Some(Instant::now());
                }
                s.offline_text = text;
            });
            update_connection_screens(ui);
            refresh_pages(ui);
        }
        Event::NeedPairing => {
            ui.set_paired(false);
            ui.set_connected(false);
            ui.set_connecting(false);
            let start = with_state(|s| {
                let first = !s.pairing_started;
                s.pairing_started = true;
                first
            });
            if start {
                start_pairing(ui);
            }
        }
        Event::Dashboard(_) => {
            ui.set_paired(true);
            let need_list = with_state(|s| s.dashboards.is_empty());
            if need_list {
                // Gives the "Dashboard" select of the panel's device its options.
                send(Command::ListDashboards);
            }
        }
        Event::States(changes) => {
            #[cfg(feature = "gpu")]
            if gpu::FRAME_STATS.load(Ordering::Relaxed) {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 100_000).unwrap_or(0);
                let ids: Vec<&str> = changes.iter().map(|(id, _)| id.as_str()).take(6).collect();
                eprintln!("nspanel-app: states at {t} ms: {} {:?}", changes.len(), ids);
            }
            let watched = with_state(|s| {
                let mut new_person = false;
                for (id, st) in &changes {
                    match st {
                        Some(st) => {
                            if id.starts_with("person.") && !s.states.contains_key(id) {
                                new_person = true;
                            }
                            s.states.insert(id.clone(), st.clone());
                        }
                        None => {
                            s.states.remove(id);
                        }
                    }
                }
                let weather = weather_entity(s);
                let media = media_entity(s);
                let touches = changes.iter().any(|(id, _)| {
                    *id == weather || *id == media || id == "sun.sun" || id == "zone.home" || id.starts_with("person.")
                });
                (touches, new_person)
            });
            let (touches, new_person) = watched;
            if touches {
                refresh_pages(ui);
            }
            if new_person {
                apply_panel_config(ui);
            }
        }
        Event::Dashboards(list) => {
            with_state(|s| {
                s.device.lock().unwrap().dashboards = list.clone();
                let _ = s.device_notify.send(());
                s.dashboards = list;
            });
        }
        Event::Panel(config) => {
            with_state(|s| {
                let web = config.as_ref().map(settings::WebConfig::from_integration).unwrap_or_default();
                if web != s.settings.web {
                    eprintln!("nspanel-app: dashboard mode settings from Home Assistant: {web:?}");
                    s.settings.web = web;
                    s.settings.save();
                }
                let reporting = config.as_ref().map(settings::Reporting::from_integration).unwrap_or_default();
                if reporting != s.settings.reporting {
                    eprintln!("nspanel-app: sensor reporting from Home Assistant: {reporting:?}");
                    s.settings.reporting = reporting;
                    s.settings.save();
                }
                s.panel_config = config;
                // Development settings stay on top of what Home Assistant sends.
                if let Some(dev) = s.dev_config.clone() {
                    overlay(&mut s.panel_config, &dev);
                }
            });
            apply_panel_config(ui);
        }
        Event::Show(v) => handle_show(ui, &v),
        Event::Command { key, .. } if key == "screenshot" => {
            // Encoded off the UI thread (about 50 ms on the panel).
            let tx = with_state(|s| s.commands.clone());
            platform::request_screen(move |w, h, rgb| {
                std::thread::spawn(move || {
                    if let (Some(png), Some(tx)) = (panel_device::png_base64(w, h, &rgb), tx) {
                        let _ = tx.send(Command::Screen(png));
                    }
                });
            });
        }
        Event::Command { key, value } => {
            let control = with_state(|s| panel_device::command(&key, &value, &s.device.lock().unwrap()));
            match control {
                Some(control) => apply_control(ui, control),
                None => eprintln!("nspanel-app: unknown command from Home Assistant: {key} = {value}"),
            }
        }
        Event::Notes { kind, notes } => {
            let fresh = if kind == "added" {
                notes.as_object().and_then(|m| m.values().last()).map(|v| {
                    let t = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    (t("title"), t("message"))
                })
            } else {
                None
            };
            info::update(&kind, &notes);
            with_state(|s| info::refresh(ui, s));
            if let Some((title, message)) = fresh {
                popup(ui, &title, &message);
            }
        }
        Event::Forecast { hourly, items } => {
            with_state(|s| if hourly { s.forecast_hourly = items } else { s.forecast_daily = items });
            refresh_pages(ui);
        }
    }
}

/// The connecting screen (paired, no data yet) and the offline screen (no
/// network / Home Assistant away for a while after it worked).
fn update_connection_screens(ui: &AppWindow) {
    with_state(|s| {
        if s.dev_screens {
            return;
        }
        let paired = ui.get_paired();
        ui.set_connecting(paired && !s.connected && !s.ever_connected && s.config.is_some());
        let away = !s.connected && s.ever_connected && s.offline_since.is_some_and(|t| t.elapsed() > Duration::from_secs(15));
        let cause = if !away {
            0
        } else if s.offline_text.contains("network") {
            1
        } else {
            2
        };
        if ui.get_offline_cause() != cause {
            ui.set_offline_cause(cause);
        }
    });
}

/// Carries out a request from Home Assistant (ESPHome API or the PanelOS
/// integration) on the UI thread.
fn apply_control(ui: &AppWindow, control: esphome::Control) {
    match control {
        esphome::Control::Setting(k, v) => on_setting(ui, &k, &v),
        esphome::Control::Screen(true) => wake(ui),
        esphome::Control::Screen(false) => with_state(|s| {
            if !s.screensaver {
                start_screensaver(ui, s);
            }
        }),
        esphome::Control::Timer { kind, id, name, left, total, active } => handle_timer(ui, kind, id, name, left, total, active),
    }
}

fn wake(ui: &AppWindow) {
    platform::note_activity();
    if ui.get_screensaver_on() {
        ui.set_screensaver_on(false);
        with_state(|s| {
            s.screensaver = false;
            s.screen_off = false;
        });
    }
}

/// The darkness below which the screensaver may start by default (lux at the panel). Measured on the
/// panel's own sensor: a lamp-lit evening 5–26 lx, the dark room about 1 lx.
const DARK_LUX: f64 = 3.0;

/// Whether the screensaver may start now (the integration's "When"): only in the dark (the default:
/// darker than `lux`, from the panel's sensor or a Home Assistant one), between two times, from sunset
/// to sunrise, or any time. `leaving`: whether one already on may stay (in the dark it stays until it
/// is clearly brighter, so a reading near the limit does not toggle it).
fn saver_window(s: &UiState, leaving: bool) -> bool {
    let cfg = s.panel_config.as_ref().and_then(|c| c.get("screensaver"));
    let get = |k: &str| cfg.and_then(|c| c.get(k));
    let night = || s.states.get("sun.sun").map(|e| e.state == "below_horizon").unwrap_or(true);
    match get("when").and_then(Value::as_str).unwrap_or("dark") {
        "any" => true,
        "night" => night(),
        "window" => {
            let mins = |k: &str, d: i64| {
                get(k)
                    .and_then(Value::as_str)
                    .and_then(|t| {
                        let (h, m) = t.split_once(':')?;
                        Some(h.trim().parse::<i64>().ok()? * 60 + m.trim().parse::<i64>().ok()?)
                    })
                    .unwrap_or(d)
            };
            let (from, to) = (mins("from", 22 * 60), mins("to", 6 * 60));
            let t = pages::now();
            let now = pages::local_format(t, "%H").parse::<i64>().unwrap_or(0) * 60 + pages::local_format(t, "%M").parse::<i64>().unwrap_or(0);
            if from <= to {
                now >= from && now < to
            } else {
                now >= from || now < to
            }
        }
        _ => {
            let lux = get("light_entity")
                .and_then(Value::as_str)
                .and_then(|e| s.states.get(e))
                .and_then(|e| e.state.parse::<f64>().ok())
                .or(s.lux);
            let limit = get("lux").and_then(Value::as_f64).unwrap_or(DARK_LUX);
            match lux {
                Some(l) if leaving => l <= limit * 2.0 + 2.0,
                Some(l) => l < limit,
                None => night(),
            }
        }
    }
}

fn start_screensaver(ui: &AppWindow, s: &mut UiState) {
    s.screensaver = true;
    s.saver_auto = false;
    s.show_until = None;
    ui.set_set_view(0);
    ui.set_strip_on(false);
    let style = s.panel_config.as_ref().and_then(|c| c.get("saver_style")).and_then(Value::as_i64).unwrap_or(1) as i32;
    ui.set_saver_style(style);
    ui.set_screensaver_on(true);
}

/// Weight of a new once-a-minute reading in its average (about five minutes).
const REPORT_SMOOTHING: f64 = 0.2;

fn smooth(avg: Option<f64>, reading: Option<f64>) -> Option<f64> {
    match (avg, reading) {
        (Some(a), Some(r)) => Some(a + REPORT_SMOOTHING * (r - a)),
        (_, r) => r,
    }
}

/// The value to send when the average moved at least `step` from the one
/// sent last (rounded to `1 / per_unit`); None = keep the old one.
fn report_value(sent: Option<f64>, avg: Option<f64>, step: f64, per_unit: f64) -> Option<Option<f64>> {
    match (sent, avg) {
        (Some(s), Some(a)) if (a - s).abs() < step => None,
        (None, None) => None,
        (_, a) => Some(a.map(|v| (v * per_unit).round() / per_unit)),
    }
}

/// Copies what Home Assistant sees (ESPHome API) from the UI state.
fn report_device(s: &mut UiState) {
    let reporting = s.settings.reporting;
    let lux_changed = match (s.lux, s.last_reported_lux) {
        (Some(now), Some(before)) => (now - before).abs() > reporting.lux_step(before),
        (a, b) => a.is_some() != b.is_some(),
    };
    let periodic = s.last_report.elapsed() > Duration::from_secs(60);
    let mut d = s.device.lock().unwrap();
    let before = (
        d.theme.clone(), d.brightness, d.auto_brightness, d.screensaver_seconds, d.screensaver_style.clone(),
        d.proximity_wake, d.screen_on, d.dashboard.clone(), d.presence, d.display_mode.clone(), d.volume,
    );
    d.theme = s.settings.theme.clone();
    d.brightness = s.settings.brightness;
    d.auto_brightness = s.settings.auto_brightness;
    d.screensaver_seconds = s.settings.screensaver_seconds;
    d.screensaver_style = s.settings.screensaver_style.clone();
    d.proximity_wake = s.settings.proximity_wake;
    d.screen_on = !s.screensaver;
    d.dashboard = s.shared.dashboard.lock().unwrap().clone();
    d.presence = s.proximity.near();
    d.mic_muted = s.settings.mic_muted;
    d.display_mode = s.settings.display_mode.clone();
    d.volume = s.settings.volume;
    d.ready = true;
    let after = (
        d.theme.clone(), d.brightness, d.auto_brightness, d.screensaver_seconds, d.screensaver_style.clone(),
        d.proximity_wake, d.screen_on, d.dashboard.clone(), d.presence, d.display_mode.clone(), d.volume,
    );
    if lux_changed {
        d.lux = s.lux.map(|l| l.round());
        s.last_reported_lux = s.lux;
    }
    if periodic {
        s.soc_temp_avg = smooth(s.soc_temp_avg, hw::soc_temperature());
        s.wifi_avg = smooth(s.wifi_avg, hw::wifi_dbm());
        if let Some(v) = report_value(d.soc_temp, s.soc_temp_avg, reporting.soc_temperature_step, 10.0) {
            d.soc_temp = v;
        }
        if let Some(v) = report_value(d.wifi_dbm, s.wifi_avg, reporting.wifi_signal_step, 1.0) {
            d.wifi_dbm = v;
        }
        d.ip = s
            .config
            .as_ref()
            .and_then(|c| pairing::local_ip_towards(&c.ha_host_port()))
            .map(|ip| ip.to_string())
            .unwrap_or_default();
        s.last_report = Instant::now();
    }
    drop(d);
    if before != after || lux_changed || periodic {
        let _ = s.device_notify.send(());
    }
}

/// Auto theme thresholds: dark below, light above, unchanged in between.
const THEME_DARK_LUX: f64 = 10.0;
const THEME_LIGHT_LUX: f64 = 30.0;
/// How long the light must stay on the other side before the look changes
/// (no flipping at dusk or when a hand passes the sensor).
const THEME_SETTLE: Duration = Duration::from_secs(10);

/// Dark or light for the "auto" theme, by the light sensor. The first
/// decision is immediate, so the first screen already has the right look.
/// Without a sensor reading: Home Assistant's sun, otherwise unchanged (dark
/// at the start).
fn auto_dark(s: &mut UiState) -> bool {
    let Some(lux) = s.lux else {
        s.theme_pending = None;
        return match s.states.get("sun.sun") {
            Some(sun) => sun.state == "below_horizon",
            None => s.dark.unwrap_or(true),
        };
    };
    let wanted = if lux < THEME_DARK_LUX {
        Some(true)
    } else if lux > THEME_LIGHT_LUX {
        Some(false)
    } else {
        None
    };
    let Some(current) = s.dark else {
        return wanted.unwrap_or(lux < (THEME_DARK_LUX + THEME_LIGHT_LUX) / 2.0);
    };
    match wanted {
        Some(w) if w != current => match s.theme_pending {
            Some((p, since)) if p == w => {
                if since.elapsed() >= THEME_SETTLE {
                    s.theme_pending = None;
                    w
                } else {
                    current
                }
            }
            _ => {
                s.theme_pending = Some((w, Instant::now()));
                current
            }
        },
        _ => {
            s.theme_pending = None;
            current
        }
    }
}

fn update_theme(ui: &AppWindow, s: &mut UiState) {
    let dark = match s.settings.theme.as_str() {
        "light" => false,
        "auto" => auto_dark(s),
        _ => true,
    };
    ui.set_theme_mode(match s.settings.theme.as_str() {
        "light" => 0,
        "auto" => 2,
        _ => 1,
    });
    if s.dark != Some(dark) {
        s.dark = Some(dark);
        ui.global::<Theme>().set_dark(dark);
    }
}

/// Backlight fade, every 20 ms: towards the target in perceived brightness
/// (gamma 2.2), so a change looks even in the dark too, and with a time
/// constant instead of steps: about 0.8 s down, 0.25 s up, 0.12 s for a big
/// rise (waking from the screensaver).
fn fade_backlight() {
    with_state(|s| {
        const GAMMA: f64 = hw::BACKLIGHT_GAMMA;
        let dt: f64 = 0.02;
        let (now, target) = (s.brightness.clamp(0.0, 1000.0), s.brightness_target.clamp(0.0, 1000.0));
        let tau = if target > now + 300.0 {
            0.12
        } else if target > now {
            0.25
        } else {
            0.8
        };
        let p_now = (now / 1000.0).powf(1.0 / GAMMA);
        let p_target = (target / 1000.0).powf(1.0 / GAMMA);
        let p = p_now + (p_target - p_now) * (1.0 - (-dt / tau).exp());
        s.brightness = if (p - p_target).abs() < 0.002 { target } else { p.powf(GAMMA) * 1000.0 };
        s.backlight.set_permille(s.brightness.round() as u32);
    });
}

/// Web view: the page for the chosen dashboard, rendered only while it is
/// on the screen.
fn update_web(ui: &AppWindow, s: &mut UiState) {
    let Some(view) = s.web.clone() else { return };
    let Some(config) = s.config.as_ref() else { return };
    let dashboard = if !s.settings.web.dashboard.is_empty() {
        s.settings.web.dashboard.clone()
    } else {
        let own = s.shared.dashboard.lock().unwrap().clone();
        if own.is_empty() { "lovelace".to_string() } else { own }
    };
    let url = format!("{}/{}?external_auth=1", config.ha_url.trim_end_matches('/'), dashboard);
    // Before the page: the helper loads it once the resource filter is ready.
    view.set_skip(s.settings.web.skip_resources.clone());
    view.set_zoom(s.settings.web.zoom);
    {
        let mut d = s.device.lock().unwrap();
        if d.web_dashboard != dashboard {
            d.web_dashboard = dashboard.clone();
        }
    }
    view.load(&url, !s.settings.web.show_header, !s.settings.web.show_sidebar);
    let on_screen = ui.get_web_shown() && !s.screensaver && ui.get_set_view() == 0;
    view.set_visible(on_screen);
}

/// Runs twice a second: theme, screensaver, proximity wake, backlight target.
fn tick(ui: &AppWindow) {
    update_connection_screens(ui);
    with_state(|s| update_web(ui, s));
    let result = with_state(|s| {
        s.lux = s.light.lux();
        update_theme(ui, s);
        // Layer opened by a show request: close it when its time is up.
        if s.show_until.is_some_and(|t| Instant::now() >= t) {
            s.show_until = None;
            ui.set_strip_on(false);
            ui.set_dashboard_mode(s.settings.display_mode == "dashboard" && s.web.is_some());
        }
        let idle = platform::idle_time();
        // Proximity test view: on while the file exists. Its first line is
        // the integration time to test (a change of the line learns the
        // resting level anew), the rest a prompt shown above the reading.
        let content = std::fs::read_to_string(PROXIMITY_TEST_FILE).ok();
        let test = content.as_deref().map(|c| c.lines().next().unwrap_or("").trim().to_string());
        if let Some(c) = content.as_deref() {
            ui.set_prox_test_prompt(c.lines().skip(1).collect::<Vec<_>>().join("\n").into());
        }
        let mut test_started = false;
        if test != s.proximity_test {
            let integration = test.as_deref().and_then(|t| t.split_whitespace().next()).unwrap_or(hw::PROXIMITY_INTEGRATION);
            s.light.set_proximity_integration(integration);
            s.proximity = hw::ProximityWake::new(integration.parse().unwrap_or(0.01184));
            test_started = test.is_some() && s.proximity_test.is_none();
            ui.set_prox_test(test.is_some());
            match &test {
                Some(_) => eprintln!("nspanel-app: proximity test on, integration time {integration} s"),
                None => eprintln!("nspanel-app: proximity test off"),
            }
            s.proximity_test = test;
        }
        // Touch test view: on while its file exists.
        // (or started from the settings / the integration; its result stays a few seconds)
        if s.touch_close_at.is_some_and(|t| Instant::now() >= t) {
            s.touch_close_at = None;
        }
        // nobody finishing it: it closes after a minute and a half
        if s.touch_test_manual && s.touch_started.is_some_and(|t| t.elapsed() > Duration::from_secs(90)) {
            s.touch_test_manual = false;
        }
        let touch_test = std::path::Path::new(TOUCH_TEST_FILE).exists() || s.touch_test_manual || s.touch_close_at.is_some();
        if touch_test != s.touch_test {
            s.touch_test = touch_test;
            s.touch_test_step = 0;
            s.touch_test_down = None;
            ui.set_touch_test_step(0);
            ui.set_touch_test_text(touch_test_text(0).into());
            ui.set_touch_test(touch_test);
            eprintln!("nspanel-app: touch test {}", if touch_test { "on" } else { "off" });
        }
        // Screensaver (not while Home Assistant shows something)
        let timeout = s.settings.screensaver_seconds;
        // (and not while a timer runs: its chips stay in view)
        let showing = s.show_until.is_some()
            || s.proximity_test.is_some()
            || ui.get_voice_on()
            || s.ringing.is_some()
            || s.timers.values().any(|t| t.active)
            || !s.statuses.is_empty()
            || s.critical.is_some();
        if !s.screensaver
            && !showing
            && timeout > 0
            && ui.get_paired()
            && !ui.get_setup()
            && idle > Duration::from_secs(timeout as u64)
            && saver_window(s, false)
        {
            start_screensaver(ui, s);
            s.saver_auto = true;
        }
        // outside its window (a lamp switched on, the morning) the panel shows its pages again
        let saver_ends = s.screensaver && s.saver_auto && !saver_window(s, true);
        // Proximity: presence for Home Assistant, wake from the screensaver
        let mut wake_now = test_started && s.screensaver;
        if let Some(p) = s.light.proximity() {
            let approach = s.proximity.update(p);
            wake_now |= approach && s.screensaver && s.settings.proximity_wake;
            if s.proximity_test.is_some() {
                show_proximity_test(ui, s, p);
            }
        }
        // Backlight
        let target = if s.screensaver && (s.settings.screensaver_style == "black" || s.screen_off) && !wake_now {
            0.0
        } else {
            let normal = if s.settings.auto_brightness {
                s.lux.map(hw::auto_brightness_permille).unwrap_or(600) as f64
            } else {
                hw::manual_brightness_permille(s.settings.brightness) as f64
            };
            if s.screensaver && !wake_now { normal.min(150.0) } else { normal }
        };
        s.brightness_target = target;
        report_device(s);
        (wake_now || saver_ends, s.weather_refreshed.elapsed() > Duration::from_secs(60))
    });
    let (wake_now, refresh) = result;
    if wake_now {
        wake(ui);
    }
    if refresh {
        refresh_pages(ui);
    }
}

/// Developer tool: while this file exists the panel shows the proximity
/// reading and an estimated hand distance. First line: the integration time
/// in seconds (one of `in_proximity_integration_time_available`, empty = the
/// default), optionally followed by a word whose change restarts learning;
/// further lines: a prompt for the person at the panel.
const PROXIMITY_TEST_FILE: &str = "/tmp/nspanel-proximity-test";

/// Touch test (development): on while this file exists. Nine points, touched
/// one after the other (corners, edge middles, the middle held for 2 s); each
/// touch is logged with its distance from the point.
const TOUCH_TEST_FILE: &str = "/tmp/nspanel-touch-test";
const TOUCH_TEST_POINTS: [(f32, f32); 9] =
    [(16.0, 16.0), (240.0, 16.0), (464.0, 16.0), (464.0, 240.0), (464.0, 464.0), (240.0, 464.0), (16.0, 464.0), (16.0, 240.0), (240.0, 240.0)];
const TOUCH_TEST_HOLD: Duration = Duration::from_millis(1500);

fn touch_test_text(step: usize) -> String {
    match step {
        0..=7 => format!("Touch the red point ({} of 9)", step + 1),
        8 => "Hold the red point for 2 seconds (9 of 9)".into(),
        _ => with_state(|s| s.touch_result.clone()).unwrap_or_else(|| "Done, thank you".into()),
    }
}

/// Starts the touch calibration (the settings' Touch row, or the integration).
fn start_touch_test(ui: &AppWindow) {
    with_state(|s| {
        s.touch_test_manual = true;
        s.touch_offsets.clear();
        s.touch_result = None;
        s.touch_started = Some(Instant::now());
    });
    ui.set_set_view(0);
    wake(ui);
}

/// The nine points touched: a correction only when the offsets are systematic (all the same way,
/// more than 3 px on average, spread under 6 px); the test closes a few seconds later.
fn finish_touch_test(s: &mut UiState) {
    let n = s.touch_offsets.len().max(1) as f32;
    let (mx, my) = s.touch_offsets.iter().fold((0.0, 0.0), |a, o| (a.0 + o.0 / n, a.1 + o.1 / n));
    let spread = (s.touch_offsets.iter().map(|o| (o.0 - mx).powi(2) + (o.1 - my).powi(2)).sum::<f32>() / n).sqrt();
    let systematic = (mx * mx + my * my).sqrt() > 3.0 && spread < 6.0;
    let (ox, oy) = s.settings.touch_offset;
    let result = if systematic {
        s.settings.touch_offset = (ox + mx, oy + my);
        *platform::TOUCH_OFFSET.lock().unwrap() = s.settings.touch_offset;
        s.settings.save();
        format!("Corrected by {:+.0} px across, {:+.0} px down", mx, my)
    } else {
        "No correction needed".into()
    };
    eprintln!("nspanel-app: touch calibration: mean ({mx:+.1},{my:+.1}) spread {spread:.1} px → {result}");
    s.touch_result = Some(result);
    s.touch_test_manual = false;
    s.touch_close_at = Some(Instant::now() + Duration::from_secs(4));
}

fn show_proximity_test(ui: &AppWindow, s: &UiState, raw: f64) {
    let rest = s.proximity.rest().unwrap_or(raw);
    let signal = raw - rest;
    let margin = s.proximity.margin();
    let jitter = s.proximity.jitter();
    let integration = s.light.proximity_integration().unwrap_or(0.0);
    let distance = if signal > (3.0 * jitter).max(10.0) {
        hw::proximity_distance_cm(signal, integration).map(|d| format!("{d:.0} cm"))
    } else {
        None
    };
    ui.set_prox_test_distance(distance.unwrap_or_else(|| "–".into()).into());
    ui.set_prox_test_details(
        format!(
            "signal {signal:+.0}, threshold {margin:.0}\nraw {raw:.0}, rest {rest:.0}, jitter {jitter:.1}\nintegration {:.2} ms",
            integration * 1000.0
        )
        .into(),
    );
    ui.set_prox_test_near(s.proximity.near());
}

fn on_setting(ui: &AppWindow, key: &str, value: &str) {
    let mut reload = false;
    match key {
        "restart_app" => {
            eprintln!("nspanel-app: restart requested (settings / Home Assistant button)");
            std::process::exit(0)
        }
        "reboot" => {
            eprintln!("nspanel-app: reboot requested (settings / Home Assistant button)");
            let _ = std::process::Command::new("reboot").status();
            return;
        }
        "logout" => {
            if let Some(auth) = with_state(|s| s.auth.clone()) {
                auth.forget();
            }
            ui.set_set_view(0);
            send(Command::Reload);
            return;
        }
        _ => {}
    }
    with_state(|s| {
        match key {
            "theme" => s.settings.theme = value.to_string(),
            "auto_brightness" => s.settings.auto_brightness = value == "true",
            "brightness" => s.settings.brightness = value.parse::<u8>().unwrap_or(70).clamp(1, 100),
            "screensaver_index" => {
                if let Some((secs, _)) = value.parse::<usize>().ok().and_then(|i| settings::SCREENSAVER_CHOICES.get(i)) {
                    s.settings.screensaver_seconds = *secs;
                }
            }
            "screensaver_style" => s.settings.screensaver_style = value.to_string(),
            "proximity_wake" => s.settings.proximity_wake = value == "true",
            "display_mode" => {
                if matches!(value, "dashboard" | "assistant") {
                    s.settings.display_mode = value.to_string();
                }
            }
            "wake_words" => {
                s.settings.wake_words = value.split(',').filter(|v| !v.is_empty()).map(str::to_string).collect();
                *s.wake_words.lock().unwrap() = s.settings.wake_words.clone();
                s.wake_phrase = wake_phrase(&s.settings.wake_words);
            }
            "volume" => {
                s.settings.volume = value.parse::<u8>().unwrap_or(80).min(100);
                hw::set_volume(s.settings.volume);
                // Short sound at the new level (also for changes from Home Assistant).
                s.player.play(voice::Sound::Chime);
            }
            "mic_muted" => {
                s.settings.mic_muted = value == "true";
                s.mic_muted.store(s.settings.mic_muted, Ordering::SeqCst);
                ui.set_mic_muted(s.settings.mic_muted);
            }
            "dashboard_index" => {
                if let Some((url, _)) = value.parse::<usize>().ok().and_then(|i| s.dashboards.get(i)) {
                    s.settings.dashboard = url.clone();
                    *s.shared.dashboard.lock().unwrap() = url.clone();
                    reload = true;
                }
            }
            _ => eprintln!("nspanel-app: unknown setting {key}"),
        }
        s.settings.save();
    });
    apply_panel_config(ui);
    if reload {
        send(Command::Reload);
    }
}

/// Taps, holds and drags from the pages and layers (`act(what, value)`).
fn handle_act(ui: &AppWindow, what: &str, value: &str) {
    platform::note_activity();
    match what {
        "saver.wake" => wake(ui),
        "timers.stop" => stop_ringing(ui),
        w if w.starts_with("alarms.") || w.starts_with("alarm.") => alarms::act(ui, w, value),
        w if w.starts_with("timers.") || w.starts_with("watch.") => timers_act(ui, w, value),
        "voice.close" => ui.set_voice_on(false),
        "strip.close" => ui.set_strip_on(false),
        "status.fold" | "strip.open" | "info.crit-silence" => alerts::act(ui, what),
        "set.touch" => start_touch_test(ui),
        "shade.open" => shade::open(ui),
        "shade.peek" => shade::fill(ui),
        // A home widget's view: clock → timers, date / event → calendar,
        // weather now / curve / hours / days → weather, music → music.
        "home.tap" => {
            let kind = match value {
                "0" => "timers",
                "1" | "3" => "calendar",
                "2" | "4" | "5" | "6" => "weather",
                "7" => "music",
                _ => return,
            };
            if !go_to_page(ui, kind) {
                open_temporary_page(ui, kind);
            }
        }
        w if w.starts_with("set.") => shade::act(ui, w, value),
        w if w.starts_with("quick.") => quick::act(ui, w, value),
        w if w.starts_with("thermo.") => thermo::act(ui, w, value),
        w if w.starts_with("cam.") => cam::act(ui, w, value),
        w if w.starts_with("cal.") => cal::act(ui, w, value),
        w if w.starts_with("todo.") => todo::act(ui, w, value),
        w if w.starts_with("info.") => info::act(ui, w, value),
        "shade.close" => ui.set_set_view(0),
        "music.picker" | "music.picker-close" | "music.show" | "music.transfer" | "music.group" => mus::act(ui, what, value),
        "music.play" | "music.next" | "music.prev" | "music.shuffle" | "music.repeat" | "music.volume" | "music.seek" => {
            let entity = with_state(|s| media_entity(s));
            if entity.is_empty() {
                return;
            }
            match what {
                "music.play" => call("media_player", "media_play_pause", &entity, json!({})),
                "music.next" => call("media_player", "media_next_track", &entity, json!({})),
                "music.prev" => call("media_player", "media_previous_track", &entity, json!({})),
                "music.shuffle" => call("media_player", "shuffle_set", &entity, json!({"shuffle": !ui.get_music_shuffle()})),
                "music.repeat" => {
                    let next = match ui.get_music_repeat() {
                        0 => "all",
                        1 => "one",
                        _ => "off",
                    };
                    call("media_player", "repeat_set", &entity, json!({"repeat": next}))
                }
                "music.volume" => {
                    let v: f64 = value.parse().unwrap_or(0.5);
                    call("media_player", "volume_set", &entity, json!({"volume_level": (v.clamp(0.0, 1.0) * 100.0).round() / 100.0}))
                }
                "music.seek" => {
                    let v: f64 = value.parse().unwrap_or(0.0);
                    call("media_player", "media_seek", &entity, json!({"seek_position": v.max(0.0)}))
                }
                _ => {}
            }
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled yet)"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The GPU build runs with the web bundle's libraries (LD_LIBRARY_PATH from
    // the init script; the loader keeps its own copy for later dlopen calls).
    // Programs the app starts (wpa_cli, iw, the web helper) use their own.
    #[cfg(feature = "gpu")]
    std::env::remove_var("LD_LIBRARY_PATH");
    if std::env::var_os("TZ").is_none() {
        std::env::set_var("TZ", DEFAULT_TZ);
    }
    // SAFETY: tzset only re-reads the TZ variable.
    unsafe { tzset() };

    platform::install_screenshot_signal();

    let settings = Settings::load();
    *platform::TOUCH_OFFSET.lock().unwrap() = settings.touch_offset;
    let config = AppConfig::load();
    // mDNS: announces the ESPHome API, so Home Assistant finds the panel;
    // browses for Home Assistant when the setup screen asks for it.
    let mdns_browser = Arc::new(mdns::Browser::default());
    mdns::spawn(esphome::mdns_service(), mdns_browser.clone());
    let shared = Arc::new(ha::Shared::default());
    let dashboard_name = if !settings.dashboard.is_empty() {
        settings.dashboard.clone()
    } else {
        config.as_ref().map(|c| c.dashboard.clone()).unwrap_or_default()
    };
    *shared.dashboard.lock().unwrap() = dashboard_name;
    // The same entities and forecast as apply_panel_config() sets later, so the
    // first session needs no reload (the integration's settings come with it).
    *shared.extra_entities.lock().unwrap() = vec!["sun.sun".into(), "zone.home".into(), settings.weather_entity.clone()];
    *shared.forecast_entity.lock().unwrap() = settings.weather_entity.clone();
    let auth = config
        .as_ref()
        .ok()
        .map(|c| Arc::new(Auth::load(&c.ha_url, PathBuf::from(format!("{PERSIST_DIR}/ha_auth.json")))));

    // Home Assistant client first, the display after: the first screen shows
    // the right time and current states instead of 1970 and empty pages.
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let ha_early = match (&config, &auth) {
        (Ok(c), Some(a)) => {
            let (event_tx, event_rx) = mpsc::channel::<Event>();
            ha::spawn(c.ws_url(), shared.clone(), a.clone(), cmd_rx, move |event| {
                let _ = event_tx.send(event);
            });
            let buffered = wait_until_ready(&event_rx);
            Some((event_rx, buffered))
        }
        _ => None,
    };

    // The boot splash owns the display until now: it shows the finished logo,
    // gives up DRM master and keeps the picture until this app has drawn.
    let splash = splash_handoff();
    let display = Rc::new(RefCell::new(display::Display::open("/dev/dri/card0")?));
    // Display buffers the web helper renders into while the web view is all
    // there is on the screen.
    let web_buffers = display.borrow_mut().create_web_buffers(3);
    match web_buffers {
        Ok(buffers) => web::set_scanout_buffers(buffers),
        Err(e) => eprintln!("nspanel-app: no display buffers for the web view: {e}"),
    }
    let (width, height) = (display.borrow().width, display.borrow().height);
    let touch = match touch::Touch::open() {
        Ok(t) => {
            eprintln!("nspanel-app: touch input: {}", t.name);
            Some(t)
        }
        Err(e) => {
            eprintln!("nspanel-app: no touch input: {e}");
            None
        }
    };

    #[cfg(not(feature = "gpu"))]
    let window = {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::SwappedBuffers);
        window.set_size(slint::PhysicalSize::new(width, height));
        window
    };
    #[cfg(feature = "gpu")]
    let window = {
        let _ = (width, height);
        gpu::GpuWindow::new(Rc::clone(&display)).map_err(|e| format!("GPU renderer: {e}"))?
    };
    slint::platform::set_platform(Box::new(platform::PanelPlatform::new(Rc::clone(&window), display, touch)))
        .map_err(|e| format!("set_platform: {e:?}"))?;

    let ui = AppWindow::new()?;
    // Hand-over picture: the splash's last frame (same logo, same pixels),
    // then it fades over the UI.
    {
        // intro.rgba (tools/splash/storyboard.py): u16 x, y, w, h + RGBA.
        let raw: &[u8] = include_bytes!("../assets/intro.rgba");
        let rd = |i: usize| u16::from_le_bytes([raw[i], raw[i + 1]]) as u32;
        let (x, y, w, h) = (rd(0), rd(2), rd(4), rd(6));
        let mut pic = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
        pic.make_mut_bytes().copy_from_slice(&raw[8..8 + (w * h * 4) as usize]);
        ui.set_intro_logo(slint::Image::from_rgba8_premultiplied(pic));
        ui.set_intro_x(x as f32);
        ui.set_intro_y(y as f32);
    }
    // Loading screen background (dashboard mode).
    {
        let mut reader = png::Decoder::new(std::io::Cursor::new(&include_bytes!("../ui/images/loading.png")[..]))
            .read_info()
            .map_err(|e| format!("loading.png: {e}"))?;
        let mut rgb = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let info = reader.next_frame(&mut rgb).map_err(|e| format!("loading.png: {e}"))?;
        if info.color_type == png::ColorType::Rgb {
            let pic = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(&rgb[..info.buffer_size()], info.width, info.height);
            ui.set_loading_image(slint::Image::from_rgb8(pic));
        }
    }
    // The pictures the app draws itself: when new ones are ready, the pages that show them follow.
    {
        let weak = ui.as_weak();
        draw::start(move || {
            if let Some(ui) = weak.upgrade() {
                with_state(|s| {
                    todo::refresh(&ui, s);
                    info::refresh(&ui, s);
                    cal::refresh(&ui, s);
                    quick::refresh(&ui, s);
                });
            }
        });
    }
    // While the start-up picture still covers the screen, every page is drawn
    // once (one per frame), then the picture fades. At most 1.5 s.
    let intro = slint::Timer::default();
    let weak = ui.as_weak();
    let warm_started = Instant::now();
    let warm_next = std::cell::Cell::new(0i32);
    intro.start(slint::TimerMode::Repeated, Duration::from_millis(20), move || {
        let Some(ui) = weak.upgrade() else { return };
        let n = ui.get_pages().row_count() as i32;
        let k = warm_next.get();
        if k < n && warm_started.elapsed() < Duration::from_millis(1500) {
            ui.set_warm_page(if k == ui.get_page() { -1 } else { k });
            warm_next.set(k + 1);
            return;
        }
        if ui.get_intro() {
            ui.set_warm_page(-1);
            if warm_started.elapsed() >= Duration::from_millis(250) {
                ui.set_intro(false);
            }
        }
    });
    let intro_end = slint::Timer::default();
    let weak = ui.as_weak();
    // The picture goes once it has faded (500 ms after the warm-up).
    intro_end.start(slint::TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_intro() || !ui.get_intro_visible() {
            return;
        }
        thread_local!(static FADE_FROM: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) });
        let from = FADE_FROM.with(|f| match f.get() {
            Some(t) => t,
            None => {
                let t = Instant::now();
                f.set(Some(t));
                t
            }
        });
        if from.elapsed() < Duration::from_millis(650) {
            return;
        }
        ui.set_intro_visible(false);
        if let Some(pid) = splash {
            // SAFETY: plain kill(2).
            unsafe { libc::kill(pid, libc::SIGTERM) };
            let _ = std::fs::remove_file(SPLASH_PIDFILE);
        }
    });

    let phrase = wake_phrase(&settings.wake_words);
    let device = shared.device.clone();
    let (device_notify, device_rx) = mpsc::channel::<()>();
    let mic_muted = Arc::new(std::sync::atomic::AtomicBool::new(settings.mic_muted));
    let wake_words = Arc::new(std::sync::Mutex::new(settings.wake_words.clone()));
    let player = Arc::new(voice::Player::new());
    hw::set_volume(settings.volume);
    let ringing_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    ui.set_mic_muted(settings.mic_muted);
    ui.set_voice_assistant("".into());
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    STATE.with(|s| {
        *s.borrow_mut() = Some(UiState {
            config: config.as_ref().ok().cloned(),
            auth: auth.clone(),
            shared: shared.clone(),
            commands: None,
            settings,
            states: HashMap::new(),
            pairing_started: false,
            dashboards: Vec::new(),
            connected: false,
            ever_connected: false,
            offline_since: None,
            offline_text: String::new(),
            screensaver: false,
            saver_auto: false,
            statuses: Vec::new(),
            critical: None,
            alert_strip: false,
            backlight: hw::Backlight::open(),
            light: hw::LightSensor::open(),
            proximity: hw::ProximityWake::default(),
            proximity_test: None,
            brightness: 600.0,
            brightness_target: 600.0,
            lux: None,
            dark: None,
            theme_pending: None,
            device: device.clone(),
            device_notify,
            last_reported_lux: None,
            last_report: Instant::now() - Duration::from_secs(120),
            soc_temp_avg: None,
            wifi_avg: None,
            web: None,
            web_swipe: None,
            touch_test: false,
            touch_test_step: 0,
            touch_test_manual: false,
            touch_offsets: Vec::new(),
            touch_result: None,
            touch_close_at: None,
            touch_started: None,
            touch_test_down: None,
            web_pull: None,
            web_pulled: None,
            timers: HashMap::new(),
            timer_seq: 0,
            dismissed_timers: Default::default(),
            laps: Vec::new(),
            lap_at: 0.0,
            ringing: None,
            ringing_flag: ringing_flag.clone(),
            mic_muted: mic_muted.clone(),
            wake_words: wake_words.clone(),
            player: player.clone(),
            voice_hide_at: None,
            panel_config: None,
            forecast_hourly: Vec::new(),
            forecast_daily: Vec::new(),
            wake_phrase: phrase,
            wake_hint: Default::default(),
            quick_title: String::new(),
            art_url: None,
            art_tx: None,
            art: None,
            show_until: None,
            digits: Default::default(),
            prev_digits: Default::default(),
            flip_at: None,
            weather_refreshed: Instant::now(),
            dev_screens: false,
            dnd: false,
            dev_config: None,
            temp_page: None,
            alarm_edit: None,
            alarm_minute: 0,
            screen_off: false,
            weather_key: String::new(),
            dev_wx: None,
        })
    });
    // The look before the first frame (auto theme: the light sensor now).
    with_state(|s| {
        s.lux = s.light.lux();
        update_theme(&ui, s);
    });
    apply_panel_config(&ui);
    update_clock(&ui);

    // Voice satellite and ESPHome native API (settings, sensors, Assist).
    let (to_api_tx, to_api_rx) = mpsc::channel::<voice::ToApi>();
    let (from_api_tx, from_api_rx) = mpsc::channel::<voice::FromApi>();
    let link = esphome::VoiceLink { to_api: to_api_rx, from_api: from_api_tx, active_wake_words: wake_words.clone() };
    let weak = ui.as_weak();
    // Only Home Assistant (and app.json api_allow) may use the ESPHome API.
    let api_config = config.as_ref().ok().cloned();
    let api_peers = move || {
        api_config.as_ref().map(|c| api_link::allowed_peers(&c.ha_host_port(), &c.api_allow)).unwrap_or_default()
    };
    esphome::spawn(device, device_rx, link, api_peers, move |control| {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            apply_control(&ui, control);
        });
    });
    // NSPANEL_NO_VOICE: no microphone, wake words or player (measurements).
    if std::env::var_os("NSPANEL_NO_VOICE").is_none() {
        let weak = ui.as_weak();
        voice::spawn(
            to_api_tx,
            from_api_rx,
            player.clone(),
            voice::VoiceControl {
                muted: mic_muted.clone(),
                ringing: with_state(|s| s.ringing_flag.clone()),
                active_wake_words: wake_words.clone(),
                ha_url: with_state(|s| s.config.as_ref().map(|c| c.ha_url.clone()).unwrap_or_default()),
            },
            move |event| {
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        handle_voice_ui(&ui, event);
                    }
                });
            },
        );
        // Wi-Fi roam check only while nothing is heard, said or played.
        let speaking = player.busy.clone();
        let ringing = ringing_flag.clone();
        roam::spawn(move || !voice::busy() && !speaking.load(Ordering::SeqCst) && !ringing.load(Ordering::SeqCst));
    }

    // UI callbacks
    let weak = ui.as_weak();
    ui.on_act(move |what, value| {
        if let Some(ui) = weak.upgrade() {
            handle_act(&ui, what.as_str(), value.as_str());
        }
    });
    let weak = ui.as_weak();
    ui.global::<Act>().on_act(move |what, value| {
        if let Some(ui) = weak.upgrade() {
            handle_act(&ui, what.as_str(), value.as_str());
        }
    });
    let weak = ui.as_weak();
    ui.on_page_changed(move |now| {
        platform::note_activity();
        // Pages that fetch while on screen (cameras) start or stop.
        if let Some(ui) = weak.upgrade() {
            leave_temporary_page(&ui, now);
            with_state(|s| cam::refresh(&ui, s));
        }
    });
    let weak = ui.as_weak();
    ui.on_theme_mode_changed(move |m| {
        if let Some(ui) = weak.upgrade() {
            on_setting(&ui, "theme", match m {
                0 => "light",
                1 => "dark",
                _ => "auto",
            });
        }
    });

    // Timers: clock, sensors/backlight/screensaver, dev commands.
    let clock_timer = slint::Timer::default();
    let weak = ui.as_weak();
    clock_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(ui) = weak.upgrade() {
            update_clock(&ui);
        }
    });
    let ticker = slint::Timer::default();
    let weak = ui.as_weak();
    ticker.start(slint::TimerMode::Repeated, Duration::from_millis(500), move || {
        if let Some(ui) = weak.upgrade() {
            tick(&ui);
        }
    });
    let fader = slint::Timer::default();
    fader.start(slint::TimerMode::Repeated, Duration::from_millis(20), fade_backlight);
    let dev_timer = slint::Timer::default();
    let weak = ui.as_weak();
    dev_timer.start(slint::TimerMode::Repeated, Duration::from_millis(200), move || {
        let dev: Vec<String> = std::mem::take(&mut *platform::DEV_COMMANDS.lock().unwrap());
        for line in dev {
            let (Some(ui), Ok(v)) = (weak.upgrade(), serde_json::from_str::<Value>(&line)) else { continue };
            if let Some(show) = v.get("show") {
                handle_show(&ui, show);
            }
            if let Some(a) = v.get("act").and_then(Value::as_array) {
                handle_act(&ui, a.first().and_then(Value::as_str).unwrap_or(""), a.get(1).and_then(Value::as_str).unwrap_or(""));
            }
            if let Some(p) = v.get("page").and_then(Value::as_i64) {
                ui.set_page(p as i32);
            }
            // Development: the system screens on demand, frame statistics.
            if let Some(b) = v.get("connecting").and_then(Value::as_bool) {
                with_state(|s| s.dev_screens = true);
                ui.set_connecting(b);
            }
            if let Some(c) = v.get("offline").and_then(Value::as_i64) {
                with_state(|s| s.dev_screens = true);
                ui.set_offline_cause(c as i32);
            }
            if let Some(list) = v.get("pages").and_then(Value::as_array) {
                let slots: Vec<PageSlot> =
                    list.iter().filter_map(Value::as_str).map(|k| PageSlot { kind: k.into(), variant: 0, theme: -1, layout: -1 }).collect();
                ui.set_page(0);
                ui.set_pages(ModelRc::new(VecModel::from(slots)));
            }
            if let Some(o) = v.get("wx") {
                with_state(|s| s.dev_wx = o.as_object().map(|_| o.clone()));
                refresh_pages(&ui);
            }
            // Development: integration settings merged over the current ones
            // (e.g. {"config": {"pages": [...]}}), as if Home Assistant sent them.
            if let Some(c) = v.get("config").filter(|c| c.is_object()) {
                with_state(|s| {
                    let mut dev = s.dev_config.clone().unwrap_or_else(|| json!({}));
                    if let (Some(d), Some(n)) = (dev.as_object_mut(), c.as_object()) {
                        for (k, val) in n {
                            d.insert(k.clone(), val.clone());
                        }
                    }
                    s.dev_config = Some(dev.clone());
                    overlay(&mut s.panel_config, &dev);
                });
                apply_panel_config(&ui);
            }
            if v.get("dev_screens").and_then(Value::as_bool) == Some(false) {
                with_state(|s| s.dev_screens = false);
            }
            if let Some(b) = v.get("pair").and_then(Value::as_bool) {
                ui.set_paired(!b);
            }
            if let Some(b) = v.get("dark").and_then(Value::as_bool) {
                ui.global::<Theme>().set_dark(b);
            }
            #[cfg(feature = "gpu")]
            if let Some(b) = v.get("frame_stats").and_then(Value::as_bool) {
                gpu::FRAME_STATS.store(b, Ordering::SeqCst);
            }
            if let Some(a) = v.get("assist").and_then(Value::as_array) {
                ui.set_voice_heard(a.get(1).and_then(Value::as_str).unwrap_or("").into());
                ui.set_voice_answer(a.get(2).and_then(Value::as_str).unwrap_or("").into());
                let st = a.first().and_then(Value::as_i64).unwrap_or(0) as i32;
                ui.set_voice_state(st.max(1));
                ui.set_voice_on(st > 0);
            }
            // Measurements: another page in the web view, or a scripted scroll
            // of the page's largest scrolling element (no touch, no click).
            if let Some(url) = v.get("web_load").and_then(Value::as_str) {
                if let Some(view) = with_state(|s| s.web.clone()) {
                    view.dev(&format!("LOAD {url}"));
                }
            }
            if let Some(list) = v.get("web_skip").and_then(Value::as_array) {
                let paths = list.iter().filter_map(Value::as_str).map(str::to_string).collect();
                if let Some(view) = with_state(|s| s.web.clone()) {
                    view.set_skip(paths);
                }
            }
            if v.get("web_scroll_find").is_some() {
                if let Some(view) = with_state(|s| s.web.clone()) {
                    view.dev("SCROLLFIND");
                }
            }
            if let Some(a) = v.get("web_scroll").and_then(Value::as_array) {
                let px = a.first().and_then(Value::as_i64).unwrap_or(300);
                let ms = a.get(1).and_then(Value::as_u64).unwrap_or(400);
                if let Some(view) = with_state(|s| s.web.clone()) {
                    webstats::gesture("scripted scroll", webstats::now_us() + ms * 1000);
                    view.dev(&format!("SCROLLTEST {px} {ms}"));
                }
            }
        }
    });

    let (config, auth) = match (config, auth) {
        (Ok(c), Some(a)) => (c, a),
        (result, _) => {
            let e = result.err().unwrap_or_else(|| "no login store".into());
            eprintln!("nspanel-app: not configured ({e}): setup screen");
            ui.set_paired(true);
            start_setup(&ui, mdns_browser, &e);
            ui.run()?;
            return Ok(());
        }
    };
    ui.set_paired(auth.is_paired());
    ui.set_connecting(auth.is_paired());
    ui.set_pair_ha_url(config.ha_url.clone().into());

    with_state(|s| s.commands = Some(cmd_tx));

    // The Home Assistant client started before the display: hand its events
    // to the UI thread, the ones from the wait first, in order.
    if let Some((events, buffered)) = ha_early {
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            for event in buffered.into_iter().chain(events.iter()) {
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        handle_event(&ui, event);
                    }
                });
            }
        });
    }

    let weak = ui.as_weak();
    let art_tx = artwork::spawn(config.ha_url.clone(), auth.clone(), move |img| {
        // RGBA in this thread: the renderer takes it without converting.
        let img = img.map(|b| cam::rgba(&b));
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                with_state(|s| s.art = img.map(slint::Image::from_rgba8));
                refresh_pages(&ui);
            }
        });
    });
    with_state(|s| {
        s.art_tx = Some(art_tx);
        s.art_url = None;
    });
    refresh_pages(&ui);

    // NSPANEL_NO_WEB: no web view (measurements).
    if web::available() && std::env::var_os("NSPANEL_NO_WEB").is_none() {
        // Only the newest frame matters: one UI job at a time takes it.
        type Latest = Option<(slint::SharedPixelBuffer<slint::Rgba8Pixel>, webstats::FrameTimes)>;
        let latest: Arc<std::sync::Mutex<Latest>> = Arc::default();
        let weak = ui.as_weak();
        let view = web::WebView::start(
            auth.clone(),
            Box::new(move |frame, times| {
                if latest.lock().unwrap().replace((frame, times)).is_some() {
                    return;
                }
                let latest = latest.clone();
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let (Some(ui), Some((frame, times))) = (weak.upgrade(), latest.lock().unwrap().take()) {
                        webstats::frame_on_ui(times);
                        ui.set_web_frame(slint::Image::from_rgba8(frame));
                        ui.set_web_ready(true);
                    }
                });
            }),
        );
        // Loading screen: shown while a page loads, faded and removed when it is ready.
        let weak = ui.as_weak();
        view.on_loading(move |on| {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                ui.set_web_loading(on);
                if on {
                    // Off in the options: Home Assistant's own start-up shows instead.
                    if with_state(|s| s.settings.web.loading_screen) {
                        loading_text::start(&ui);
                        ui.set_web_overlay(true);
                    }
                } else {
                    loading_text::stop();
                    let weak = ui.as_weak();
                    slint::Timer::single_shot(Duration::from_millis(500), move || {
                        if let Some(ui) = weak.upgrade() {
                            if !ui.get_web_loading() {
                                ui.set_web_overlay(false);
                            }
                        }
                    });
                }
            });
        });
        let weak = ui.as_weak();
        let changed_view = view.clone();
        let release_view = view.clone();
        platform::set_web_direct(platform::WebDirect {
            // Straight to the screen only while nothing is drawn over the web view.
            allowed: Box::new(move || {
                let Some(ui) = weak.upgrade() else { return false };
                ui.get_web_shown()
                    && ui.get_web_ready()
                    && !(ui.get_web_overlay()
                        || ui.get_web_pull() > 0.0
                        || ui.get_touch_test()
                        || ui.get_set_view() > 0
                        || ui.get_screensaver_on()
                        || ui.get_strip_on()
                        || ui.get_voice_on()
                        || ui.get_timers_ringing()
                        || ui.get_setup()
                        || ui.get_prox_test()
                        || ui.get_intro_visible())
            }),
            changed: Box::new(move |on| changed_view.set_direct(on)),
            release: Box::new(move |list| {
                for i in list {
                    release_view.dev(&format!("FREE {i}"));
                }
            }),
        });
        with_state(|s| {
            s.web = Some(view);
            update_web(&ui, s);
        });
        ui.set_web_active(true);
        apply_panel_config(&ui);
    }
    let weak = ui.as_weak();
    ui.on_touch_test_event(move |kind, x, y| {
        let Some(ui) = weak.upgrade() else { return };
        platform::note_activity();
        let step = with_state(|s| {
            match kind {
                0 => s.touch_test_down = Some((Instant::now(), x, y, 0)),
                1 => {
                    if let Some(d) = s.touch_test_down.as_mut() {
                        d.3 += 1;
                    }
                }
                _ => {
                    let Some((at, x0, y0, moves)) = s.touch_test_down.take() else { return None };
                    let held = at.elapsed();
                    let i = s.touch_test_step.min(8);
                    let (tx, ty) = TOUCH_TEST_POINTS[i];
                    let (dx, dy) = (x0 - tx, y0 - ty);
                    eprintln!(
                        "nspanel-app: touch test: point {} ({tx:.0},{ty:.0}) down ({x0:.0},{y0:.0}) up ({x:.0},{y:.0}) off ({dx:+.0},{dy:+.0}) {:.0} px, held {} ms, {moves} moves",
                        i + 1,
                        (dx * dx + dy * dy).sqrt(),
                        held.as_millis()
                    );
                    if s.touch_test_step < 8 || (s.touch_test_step == 8 && held >= TOUCH_TEST_HOLD) {
                        s.touch_test_step += 1;
                        // measured with the correction in place: the offset left over adds to it
                        s.touch_offsets.push((dx, dy));
                    }
                    if s.touch_test_step == 9 && s.touch_test_manual {
                        finish_touch_test(s);
                    }
                    return Some(s.touch_test_step);
                }
            }
            None
        });
        if let Some(step) = step {
            ui.set_touch_test_step(step as i32);
            ui.set_touch_test_text(touch_test_text(step).into());
        }
    });

    let weak = ui.as_weak();
    ui.on_web_touch(move |kind, x, y| {
        let Some(ui) = weak.upgrade() else { return };
        // Inside with_state only the decision; UI callbacks run after it.
        // The top edge (24 px) belongs to the quick settings swipe and never
        // reaches the page; a pull down from below it, on content scrolled to
        // its top, reloads the page (pull to refresh).
        let (swiped, pulled) = with_state(|s| {
            let Some(view) = s.web.clone() else { return (false, None) };
            match kind {
                0 if y < 24.0 => s.web_swipe = Some(y),
                1 if s.web_swipe.is_some() => {
                    if y - s.web_swipe.unwrap_or(y) > 60.0 {
                        s.web_swipe = None;
                        return (true, None);
                    }
                }
                2 if s.web_swipe.is_some() => s.web_swipe = None,
                0 => {
                    view.touch('d', x, y);
                    view.check_top(x, y);
                    s.web_pull = Some((x, y));
                    s.web_pulled = None;
                }
                1 => {
                    view.touch('m', x, y);
                    if let Some((x0, y0)) = s.web_pull {
                        let (dx, dy) = ((x - x0).abs(), y - y0);
                        if s.web_pulled.is_none() && dy > PULL_START && dy > 2.0 * dx && view.at_top() == Some(true) {
                            s.web_pulled = Some(0.0);
                        } else if s.web_pulled.is_none() && (dy < -PULL_START || dx > PULL_START) {
                            // Scrolling or sideways: not a pull.
                            s.web_pull = None;
                        }
                        if s.web_pulled.is_some() {
                            s.web_pulled = Some(((dy - PULL_START) / PULL_DISTANCE).clamp(0.0, 1.0));
                        }
                    }
                    return (false, s.web_pulled);
                }
                _ => {
                    view.touch('u', x, y);
                    if s.web_pulled.is_some_and(|p| p >= 1.0) {
                        view.reload();
                    }
                    s.web_pull = None;
                    s.web_pulled = None;
                    return (false, Some(0.0));
                }
            }
            (false, None)
        });
        if let Some(p) = pulled {
            ui.set_web_pull(p);
        }
        if swiped {
            shade::open(&ui);
        }
    });

    let _ = VERSION;
    ui.run()?;
    Ok(())
}
