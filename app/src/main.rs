//! NSPanel Pro 86 panel app: draws a Home Assistant dashboard natively
//! (Slint software renderer straight to DRM/KMS, touch from evdev).
//! Login by QR code (OAuth), live states over the WebSocket API, details
//! layers, settings, screensaver, automatic brightness and theme; assistant
//! mode (home screen, full-screen conversation, layers opened from Home
//! Assistant through the nspanel_pro integration).

mod artwork;
mod assistant;
mod auth;
mod camera;
mod config;
mod dashboard;
mod display;
mod esphome;
mod ha;
mod hw;
mod icons;
mod pairing;
mod platform;
mod settings;
mod touch;
mod voice;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use auth::Auth;
use camera::CameraControl;
use config::{AppConfig, PERSIST_DIR};
use dashboard::RowSpec;
use ha::{Command, EntityState, Event};
use settings::{Settings, SCREENSAVER_CHOICES};

slint::include_modules!();

/// Budapest time as a POSIX TZ rule (works without zoneinfo files).
const DEFAULT_TZ: &str = "CET-1CEST,M3.5.0,M10.5.0/3";
const VERSION: &str = env!("CARGO_PKG_VERSION");

extern "C" {
    // POSIX; not exported by the libc crate for this target.
    fn tzset();
}

struct UiState {
    config: Option<AppConfig>,
    auth: Option<Arc<Auth>>,
    shared: Arc<ha::Shared>,
    commands: Option<Sender<Command>>,
    settings: Settings,
    dash_config: Value,
    views: Vec<String>,
    view_index: usize,
    rows: Vec<RowSpec>,
    states: HashMap<String, EntityState>,
    model: Rc<VecModel<RowData>>,
    camera_row: Option<usize>,
    pairing_started: bool,
    more_info: Option<(String, Option<String>)>,
    dashboards: Vec<(String, String)>,
    connected: bool,
    screensaver: bool,
    backlight: hw::Backlight,
    light: hw::LightSensor,
    proximity: hw::ProximityWake,
    brightness: f64,
    lux: Option<f64>,
    dark: Option<bool>,
    started: Instant,
    device: Arc<std::sync::Mutex<esphome::DeviceState>>,
    device_notify: Sender<()>,
    last_reported_lux: Option<f64>,
    last_report: Instant,
    timers: HashMap<String, TimerInfo>,
    ringing: Option<Instant>,
    /// Same as `ringing.is_some()`, for the voice thread.
    ringing_flag: Arc<std::sync::atomic::AtomicBool>,
    mic_muted: Arc<std::sync::atomic::AtomicBool>,
    wake_words: Arc<std::sync::Mutex<Vec<String>>>,
    player: Arc<voice::Player>,
    assist_hide_at: Option<Instant>,
    // Assistant mode
    forecast_hourly: Vec<Value>,
    forecast_daily: Vec<Value>,
    assistant_rows: Vec<RowSpec>,
    assistant_model: Rc<VecModel<RowData>>,
    wake_phrase: String,
    art_url: Option<String>,
    art_tx: Option<Sender<Option<String>>>,
    /// When the layer opened by a show request closes by itself.
    show_until: Option<Instant>,
    /// Camera shown full screen by a show request (instead of the dashboard's).
    camera_override: Option<String>,
    assistant_reset: i32,
    /// Forecast labels depend on the clock (next hours, "Today"; the clock
    /// may still be wrong when the first forecast arrives before NTP).
    assistant_refreshed: Instant,
}

struct TimerInfo {
    name: String,
    active: bool,
    end: Instant,
    left: u32,
}

thread_local! {
    static STATE: RefCell<Option<UiState>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut UiState) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("state")))
}

fn local_time_strings() -> (String, String, String) {
    // SAFETY: time/localtime_r/strftime on local buffers.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        let fmt = |pattern: &str| {
            let mut buf = [0u8; 64];
            let pattern = std::ffi::CString::new(pattern).unwrap();
            let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), pattern.as_ptr(), &tm);
            String::from_utf8_lossy(&buf[..n]).into_owned()
        };
        (fmt("%H:%M"), fmt("%A, %Y-%m-%d"), fmt("%A, %e %B").split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

fn update_clock(ui: &MainWindow) {
    let (time, date, long_date) = local_time_strings();
    ui.set_time(time.into());
    ui.set_date(date.into());
    ui.set_long_date(long_date.into());
    let beep = STATE.with(|s| {
        let mut guard = s.borrow_mut();
        let Some(s) = guard.as_mut() else { return None };
        // Timers: nearest one in the top bar.
        let mut left: Vec<u32> = s
            .timers
            .values()
            .map(|t| if t.active { t.end.saturating_duration_since(Instant::now()).as_secs() as u32 } else { t.left })
            .collect();
        left.sort();
        let text = match left.first() {
            Some(l) => {
                let base = if *l >= 3600 {
                    format!("{}:{:02}:{:02}", l / 3600, (l % 3600) / 60, l % 60)
                } else {
                    format!("{}:{:02}", l / 60, l % 60)
                };
                if left.len() > 1 { format!("{base} +{}", left.len() - 1) } else { base }
            }
            None => String::new(),
        };
        ui.set_timer_text(text.into());
        // Hide the conversation panel a few seconds after the end.
        if s.assist_hide_at.is_some_and(|t| Instant::now() >= t) {
            s.assist_hide_at = None;
            ui.set_assist_state(0);
        }
        // Ringing timer: beep until stopped, at most a minute.
        match s.ringing {
            Some(started) if started.elapsed() > Duration::from_secs(60) => {
                s.ringing = None;
                s.ringing_flag.store(false, Ordering::SeqCst);
                ui.set_timer_ringing(false);
                None
            }
            Some(_) => Some(s.player.clone()),
            None => None,
        }
    });
    if let Some(player) = beep {
        player.play(voice::Sound::Beep);
    }
}

fn handle_timer(ui: &MainWindow, kind: i32, id: String, name: String, left: u32, active: bool) {
    let finished = with_state(|s| match kind {
        // started, updated
        0 | 1 => {
            s.timers.insert(id, TimerInfo { name, active, end: Instant::now() + Duration::from_secs(left as u64), left });
            false
        }
        // cancelled
        2 => {
            s.timers.remove(&id);
            false
        }
        // finished
        3 => {
            let name = s.timers.remove(&id).map(|t| t.name).unwrap_or(name);
            s.ringing = Some(Instant::now());
            s.ringing_flag.store(true, Ordering::SeqCst);
            ui.set_timer_name(name.into());
            ui.set_timer_ringing(true);
            true
        }
        _ => false,
    });
    if finished {
        wake(ui);
    }
}

fn handle_voice_ui(ui: &MainWindow, event: voice::VoiceUi) {
    use voice::VoiceUi;
    let hide_after = |secs: u64| with_state(|s| s.assist_hide_at = Some(Instant::now() + Duration::from_secs(secs)));
    match event {
        VoiceUi::Listening => {
            wake(ui);
            with_state(|s| s.assist_hide_at = None);
            ui.set_assist_heard("".into());
            ui.set_assist_answer("".into());
            ui.set_assist_state(1);
        }
        VoiceUi::Heard(text) => ui.set_assist_heard(text.into()),
        VoiceUi::Thinking => ui.set_assist_state(2),
        VoiceUi::Answer(text) => {
            ui.set_assist_answer(text.into());
            ui.set_assist_state(3);
        }
        VoiceUi::Idle => {
            if ui.get_assist_state() > 0 {
                hide_after(5);
            }
        }
        VoiceUi::Error(message) => {
            ui.set_assist_answer(message.into());
            ui.set_assist_state(4);
            hide_after(5);
        }
        VoiceUi::Stopped => {
            with_state(|s| {
                s.ringing = None;
                s.ringing_flag.store(false, Ordering::SeqCst);
            });
            ui.set_timer_ringing(false);
            if ui.get_assist_state() > 0 {
                hide_after(1);
            }
        }
    }
    platform::note_activity();
}

const SPLASH_PIDFILE: &str = "/var/run/nspanel-splash.pid";

const SPLASH_DONE: &str = "/var/run/nspanel-splash.done";

/// Asks the boot splash (nspanel-splash) to hand the display over and waits
/// until it has: it first finishes its sequence and keeps the last picture
/// for a moment (the UI must not cut the animation short), then waits
/// for Wi-Fi (the panel is no use without the network), at most 30 s
/// after its start. Its pid.
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

fn wake_phrase(ids: &[String]) -> String {
    let Some(id) = ids.first() else { return String::new() };
    voice::available_wake_words().into_iter().find(|w| w.id == *id).map(|w| w.phrase).unwrap_or_default()
}

/// Assistant home screen contents and the media artwork.
fn refresh_assistant(ui: &MainWindow) {
    let (data, art) = with_state(|s| {
        let c = assistant::effective(&s.settings);
        let d = assistant::data(
            &c,
            &s.states,
            &s.forecast_hourly,
            &s.forecast_daily,
            &s.wake_phrase,
            s.settings.mic_muted,
            s.connected,
        );
        let url = assistant::artwork_url(&c, &s.states);
        let art = if url != s.art_url {
            s.art_url = url.clone();
            s.art_tx.clone().map(|tx| (tx, url))
        } else {
            None
        };
        (d, art)
    });
    ui.set_assistant(data);
    if let Some((tx, url)) = art {
        let _ = tx.send(url);
    }
}

/// Applies the display mode and the assistant configuration (tiles,
/// followed entities, forecasts); reconnects if Home Assistant has to send
/// other entities.
fn apply_assistant_config(ui: &MainWindow) {
    let reload = with_state(|s| {
        let c = assistant::effective(&s.settings);
        s.assistant_rows = assistant::tile_rows(&c);
        let data: Vec<RowData> = s.assistant_rows.iter().map(|r| dashboard::row_data(r, &s.states)).collect();
        s.assistant_model.set_vec(data);
        let assistant_mode = s.settings.display_mode == "assistant";
        ui.set_display_mode(if assistant_mode { 1 } else { 0 });
        ui.set_assist_full(assistant_mode && c.show_conversation);
        let mut changed = false;
        let extra = assistant::extra_entities(&s.settings);
        {
            let mut e = s.shared.extra_entities.lock().unwrap();
            if *e != extra {
                *e = extra;
                changed = true;
            }
        }
        {
            let mut f = s.shared.forecast_entity.lock().unwrap();
            if *f != c.weather_entity {
                *f = c.weather_entity.clone();
                s.forecast_hourly.clear();
                s.forecast_daily.clear();
                changed = true;
            }
        }
        changed
    });
    refresh_assistant(ui);
    if reload {
        send(Command::Reload);
    }
}

/// show_view / show_message from Home Assistant (nspanel_pro integration).
fn handle_show(ui: &MainWindow, v: &Value) {
    let kind = v["type"].as_str().unwrap_or("view");
    let seconds = v["seconds"].as_u64().unwrap_or(30);
    eprintln!("nspanel-app: show request {v}");
    wake(ui);
    ui.set_message_open(false);
    ui.set_weather_open(false);
    ui.set_media_open(false);
    ui.set_more_info_open(false);
    ui.set_settings_open(false);
    let chime = with_state(|s| {
        s.more_info = None;
        if s.camera_override.is_some() {
            ui.set_camera_full(false);
        }
        match kind {
            "message" => {
                ui.set_message_title(v["title"].as_str().unwrap_or("").into());
                ui.set_message_text(v["message"].as_str().unwrap_or("").into());
                let icon = v["icon"].as_str().unwrap_or("").trim_start_matches("mdi:");
                ui.set_message_icon(icons::glyph(icon).map(String::from).unwrap_or_default().into());
                ui.set_message_open(true);
            }
            _ => match v["view"].as_str().unwrap_or("home") {
                "dashboard" => {
                    if s.settings.display_mode == "assistant" {
                        ui.set_show_dashboard(true);
                    }
                }
                "weather" => ui.set_weather_open(true),
                "media" => ui.set_media_open(true),
                "camera" => {
                    if let Some(entity) = v["entity_id"].as_str().filter(|e| e.starts_with("camera.")) {
                        s.camera_override = Some(entity.to_string());
                        ui.set_camera_full(true);
                    }
                }
                _ => {
                    ui.set_show_dashboard(false);
                    ui.set_camera_full(false);
                    s.assistant_reset += 1;
                    ui.set_assistant_reset(s.assistant_reset);
                }
            },
        }
        s.show_until = (seconds > 0).then(|| Instant::now() + Duration::from_secs(seconds));
        v["chime"].as_bool().unwrap_or(false).then(|| s.player.clone())
    });
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

fn start_pairing(ui: &MainWindow) {
    let (config, auth) = with_state(|s| (s.config.clone(), s.auth.clone()));
    let (Some(config), Some(auth)) = (config, auth) else { return };
    let weak = ui.as_weak();
    let on_paired = move || {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_paired(true);
                ui.set_status("Connecting to Home Assistant…".into());
            }
        });
    };
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
        Err(e) => ui.set_pair_hint(format!("Cannot start pairing: {e}").into()),
    }
}

/// Shows the current view's rows.
fn apply_view(ui: &MainWindow, camera: &CameraControl) {
    with_state(|s| {
        let rows = dashboard::build_rows(&s.dash_config, s.view_index);
        s.camera_row = rows.iter().position(|r| r.kind == dashboard::ROW_CAMERA);
        *camera.entity.lock().unwrap() = s.camera_row.and_then(|i| rows[i].a.as_ref()).map(|c| c.entity.clone());
        let data: Vec<RowData> = rows.iter().map(|r| dashboard::row_data(r, &s.states)).collect();
        s.model.set_vec(data);
        s.rows = rows;
        let title = s.views.get(s.view_index).cloned().unwrap_or_default();
        ui.set_views(ModelRc::new(VecModel::from(s.views.iter().map(SharedString::from).collect::<Vec<_>>())));
        ui.set_view_index(s.view_index as i32);
        if s.connected {
            ui.set_header_info(title.into());
        }
    });
    ui.invoke_scroll_top();
}

fn refresh_more_info(ui: &MainWindow) {
    with_state(|s| {
        if let Some((entity, name)) = &s.more_info {
            ui.set_more_info(dashboard::more_info(entity, name.as_deref(), &s.states));
        }
    });
}

fn open_more_info(ui: &MainWindow, entity: &str) {
    with_state(|s| {
        let name = s
            .rows
            .iter()
            .flat_map(|r| [r.a.as_ref(), r.b.as_ref()])
            .flatten()
            .find(|c| c.entity == entity)
            .and_then(|c| c.name.clone());
        s.more_info = Some((entity.to_string(), name));
    });
    refresh_more_info(ui);
    ui.set_more_info_open(true);
}

fn about_text(s: &UiState) -> String {
    let ip = s
        .config
        .as_ref()
        .and_then(|c| pairing::local_ip_towards(&c.ha_host_port()))
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "–".into());
    let uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|u| u.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .map(|secs| format!("{}h {}m", (secs / 3600.0) as u64, ((secs % 3600.0) / 60.0) as u64))
        .unwrap_or_default();
    let mem = std::fs::read_to_string("/proc/meminfo").ok().map(|m| {
        let get = |k: &str| {
            m.lines().find(|l| l.starts_with(k)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok())
        };
        match (get("MemTotal:"), get("MemAvailable:")) {
            (Some(t), Some(a)) => format!("{} MB used of {} MB", (t - a) / 1024, t / 1024),
            _ => String::new(),
        }
    });
    let temp = hw::soc_temperature().map(|t| format!("{t:.0} °C")).unwrap_or_default();
    let lux = s.lux.map(|l| format!("{l:.0} lx")).unwrap_or_else(|| "–".into());
    format!(
        "App {VERSION} · IP {ip}\nHome Assistant: {} ({})\nUptime {uptime} · {} · SoC {temp} · light {lux}",
        s.config.as_ref().map(|c| c.ha_url.as_str()).unwrap_or("not configured"),
        if s.connected { "connected" } else { "not connected" },
        mem.unwrap_or_default()
    )
}

fn settings_data(s: &UiState) -> SettingsData {
    let current = &s.settings.dashboard;
    let effective = if current.is_empty() {
        s.config.as_ref().map(|c| c.dashboard.clone()).unwrap_or_default()
    } else {
        current.clone()
    };
    SettingsData {
        theme_index: match s.settings.theme.as_str() {
            "light" => 1,
            "auto" => 2,
            _ => 0,
        },
        brightness: s.settings.brightness as f32 / 100.0,
        auto_brightness: s.settings.auto_brightness,
        screensaver_index: s.settings.screensaver_index(),
        style_index: if s.settings.screensaver_style == "black" { 1 } else { 0 },
        proximity: s.settings.proximity_wake,
        dashboards: ModelRc::new(VecModel::from(
            s.dashboards.iter().map(|(_, t)| SharedString::from(t.as_str())).collect::<Vec<_>>(),
        )),
        dashboard_index: s.dashboards.iter().position(|(u, _)| *u == effective).map(|i| i as i32).unwrap_or(-1),
        display_mode: if s.settings.display_mode == "assistant" { 1 } else { 0 },
        volume: s.settings.volume as f32 / 100.0,
        about: about_text(s).into(),
    }
}

fn refresh_settings(ui: &MainWindow) {
    let data = with_state(|s| settings_data(s));
    ui.set_settings(data);
}

fn handle_event(ui: &MainWindow, event: Event, camera: &CameraControl) {
    match event {
        Event::Status { text, connected } => {
            ui.set_connected(connected);
            ui.set_status(text.clone().into());
            let title = with_state(|s| {
                s.connected = connected;
                s.views.get(s.view_index).cloned().unwrap_or_default()
            });
            ui.set_header_info(if connected { title.into() } else { text.into() });
            refresh_assistant(ui);
        }
        Event::NeedPairing => {
            ui.set_paired(false);
            ui.set_connected(false);
            ui.set_header_info("Waiting for login".into());
            let start = with_state(|s| {
                let first = !s.pairing_started;
                s.pairing_started = true;
                first
            });
            if start {
                start_pairing(ui);
            }
        }
        Event::Dashboard(config) => {
            ui.set_paired(true);
            let need_list = with_state(|s| {
                s.views = dashboard::view_titles(&config);
                if s.view_index >= s.views.len() {
                    s.view_index = 0;
                }
                s.dash_config = config;
                s.dashboards.is_empty()
            });
            apply_view(ui, camera);
            if need_list {
                // Also gives the ESPHome "Dashboard" select its options.
                send(Command::ListDashboards);
            }
        }
        Event::States(changes) => {
            let refresh_details = with_state(|s| {
                for (id, st) in &changes {
                    match st {
                        Some(st) => {
                            s.states.insert(id.clone(), st.clone());
                        }
                        None => {
                            s.states.remove(id);
                        }
                    }
                }
                for (i, row) in s.rows.iter().enumerate() {
                    let touches =
                        |c: &Option<dashboard::CardRef>| c.as_ref().is_some_and(|c| changes.iter().any(|(id, _)| *id == c.entity));
                    if (touches(&row.a) || touches(&row.b)) && i < s.model.row_count() {
                        s.model.set_row_data(i, dashboard::row_data(row, &s.states));
                    }
                }
                for (i, row) in s.assistant_rows.iter().enumerate() {
                    let touches =
                        |c: &Option<dashboard::CardRef>| c.as_ref().is_some_and(|c| changes.iter().any(|(id, _)| *id == c.entity));
                    if (touches(&row.a) || touches(&row.b)) && i < s.assistant_model.row_count() {
                        s.assistant_model.set_row_data(i, dashboard::row_data(row, &s.states));
                    }
                }
                let c = assistant::effective(&s.settings);
                let home = changes.iter().any(|(id, _)| *id == c.weather_entity || *id == c.media_player_entity);
                (s.more_info.as_ref().is_some_and(|(e, _)| changes.iter().any(|(id, _)| id == e)), home)
            });
            let (refresh_details, refresh_home) = refresh_details;
            if refresh_details {
                refresh_more_info(ui);
            }
            if refresh_home {
                refresh_assistant(ui);
            }
        }
        Event::Dashboards(list) => {
            with_state(|s| {
                s.device.lock().unwrap().dashboards = list.clone();
                let _ = s.device_notify.send(());
                s.dashboards = list;
            });
            refresh_settings(ui);
        }
        Event::Panel(config) => {
            let changed = with_state(|s| {
                let new = match &config {
                    Some(v) => assistant::from_integration(v),
                    None => settings::AssistantConfig::default(),
                };
                if new != s.settings.assistant {
                    eprintln!("nspanel-app: assistant mode settings from Home Assistant: {new:?}");
                    s.settings.assistant = new;
                    s.settings.save();
                    true
                } else {
                    false
                }
            });
            if changed {
                apply_assistant_config(ui);
            }
        }
        Event::Show(v) => handle_show(ui, &v),
        Event::Forecast { hourly, items } => {
            with_state(|s| if hourly { s.forecast_hourly = items } else { s.forecast_daily = items });
            refresh_assistant(ui);
        }
    }
}

/// Maps a button on the panel to a Home Assistant service call.
fn command_for(entity: &str, action: &str, states: &HashMap<String, EntityState>) -> Option<Command> {
    let domain = entity.split('.').next().unwrap_or("").to_string();
    let st = states.get(entity);
    let attr = |k: &str| st.and_then(|s| s.attrs.get(k)).and_then(Value::as_f64);
    let call = |domain: &str, service: &str, data: Value| Command::CallService {
        domain: domain.into(),
        service: service.into(),
        entity_id: entity.into(),
        data,
    };
    Some(match (domain.as_str(), action) {
        ("cover", "open") => call("cover", "open_cover", json!({})),
        ("cover", "close") => call("cover", "close_cover", json!({})),
        ("cover", "stop") => call("cover", "stop_cover", json!({})),
        ("climate", "temp_up") | ("climate", "temp_down") => {
            let step = attr("target_temp_step").unwrap_or(0.5);
            let current = attr("temperature")?;
            let delta = if action == "temp_up" { step } else { -step };
            let min = attr("min_temp").unwrap_or(7.0);
            let max = attr("max_temp").unwrap_or(35.0);
            let target = ((current + delta) / step).round() * step;
            call("climate", "set_temperature", json!({"temperature": target.clamp(min, max)}))
        }
        ("climate", "mode_toggle") | ("climate", "toggle") => {
            let state = st.map(|s| s.state.as_str()).unwrap_or("off");
            let modes = st.map(|s| dashboard::attr_list(s, "hvac_modes")).unwrap_or_default();
            let next = if state == "off" {
                if modes.iter().any(|m| m == "heat") { "heat".to_string() } else { modes.into_iter().find(|m| m != "off")? }
            } else {
                "off".to_string()
            };
            call("climate", "set_hvac_mode", json!({"hvac_mode": next}))
        }
        ("media_player", "play_pause") => call("media_player", "media_play_pause", json!({})),
        ("media_player", "next") => call("media_player", "media_next_track", json!({})),
        ("media_player", "prev") => call("media_player", "media_previous_track", json!({})),
        ("media_player", "vol_up") | ("media_player", "vol_down") => {
            let v = attr("volume_level").unwrap_or(0.0);
            let delta = if action == "vol_up" { 0.05 } else { -0.05 };
            call("media_player", "volume_set", json!({"volume_level": (((v + delta) * 100.0).round() / 100.0).clamp(0.0, 1.0)}))
        }
        (_, "toggle") => {
            let service_domain = match domain.as_str() {
                "light" | "switch" | "fan" | "input_boolean" | "media_player" | "cover" | "automation" => domain.as_str(),
                _ => "homeassistant",
            };
            call(service_domain, "toggle", json!({}))
        }
        _ => return None,
    })
}

fn slider_command(entity: &str, what: &str, v: f32, states: &HashMap<String, EntityState>) -> Option<Command> {
    let st = states.get(entity)?;
    let v = v.clamp(0.0, 1.0) as f64;
    let call = |domain: &str, service: &str, data: Value| Command::CallService {
        domain: domain.into(),
        service: service.into(),
        entity_id: entity.into(),
        data,
    };
    Some(match what {
        "brightness" if v < 0.01 => call("light", "turn_off", json!({})),
        "brightness" => call("light", "turn_on", json!({"brightness_pct": (v * 100.0).round().max(1.0)})),
        "ct" => {
            let min = dashboard::attr_f64(st, "min_color_temp_kelvin").unwrap_or(2000.0);
            let max = dashboard::attr_f64(st, "max_color_temp_kelvin").unwrap_or(6500.0);
            call("light", "turn_on", json!({"color_temp_kelvin": (min + v * (max - min)).round()}))
        }
        "hue" => call("light", "turn_on", json!({"hs_color": [(v * 360.0).round(), 100]})),
        "position" => call("cover", "set_cover_position", json!({"position": (v * 100.0).round()})),
        "volume" => call("media_player", "volume_set", json!({"volume_level": (v * 100.0).round() / 100.0})),
        _ => return None,
    })
}

fn select_command(entity: &str, what: &str, index: i32, states: &HashMap<String, EntityState>) -> Option<Command> {
    let st = states.get(entity)?;
    let (list_attr, service, field) = match what {
        "hvac" => ("hvac_modes", "set_hvac_mode", "hvac_mode"),
        "fan" => ("fan_modes", "set_fan_mode", "fan_mode"),
        "preset" => ("preset_modes", "set_preset_mode", "preset_mode"),
        "swing" => ("swing_modes", "set_swing_mode", "swing_mode"),
        "source" => ("source_list", "select_source", "source"),
        _ => return None,
    };
    let value = dashboard::attr_list(st, list_attr).get(usize::try_from(index).ok()?)?.clone();
    let domain = entity.split('.').next().unwrap_or("");
    Some(Command::CallService {
        domain: domain.into(),
        service: service.into(),
        entity_id: entity.into(),
        data: json!({ field: value }),
    })
}

fn update_camera_mode(ui: &MainWindow, camera: &CameraControl) {
    // A camera shown by a show request: its entity while it is open, then
    // back to the dashboard's camera.
    with_state(|s| {
        let wanted = match &s.camera_override {
            Some(e) if ui.get_camera_full() => Some(e.clone()),
            Some(_) => {
                s.camera_override = None;
                None
            }
            None => None,
        };
        let entity = wanted.or_else(|| s.camera_row.and_then(|i| s.rows[i].a.as_ref()).map(|c| c.entity.clone()));
        let mut current = camera.entity.lock().unwrap();
        if *current != entity {
            *current = entity;
        }
    });
    let dashboard_hidden = ui.get_display_mode() == 1 && !ui.get_show_dashboard();
    let mode = if !ui.get_paired() || ui.get_screensaver() || ui.get_settings_open() || ui.get_more_info_open() {
        camera::OFF
    } else if ui.get_camera_full() {
        camera::FULL
    } else if dashboard_hidden {
        camera::OFF
    } else {
        with_state(|s| {
            let Some(idx) = s.camera_row else { return camera::OFF };
            let offset: f32 = dashboard::LIST_PADDING_TOP
                + s.rows[..idx].iter().map(|r| dashboard::row_height(r.kind) + dashboard::ROW_SPACING).sum::<f32>();
            let top = offset + ui.get_scroll_y();
            let bottom = top + dashboard::row_height(dashboard::ROW_CAMERA);
            if bottom > 0.0 && top < ui.get_list_height() { camera::LIST } else { camera::OFF }
        })
    };
    camera.mode.store(mode, Ordering::SeqCst);
}

fn wake(ui: &MainWindow) {
    platform::note_activity();
    if ui.get_screensaver() {
        ui.set_screensaver(false);
        with_state(|s| s.screensaver = false);
    }
}

fn start_screensaver(ui: &MainWindow, s: &mut UiState) {
    s.screensaver = true;
    ui.set_screensaver_black(s.settings.screensaver_style == "black");
    ui.set_more_info_open(false);
    ui.set_settings_open(false);
    ui.set_camera_full(false);
    ui.set_show_dashboard(false);
    ui.set_weather_open(false);
    ui.set_media_open(false);
    ui.set_message_open(false);
    s.show_until = None;
    s.more_info = None;
    let weather = assistant::effective(&s.settings).weather_entity;
    let (info, icon) = match s.states.get(&weather) {
        Some(w) if !weather.is_empty() => (
            dashboard::weather_text(w),
            icons::glyph(dashboard::weather_icon(&w.state)).map(String::from).unwrap_or_default(),
        ),
        _ => (String::new(), String::new()),
    };
    ui.set_saver_info(info.into());
    ui.set_saver_icon(icon.into());
    ui.set_screensaver(true);
}

/// Copies what Home Assistant sees (ESPHome API) from the UI state.
fn report_device(s: &mut UiState) {
    let lux_changed = match (s.lux, s.last_reported_lux) {
        (Some(now), Some(before)) => (now - before).abs() > (before * 0.15).max(5.0),
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
    let after = (
        d.theme.clone(), d.brightness, d.auto_brightness, d.screensaver_seconds, d.screensaver_style.clone(),
        d.proximity_wake, d.screen_on, d.dashboard.clone(), d.presence, d.display_mode.clone(), d.volume,
    );
    if lux_changed || periodic {
        d.lux = s.lux.map(|l| l.round());
        s.last_reported_lux = s.lux;
    }
    if periodic {
        d.soc_temp = hw::soc_temperature();
        d.wifi_dbm = hw::wifi_dbm();
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

/// Runs twice a second: theme, screensaver, proximity wake, backlight.
fn tick(ui: &MainWindow) {
    let wake_now = with_state(|s| {
        s.lux = s.light.lux();
        // Theme
        let dark = match s.settings.theme.as_str() {
            "light" => false,
            "auto" => match s.states.get("sun.sun") {
                Some(sun) => sun.state == "below_horizon",
                None => s.lux.map(|l| l < 10.0).unwrap_or(true),
            },
            _ => true,
        };
        if s.dark != Some(dark) {
            s.dark = Some(dark);
            ui.global::<Theme>().set_dark(dark);
        }
        // Layer opened by a show request: close it when its time is up.
        if s.show_until.is_some_and(|t| Instant::now() >= t) {
            s.show_until = None;
            ui.set_message_open(false);
            ui.set_weather_open(false);
            ui.set_media_open(false);
            if s.camera_override.is_some() {
                ui.set_camera_full(false);
            }
            if s.settings.display_mode == "assistant" {
                ui.set_show_dashboard(false);
            }
        }
        // Assistant mode: back to the home screen after a minute untouched.
        let idle = platform::idle_time();
        if s.settings.display_mode == "assistant"
            && s.show_until.is_none()
            && idle > Duration::from_secs(60)
            && (ui.get_show_dashboard() || ui.get_more_info_open() || ui.get_weather_open() || ui.get_media_open())
        {
            ui.set_show_dashboard(false);
            ui.set_more_info_open(false);
            ui.set_weather_open(false);
            ui.set_media_open(false);
            s.more_info = None;
        }
        // Screensaver (not while Home Assistant shows something)
        let timeout = s.settings.screensaver_seconds;
        let showing = s.show_until.is_some() || ui.get_message_open() || s.camera_override.is_some();
        if !s.screensaver && !showing && timeout > 0 && ui.get_paired() && idle > Duration::from_secs(timeout as u64) {
            start_screensaver(ui, s);
        }
        // Proximity: presence for Home Assistant, wake from the screensaver
        let mut wake_now = false;
        if let Some(p) = s.light.proximity() {
            let approach = s.proximity.update(p);
            wake_now = approach && s.screensaver && s.settings.proximity_wake;
        }
        // Backlight
        let target = if s.screensaver && s.settings.screensaver_style == "black" && !wake_now {
            0.0
        } else {
            let normal = if s.settings.auto_brightness {
                s.lux.map(hw::auto_brightness_permille).unwrap_or(600) as f64
            } else {
                s.settings.brightness.max(5) as f64 * 10.0
            };
            if s.screensaver && !wake_now { normal.min(150.0) } else { normal }
        };
        s.brightness = if (target - s.brightness).abs() < 5.0 || target > s.brightness + 300.0 {
            target
        } else {
            s.brightness + (target - s.brightness) * 0.25
        };
        let level = s.brightness.round() as u32;
        s.backlight.set_permille(level);
        report_device(s);
        let refresh = s.assistant_refreshed.elapsed() > Duration::from_secs(60);
        if refresh {
            s.assistant_refreshed = Instant::now();
        }
        (wake_now, refresh)
    });
    let (wake_now, refresh) = wake_now;
    if wake_now {
        wake(ui);
    }
    if refresh {
        refresh_assistant(ui);
    }
}

fn on_setting(ui: &MainWindow, key: &str, value: &str, camera: &CameraControl) {
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
            ui.set_settings_open(false);
            send(Command::Reload);
            return;
        }
        _ => {}
    }
    with_state(|s| {
        match key {
            "theme" => s.settings.theme = value.to_string(),
            "auto_brightness" => s.settings.auto_brightness = value == "true",
            "brightness" => s.settings.brightness = value.parse::<u8>().unwrap_or(70).clamp(5, 100),
            "screensaver_index" => {
                if let Some((secs, _)) = value.parse::<usize>().ok().and_then(|i| SCREENSAVER_CHOICES.get(i)) {
                    s.settings.screensaver_seconds = *secs;
                }
            }
            "screensaver_style" => s.settings.screensaver_style = value.to_string(),
            "proximity_wake" => s.settings.proximity_wake = value == "true",
            "display_mode" => {
                if matches!(value, "dashboard" | "assistant") {
                    s.settings.display_mode = value.to_string();
                    ui.set_show_dashboard(false);
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
                    s.view_index = 0;
                    reload = true;
                }
            }
            _ => eprintln!("nspanel-app: unknown setting {key}"),
        }
        s.settings.save();
    });
    refresh_settings(ui);
    if key == "display_mode" {
        apply_assistant_config(ui);
    } else {
        refresh_assistant(ui);
    }
    if reload {
        send(Command::Reload);
        let _ = camera;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("TZ").is_none() {
        std::env::set_var("TZ", DEFAULT_TZ);
    }
    // SAFETY: tzset only re-reads the TZ variable.
    unsafe { tzset() };

    platform::install_screenshot_signal();
    // The boot splash owns the display until now: it shows the finished logo,
    // gives up DRM master and keeps the picture until this app has drawn.
    let splash = splash_handoff();
    let display = display::Display::open("/dev/dri/card0")?;
    let (width, height) = (display.width, display.height);
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

    let window = MinimalSoftwareWindow::new(RepaintBufferType::SwappedBuffers);
    window.set_size(slint::PhysicalSize::new(width, height));
    slint::platform::set_platform(Box::new(platform::PanelPlatform::new(Rc::clone(&window), display, touch)))
        .map_err(|e| format!("set_platform: {e:?}"))?;

    let ui = MainWindow::new()?;
    update_clock(&ui);
    // Hand-over picture: the splash's last frame (same logo, same pixels),
    // then it shrinks and fades over the UI.
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
    let intro = slint::Timer::default();
    let weak = ui.as_weak();
    intro.start(slint::TimerMode::SingleShot, Duration::from_millis(250), move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_intro(false);
        }
    });
    let intro_end = slint::Timer::default();
    let weak = ui.as_weak();
    intro_end.start(slint::TimerMode::SingleShot, Duration::from_millis(900), move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_intro_visible(false);
        }
        if let Some(pid) = splash {
            // SAFETY: plain kill(2).
            unsafe { libc::kill(pid, libc::SIGTERM) };
            let _ = std::fs::remove_file(SPLASH_PIDFILE);
        }
    });

    let settings = Settings::load();
    let config = AppConfig::load();
    let fixture = std::env::var("NSPANEL_FIXTURE").ok();
    let model = Rc::new(VecModel::<RowData>::default());
    ui.set_rows(model.clone().into());
    let assistant_model = Rc::new(VecModel::<RowData>::default());
    ui.set_assistant_rows(assistant_model.clone().into());
    let phrase = wake_phrase(&settings.wake_words);
    ui.set_screensaver_labels(ModelRc::new(VecModel::from(
        SCREENSAVER_CHOICES.iter().map(|(_, l)| SharedString::from(*l)).collect::<Vec<_>>(),
    )));

    let shared = Arc::new(ha::Shared::default());
    let dashboard_name = if !settings.dashboard.is_empty() {
        settings.dashboard.clone()
    } else {
        config.as_ref().map(|c| c.dashboard.clone()).unwrap_or_default()
    };
    *shared.dashboard.lock().unwrap() = dashboard_name;

    let auth = config
        .as_ref()
        .ok()
        .map(|c| Arc::new(Auth::load(&c.ha_url, PathBuf::from(format!("{PERSIST_DIR}/ha_auth.json")))));
    let device = Arc::new(std::sync::Mutex::new(esphome::DeviceState::default()));
    let (device_notify, device_rx) = mpsc::channel::<()>();
    let mic_muted = Arc::new(std::sync::atomic::AtomicBool::new(settings.mic_muted));
    let wake_words = Arc::new(std::sync::Mutex::new(settings.wake_words.clone()));
    let player = Arc::new(voice::Player::new());
    hw::set_volume(settings.volume);
    let ringing_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    ui.set_mic_muted(settings.mic_muted);
    STATE.with(|s| {
        *s.borrow_mut() = Some(UiState {
            config: config.as_ref().ok().cloned(),
            auth: auth.clone(),
            shared: shared.clone(),
            commands: None,
            settings,
            dash_config: Value::Null,
            views: Vec::new(),
            view_index: 0,
            rows: Vec::new(),
            states: HashMap::new(),
            model,
            camera_row: None,
            pairing_started: false,
            more_info: None,
            dashboards: Vec::new(),
            connected: false,
            screensaver: false,
            backlight: hw::Backlight::open(),
            light: hw::LightSensor::open(),
            proximity: hw::ProximityWake::default(),
            brightness: 600.0,
            lux: None,
            dark: None,
            started: Instant::now(),
            device: device.clone(),
            device_notify,
            last_reported_lux: None,
            last_report: Instant::now() - Duration::from_secs(120),
            timers: HashMap::new(),
            ringing: None,
            ringing_flag: ringing_flag.clone(),
            mic_muted: mic_muted.clone(),
            wake_words: wake_words.clone(),
            player: player.clone(),
            assist_hide_at: None,
            forecast_hourly: Vec::new(),
            forecast_daily: Vec::new(),
            assistant_rows: Vec::new(),
            assistant_model,
            wake_phrase: phrase,
            art_url: None,
            art_tx: None,
            show_until: None,
            camera_override: None,
            assistant_reset: 0,
            assistant_refreshed: Instant::now(),
        })
    });
    apply_assistant_config(&ui);
    let _ = with_state(|s| s.started);

    let camera = CameraControl::default();

    // Voice satellite and ESPHome native API (settings, sensors, Assist).
    let (to_api_tx, to_api_rx) = mpsc::channel::<voice::ToApi>();
    let (from_api_tx, from_api_rx) = mpsc::channel::<voice::FromApi>();
    let link = esphome::VoiceLink { to_api: to_api_rx, from_api: from_api_tx, active_wake_words: wake_words.clone() };
    let weak = ui.as_weak();
    let cam = camera.clone();
    esphome::spawn(device, device_rx, link, move |control| {
        let weak = weak.clone();
        let cam = cam.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            match control {
                esphome::Control::Setting(k, v) => on_setting(&ui, &k, &v, &cam),
                esphome::Control::Screen(true) => wake(&ui),
                esphome::Control::Screen(false) => with_state(|s| {
                    if !s.screensaver {
                        start_screensaver(&ui, s);
                    }
                }),
                esphome::Control::Timer { kind, id, name, left, active, .. } => {
                    handle_timer(&ui, kind, id, name, left, active)
                }
            }
        });
    });
    if fixture.is_none() {
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
    }
    let weak = ui.as_weak();
    ui.on_stop_timer(move || {
        if let Some(ui) = weak.upgrade() {
            with_state(|s| {
                s.ringing = None;
                s.ringing_flag.store(false, Ordering::SeqCst);
            });
            ui.set_timer_ringing(false);
        }
    });
    let weak = ui.as_weak();
    ui.on_dismiss_assist(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_assist_state(0);
        }
    });

    ui.on_media_action(|action| {
        let cmd = with_state(|s| {
            let c = assistant::effective(&s.settings);
            command_for(&c.media_player_entity, action.as_str(), &s.states)
        });
        if let Some(cmd) = cmd {
            send(cmd);
        }
    });
    ui.on_media_volume(|v| {
        let cmd = with_state(|s| {
            let c = assistant::effective(&s.settings);
            slider_command(&c.media_player_entity, "volume", v, &s.states)
        });
        if let Some(cmd) = cmd {
            send(cmd);
        }
    });
    ui.on_overlay_closed(|| with_state(|s| s.show_until = None));

    // UI callbacks
    let weak = ui.as_weak();
    let cam = camera.clone();
    ui.on_action(move |entity, action| {
        let Some(ui) = weak.upgrade() else { return };
        let entity = entity.as_str();
        if action == "tap" {
            let details = with_state(|s| dashboard::has_details(entity, &s.states));
            if details {
                open_more_info(&ui, entity);
                return;
            }
            let toggleable = matches!(entity.split('.').next(), Some("light" | "switch" | "fan" | "input_boolean" | "automation" | "media_player"));
            if !toggleable {
                return;
            }
        }
        let action = if action == "tap" { "toggle" } else { action.as_str() };
        if let Some(cmd) = with_state(|s| command_for(entity, action, &s.states)) {
            send(cmd);
        }
        let _ = &cam;
    });
    let weak = ui.as_weak();
    let cam = camera.clone();
    ui.on_view_selected(move |i| {
        if let Some(ui) = weak.upgrade() {
            with_state(|s| s.view_index = i.max(0) as usize);
            apply_view(&ui, &cam);
        }
    });
    ui.on_more_info_slider(|what, v| {
        let cmd = with_state(|s| {
            let entity = s.more_info.as_ref()?.0.clone();
            slider_command(&entity, what.as_str(), v, &s.states)
        });
        if let Some(cmd) = cmd {
            send(cmd);
        }
    });
    ui.on_more_info_select(|what, i| {
        let cmd = with_state(|s| {
            let entity = s.more_info.as_ref()?.0.clone();
            select_command(&entity, what.as_str(), i, &s.states)
        });
        if let Some(cmd) = cmd {
            send(cmd);
        }
    });
    ui.on_more_info_action(|action| {
        let cmd = with_state(|s| {
            let entity = s.more_info.as_ref()?.0.clone();
            command_for(&entity, action.as_str(), &s.states)
        });
        if let Some(cmd) = cmd {
            send(cmd);
        }
    });
    ui.on_more_info_closed(|| with_state(|s| s.more_info = None));
    let weak = ui.as_weak();
    ui.on_settings_opened(move || {
        send(Command::ListDashboards);
        if let Some(ui) = weak.upgrade() {
            refresh_settings(&ui);
        }
    });
    let weak = ui.as_weak();
    let cam = camera.clone();
    ui.on_setting_changed(move |k, v| {
        if let Some(ui) = weak.upgrade() {
            on_setting(&ui, k.as_str(), v.as_str(), &cam);
        }
    });
    let weak = ui.as_weak();
    ui.on_wake(move || {
        if let Some(ui) = weak.upgrade() {
            wake(&ui);
        }
    });

    // Timers: clock, sensors/backlight/screensaver, camera visibility, dev scroll.
    let clock = slint::Timer::default();
    let weak = ui.as_weak();
    clock.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
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
    let camera_timer = slint::Timer::default();
    let weak = ui.as_weak();
    let cam = camera.clone();
    camera_timer.start(slint::TimerMode::Repeated, Duration::from_millis(400), move || {
        if let Some(ui) = weak.upgrade() {
            update_camera_mode(&ui, &cam);
        }
    });
    let dev_scroll = slint::Timer::default();
    let weak = ui.as_weak();
    dev_scroll.start(slint::TimerMode::Repeated, Duration::from_millis(200), move || {
        let dev: Vec<String> = std::mem::take(&mut *platform::DEV_COMMANDS.lock().unwrap());
        for line in dev {
            let (Some(ui), Ok(v)) = (weak.upgrade(), serde_json::from_str::<Value>(&line)) else { continue };
            if let Some(show) = v.get("show") {
                handle_show(&ui, show);
            }
            if let Some(a) = v.get("assist").and_then(Value::as_array) {
                ui.set_assist_heard(a.get(1).and_then(Value::as_str).unwrap_or("").into());
                ui.set_assist_answer(a.get(2).and_then(Value::as_str).unwrap_or("").into());
                ui.set_assist_state(a.first().and_then(Value::as_i64).unwrap_or(0) as i32);
            }
        }
        if platform::SCROLL_REQUEST.swap(false, Ordering::SeqCst) {
            if let Some(ui) = weak.upgrade() {
                let before = ui.get_scroll_y();
                ui.invoke_scroll_by(380.0);
                if ui.get_scroll_y() == before {
                    ui.invoke_scroll_top();
                }
            }
        }
    });

    // Development: draw a saved dashboard config + states without Home Assistant.
    if let Some(path) = fixture {
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        handle_event(&ui, Event::Dashboard(fixture["config"].clone()), &camera);
        let states = fixture["states"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(id, v)| {
                        let st = EntityState {
                            state: v["state"].as_str().unwrap_or_default().to_string(),
                            attrs: v["attributes"].as_object().cloned().unwrap_or_default(),
                        };
                        (id.clone(), Some(st))
                    })
                    .collect()
            })
            .unwrap_or_default();
        handle_event(&ui, Event::States(states), &camera);
        handle_event(&ui, Event::Status { text: "Fixture".into(), connected: true }, &camera);
        if fixture.get("panel").is_some() {
            handle_event(&ui, Event::Panel(Some(fixture["panel"].clone())), &camera);
        }
        for (key, hourly) in [("forecast_hourly", true), ("forecast_daily", false)] {
            if let Some(items) = fixture[key].as_array() {
                handle_event(&ui, Event::Forecast { hourly, items: items.clone() }, &camera);
            }
        }
        handle_event(
            &ui,
            Event::Dashboards(vec![("".into(), "Overview".into()), ("wall-panel".into(), "Wall panel".into())]),
            &camera,
        );
        ui.run()?;
        return Ok(());
    }

    let (config, auth) = match (config, auth) {
        (Ok(c), Some(a)) => (c, a),
        (result, _) => {
            let e = result.err().unwrap_or_else(|| "no login store".into());
            eprintln!("nspanel-app: {e}");
            ui.set_paired(true);
            ui.set_status(format!("Not configured: {e}").into());
            ui.set_header_info("Not configured".into());
            ui.run()?;
            return Ok(());
        }
    };
    ui.set_paired(auth.is_paired());
    ui.set_status("Connecting to Home Assistant…".into());

    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    with_state(|s| s.commands = Some(cmd_tx));

    let weak = ui.as_weak();
    let cam = camera.clone();
    ha::spawn(config.ws_url(), shared, auth.clone(), cmd_rx, move |event| {
        let weak = weak.clone();
        let cam = cam.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                handle_event(&ui, event, &cam);
            }
        });
    });

    let weak = ui.as_weak();
    let art_tx = artwork::spawn(config.ha_url.clone(), auth.clone(), move |img| {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_has_art(img.is_some());
                if let Some(img) = img {
                    ui.set_media_art(slint::Image::from_rgb8(img));
                }
            }
        });
    });
    with_state(|s| {
        s.art_tx = Some(art_tx);
        s.art_url = None;
    });
    refresh_assistant(&ui);

    let weak = ui.as_weak();
    camera::spawn(config.ha_url.clone(), auth, camera.clone(), move |frame| {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_camera_image(slint::Image::from_rgb8(frame));
            }
        });
    });

    ui.run()?;
    Ok(())
}
