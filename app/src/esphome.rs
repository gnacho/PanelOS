//! ESPHome native API server (TCP 6053, plaintext for now): Home Assistant's
//! built-in ESPHome integration adds the panel as a device with its settings
//! and sensors as entities. One client (Home Assistant) at a time.
//!
//! Frame: 0x00, varint payload length, varint message type, protobuf payload.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use prost::Message;

#[allow(clippy::all, dead_code)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/_.rs"));
}
use proto::*;

pub const PORT: u16 = 6053;

/// What the panel reports to Home Assistant; updated by the UI thread.
#[derive(Debug, Clone, Default)]
pub struct DeviceState {
    pub theme: String,
    pub brightness: u8,
    pub auto_brightness: bool,
    pub screensaver_seconds: u32,
    pub screensaver_style: String,
    pub proximity_wake: bool,
    pub screen_on: bool,
    pub dashboards: Vec<(String, String)>,
    pub dashboard: String,
    pub lux: Option<f64>,
    pub soc_temp: Option<f64>,
    pub wifi_dbm: Option<f64>,
    pub presence: bool,
    pub ip: String,
    pub mic_muted: bool,
    pub volume: u8,
    /// "dashboard" or "assistant".
    pub display_mode: String,
}

/// Requests from Home Assistant, carried out on the UI thread.
#[derive(Debug, Clone)]
pub enum Control {
    /// Same keys as the settings screen (`on_setting`).
    Setting(String, String),
    /// true = wake the screen, false = screensaver now.
    Screen(bool),
    /// Assist timer event (VoiceAssistantTimerEventResponse).
    Timer { kind: i32, id: String, name: String, total: u32, left: u32, active: bool },
}

/// Voice satellite plumbing for the API thread.
pub struct VoiceLink {
    pub to_api: Receiver<crate::voice::ToApi>,
    pub from_api: std::sync::mpsc::Sender<crate::voice::FromApi>,
    pub active_wake_words: Arc<Mutex<Vec<String>>>,
}

// VoiceAssistantFeature flags (aioesphomeapi): voice assistant, speaker,
// audio over the API, timers, announcements. Home Assistant only asks for
// the wake word configuration when ANNOUNCE is set.
const VOICE_FEATURES: u32 = 1 | 2 | 4 | 8 | 16;

const THEMES: [(&str, &str); 3] = [("dark", "Dark"), ("light", "Light"), ("auto", "Auto")];
const STYLES: [(&str, &str); 2] = [("clock", "Clock"), ("black", "Screen off")];
pub const DISPLAY_MODES: [(&str, &str); 2] = [("dashboard", "Dashboard"), ("assistant", "Assistant")];

#[derive(Clone, Copy, PartialEq)]
enum Id {
    Theme,
    Brightness,
    AutoBrightness,
    Screensaver,
    ScreensaverStyle,
    ProximityWake,
    Screen,
    Dashboard,
    Illuminance,
    SocTemp,
    Wifi,
    Presence,
    Ip,
    Version,
    RestartApp,
    Reboot,
    MicMute,
    DisplayMode,
    Volume,
}

const ENTITIES: &[(Id, u32, &str, &str, &str)] = &[
    (Id::Screen, 1, "screen", "Screen", "mdi:tablet"),
    (Id::Dashboard, 2, "dashboard", "Dashboard", "mdi:view-dashboard"),
    (Id::Theme, 3, "theme", "Theme", "mdi:theme-light-dark"),
    (Id::Brightness, 4, "brightness", "Brightness", "mdi:brightness-6"),
    (Id::AutoBrightness, 5, "auto_brightness", "Automatic brightness", "mdi:brightness-auto"),
    (Id::Screensaver, 6, "screensaver", "Screensaver after", "mdi:timer-outline"),
    (Id::ScreensaverStyle, 7, "screensaver_style", "Screensaver style", "mdi:clock-outline"),
    (Id::ProximityWake, 8, "proximity_wake", "Wake on approach", "mdi:motion-sensor"),
    (Id::Illuminance, 9, "illuminance", "Illuminance", ""),
    (Id::Presence, 10, "presence", "Presence", ""),
    (Id::SocTemp, 11, "soc_temperature", "SoC temperature", ""),
    (Id::Wifi, 12, "wifi_signal", "Wi-Fi signal", ""),
    (Id::Ip, 13, "ip_address", "IP address", "mdi:ip-network"),
    (Id::Version, 14, "app_version", "App version", "mdi:package-variant"),
    (Id::RestartApp, 15, "restart_app", "Restart app", "mdi:restart"),
    (Id::Reboot, 16, "reboot", "Reboot", "mdi:power-cycle"),
    (Id::MicMute, 17, "mute_microphone", "Mute microphone", "mdi:microphone-off"),
    (Id::DisplayMode, 18, "display_mode", "Display mode", "mdi:monitor-dashboard"),
    (Id::Volume, 19, "volume", "Volume", "mdi:volume-high"),
];

const CONFIG: i32 = EntityCategory::Config as i32;
const DIAGNOSTIC: i32 = EntityCategory::Diagnostic as i32;

#[derive(Clone, PartialEq, Debug)]
enum Value {
    Str(String),
    Float(Option<f32>),
    Bool(bool),
}

fn dashboard_titles(s: &DeviceState) -> Vec<String> {
    let mut titles: Vec<String> = s.dashboards.iter().map(|(_, t)| t.clone()).collect();
    if titles.is_empty() {
        titles.push(if s.dashboard.is_empty() { "Overview".into() } else { s.dashboard.clone() });
    }
    titles
}

fn value_of(id: Id, s: &DeviceState) -> Option<Value> {
    let label = |list: &[(&str, &str)], v: &str| list.iter().find(|(k, _)| *k == v).map(|(_, l)| l.to_string()).unwrap_or_default();
    Some(match id {
        Id::Theme => Value::Str(label(&THEMES, &s.theme)),
        Id::ScreensaverStyle => Value::Str(label(&STYLES, &s.screensaver_style)),
        Id::DisplayMode => Value::Str(label(&DISPLAY_MODES, &s.display_mode)),
        Id::Screensaver => Value::Str(
            crate::settings::SCREENSAVER_CHOICES
                .iter()
                .find(|(secs, _)| *secs == s.screensaver_seconds)
                .map(|(_, l)| l.to_string())
                .unwrap_or_default(),
        ),
        Id::Dashboard => Value::Str(
            s.dashboards
                .iter()
                .find(|(u, _)| *u == s.dashboard)
                .map(|(_, t)| t.clone())
                .unwrap_or_else(|| if s.dashboard.is_empty() { "Overview".into() } else { s.dashboard.clone() }),
        ),
        Id::Brightness => Value::Float(Some(s.brightness as f32)),
        Id::Volume => Value::Float(Some(s.volume as f32)),
        Id::AutoBrightness => Value::Bool(s.auto_brightness),
        Id::ProximityWake => Value::Bool(s.proximity_wake),
        Id::Screen => Value::Bool(s.screen_on),
        Id::MicMute => Value::Bool(s.mic_muted),
        Id::Illuminance => Value::Float(s.lux.map(|v| v as f32)),
        Id::SocTemp => Value::Float(s.soc_temp.map(|v| v as f32)),
        Id::Wifi => Value::Float(s.wifi_dbm.map(|v| v as f32)),
        Id::Presence => Value::Bool(s.presence),
        Id::Ip => Value::Str(s.ip.clone()),
        Id::Version => Value::Str(env!("CARGO_PKG_VERSION").to_string()),
        Id::RestartApp | Id::Reboot => return None,
    })
}

fn encode_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn send<M: Message>(stream: &mut TcpStream, msg_type: u32, msg: &M) -> io::Result<()> {
    let payload = msg.encode_to_vec();
    let mut frame = Vec::with_capacity(payload.len() + 8);
    frame.push(0);
    encode_varint(payload.len() as u64, &mut frame);
    encode_varint(msg_type as u64, &mut frame);
    frame.extend_from_slice(&payload);
    stream.write_all(&frame)
}

/// Takes one complete frame off the front of the buffer.
fn take_frame(buf: &mut Vec<u8>) -> io::Result<Option<(u32, Vec<u8>)>> {
    fn varint(b: &[u8], pos: &mut usize) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *b.get(*pos)?;
            *pos += 1;
            v |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
    if buf.is_empty() {
        return Ok(None);
    }
    if buf[0] != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "encrypted or invalid frame (expected plaintext)"));
    }
    let mut pos = 1;
    let Some(len) = varint(buf, &mut pos) else { return Ok(None) };
    let Some(kind) = varint(buf, &mut pos) else { return Ok(None) };
    let end = pos + len as usize;
    if buf.len() < end {
        return Ok(None);
    }
    let payload = buf[pos..end].to_vec();
    buf.drain(..end);
    Ok(Some((kind as u32, payload)))
}

fn mac_address() -> String {
    std::fs::read_to_string("/sys/class/net/wlan0/address")
        .map(|s| s.trim().to_uppercase())
        .unwrap_or_else(|_| "00:00:00:00:00:00".into())
}

fn list_entities(stream: &mut TcpStream, s: &DeviceState) -> io::Result<()> {
    for &(id, key, object_id, name, icon) in ENTITIES {
        let common = |category: i32| (object_id.to_string(), key, name.to_string(), icon.to_string(), category);
        match id {
            Id::Theme | Id::Screensaver | Id::ScreensaverStyle | Id::Dashboard | Id::DisplayMode => {
                let options: Vec<String> = match id {
                    Id::Theme => THEMES.iter().map(|(_, l)| l.to_string()).collect(),
                    Id::DisplayMode => DISPLAY_MODES.iter().map(|(_, l)| l.to_string()).collect(),
                    Id::ScreensaverStyle => STYLES.iter().map(|(_, l)| l.to_string()).collect(),
                    Id::Screensaver => crate::settings::SCREENSAVER_CHOICES.iter().map(|(_, l)| l.to_string()).collect(),
                    _ => dashboard_titles(s),
                };
                let (object_id, key, name, icon, category) = common(CONFIG);
                send(stream, 52, &ListEntitiesSelectResponse {
                    object_id, key, name, icon, options, entity_category: category, ..Default::default()
                })?;
            }
            Id::Brightness | Id::Volume => {
                let (min, step, category) = if id == Id::Volume { (0.0, 5.0, 0) } else { (5.0, 1.0, CONFIG) };
                let (object_id, key, name, icon, category) = common(category);
                send(stream, 49, &ListEntitiesNumberResponse {
                    object_id, key, name, icon, min_value: min, max_value: 100.0, step,
                    entity_category: category, unit_of_measurement: "%".into(),
                    mode: NumberMode::Slider as i32, ..Default::default()
                })?;
            }
            Id::AutoBrightness | Id::ProximityWake | Id::Screen | Id::MicMute => {
                let category = if matches!(id, Id::Screen | Id::MicMute) { 0 } else { CONFIG };
                let (object_id, key, name, icon, category) = common(category);
                send(stream, 17, &ListEntitiesSwitchResponse {
                    object_id, key, name, icon, entity_category: category, ..Default::default()
                })?;
            }
            Id::Illuminance | Id::SocTemp | Id::Wifi => {
                let (unit, class, decimals, category) = match id {
                    Id::Illuminance => ("lx", "illuminance", 0, 0),
                    Id::SocTemp => ("°C", "temperature", 1, DIAGNOSTIC),
                    _ => ("dBm", "signal_strength", 0, DIAGNOSTIC),
                };
                let (object_id, key, name, icon, category) = common(category);
                send(stream, 16, &ListEntitiesSensorResponse {
                    object_id, key, name, icon, unit_of_measurement: unit.into(), accuracy_decimals: decimals,
                    device_class: class.into(), state_class: SensorStateClass::StateClassMeasurement as i32,
                    entity_category: category, ..Default::default()
                })?;
            }
            Id::Presence => {
                let (object_id, key, name, icon, category) = common(0);
                send(stream, 12, &ListEntitiesBinarySensorResponse {
                    object_id, key, name, icon, device_class: "occupancy".into(), entity_category: category,
                    ..Default::default()
                })?;
            }
            Id::Ip | Id::Version => {
                let (object_id, key, name, icon, category) = common(DIAGNOSTIC);
                send(stream, 18, &ListEntitiesTextSensorResponse {
                    object_id, key, name, icon, entity_category: category, ..Default::default()
                })?;
            }
            Id::RestartApp | Id::Reboot => {
                let (object_id, key, name, icon, category) = common(CONFIG);
                send(stream, 61, &ListEntitiesButtonResponse {
                    object_id, key, name, icon, entity_category: category,
                    device_class: "restart".into(), ..Default::default()
                })?;
            }
        }
    }
    send(stream, 19, &ListEntitiesDoneResponse {})
}

fn send_state(stream: &mut TcpStream, id: Id, key: u32, value: &Value) -> io::Result<()> {
    match (id, value) {
        (Id::Theme | Id::Screensaver | Id::ScreensaverStyle | Id::Dashboard | Id::DisplayMode, Value::Str(v)) => {
            send(stream, 53, &SelectStateResponse { key, state: v.clone(), missing_state: v.is_empty(), ..Default::default() })
        }
        (Id::Brightness | Id::Volume, Value::Float(v)) => send(stream, 50, &NumberStateResponse {
            key, state: v.unwrap_or(0.0), missing_state: v.is_none(), ..Default::default()
        }),
        (Id::AutoBrightness | Id::ProximityWake | Id::Screen | Id::MicMute, Value::Bool(v)) => {
            send(stream, 26, &SwitchStateResponse { key, state: *v, ..Default::default() })
        }
        (Id::Illuminance | Id::SocTemp | Id::Wifi, Value::Float(v)) => send(stream, 25, &SensorStateResponse {
            key, state: v.unwrap_or(0.0), missing_state: v.is_none(), ..Default::default()
        }),
        (Id::Presence, Value::Bool(v)) => {
            send(stream, 21, &BinarySensorStateResponse { key, state: *v, ..Default::default() })
        }
        (Id::Ip | Id::Version, Value::Str(v)) => send(stream, 27, &TextSensorStateResponse {
            key, state: v.clone(), missing_state: v.is_empty(), ..Default::default()
        }),
        _ => Ok(()),
    }
}

fn entity_by_key(key: u32) -> Option<Id> {
    ENTITIES.iter().find(|e| e.1 == key).map(|e| e.0)
}

fn handle_command(kind: u32, payload: &[u8], s: &DeviceState, control: &dyn Fn(Control)) {
    let setting = |k: &str, v: String| control(Control::Setting(k.to_string(), v));
    match kind {
        54 => {
            let Ok(m) = SelectCommandRequest::decode(payload) else { return };
            match entity_by_key(m.key) {
                Some(Id::Theme) => {
                    if let Some((k, _)) = THEMES.iter().find(|(_, l)| *l == m.state) {
                        setting("theme", k.to_string());
                    }
                }
                Some(Id::ScreensaverStyle) => {
                    if let Some((k, _)) = STYLES.iter().find(|(_, l)| *l == m.state) {
                        setting("screensaver_style", k.to_string());
                    }
                }
                Some(Id::DisplayMode) => {
                    if let Some((k, _)) = DISPLAY_MODES.iter().find(|(_, l)| *l == m.state) {
                        setting("display_mode", k.to_string());
                    }
                }
                Some(Id::Screensaver) => {
                    if let Some(i) = crate::settings::SCREENSAVER_CHOICES.iter().position(|(_, l)| *l == m.state) {
                        setting("screensaver_index", i.to_string());
                    }
                }
                Some(Id::Dashboard) => {
                    if let Some(i) = s.dashboards.iter().position(|(_, t)| *t == m.state) {
                        setting("dashboard_index", i.to_string());
                    }
                }
                _ => {}
            }
        }
        51 => {
            let Ok(m) = NumberCommandRequest::decode(payload) else { return };
            match entity_by_key(m.key) {
                Some(Id::Brightness) => setting("brightness", format!("{}", m.state.round() as i32)),
                Some(Id::Volume) => setting("volume", format!("{}", m.state.round() as i32)),
                _ => {}
            }
        }
        33 => {
            let Ok(m) = SwitchCommandRequest::decode(payload) else { return };
            match entity_by_key(m.key) {
                Some(Id::AutoBrightness) => setting("auto_brightness", m.state.to_string()),
                Some(Id::ProximityWake) => setting("proximity_wake", m.state.to_string()),
                Some(Id::Screen) => control(Control::Screen(m.state)),
                Some(Id::MicMute) => setting("mic_muted", m.state.to_string()),
                _ => {}
            }
        }
        62 => {
            let Ok(m) = ButtonCommandRequest::decode(payload) else { return };
            match entity_by_key(m.key) {
                Some(Id::RestartApp) => setting("restart_app", String::new()),
                Some(Id::Reboot) => setting("reboot", String::new()),
                _ => {}
            }
        }
        _ => {}
    }
}

fn wake_word_config(voice: &VoiceLink) -> VoiceAssistantConfigurationResponse {
    let available: Vec<VoiceAssistantWakeWord> = crate::voice::available_wake_words()
        .into_iter()
        .filter(|w| !crate::voice::INTERNAL_WORDS.contains(&w.id.as_str()))
        .map(|w| VoiceAssistantWakeWord { id: w.id, wake_word: w.phrase, trained_languages: vec!["en".into()] })
        .collect();
    VoiceAssistantConfigurationResponse {
        available_wake_words: available,
        active_wake_words: voice.active_wake_words.lock().unwrap().clone(),
        max_active_wake_words: 2,
    }
}

fn handle_voice(kind: u32, payload: &[u8], voice: &VoiceLink, stream: &mut TcpStream, control: &dyn Fn(Control), subscribed: &mut bool) -> io::Result<()> {
    use crate::voice::FromApi;
    match kind {
        89 => {
            if let Ok(m) = SubscribeVoiceAssistantRequest::decode(payload) {
                *subscribed = m.subscribe;
                eprintln!("nspanel-app: Assist subscription {} (flags {})", m.subscribe, m.flags);
            }
        }
        121 => send(stream, 122, &wake_word_config(voice))?,
        123 => {
            if let Ok(m) = VoiceAssistantSetConfiguration::decode(payload) {
                let _ = voice.from_api.send(FromApi::SetWakeWords(m.active_wake_words.clone()));
                control(Control::Setting("wake_words".into(), m.active_wake_words.join(",")));
            }
        }
        91 => {
            if let Ok(m) = VoiceAssistantResponse::decode(payload) {
                let _ = voice.from_api.send(FromApi::Started { error: m.error });
            }
        }
        92 => {
            if let Ok(m) = VoiceAssistantEventResponse::decode(payload) {
                let data: Vec<(String, String)> = m.data.into_iter().map(|d| (d.name, d.value)).collect();
                // One line per pipeline event (follow-up decisions are checked from these).
                let short: Vec<String> = data
                    .iter()
                    .filter(|(k, _)| k != "tts_output" && k != "token")
                    .map(|(k, v)| format!("{k}={}", v.chars().take(100).collect::<String>()))
                    .collect();
                eprintln!("nspanel-app: Assist event {} {}", m.event_type, short.join(" "));
                let _ = voice.from_api.send(FromApi::Event { kind: m.event_type, data });
            }
        }
        106 => {
            if let Ok(m) = VoiceAssistantAudio::decode(payload) {
                if !m.data.is_empty() {
                    let _ = voice.from_api.send(FromApi::TtsAudio(m.data));
                }
            }
        }
        119 => {
            if let Ok(m) = VoiceAssistantAnnounceRequest::decode(payload) {
                let _ = voice.from_api.send(FromApi::Announce { media_id: m.media_id, text: m.text });
            }
        }
        115 => {
            if let Ok(m) = VoiceAssistantTimerEventResponse::decode(payload) {
                eprintln!(
                    "nspanel-app: timer event {} id={} name='{}' total={} left={} active={}",
                    m.event_type, m.timer_id, m.name, m.total_seconds, m.seconds_left, m.is_active
                );
                control(Control::Timer {
                    kind: m.event_type,
                    id: m.timer_id,
                    name: m.name,
                    total: m.total_seconds,
                    left: m.seconds_left,
                    active: m.is_active,
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn forward_voice(voice: &VoiceLink, stream: &mut TcpStream, subscribed: bool) -> io::Result<()> {
    use crate::voice::{FromApi, ToApi};
    while let Ok(msg) = voice.to_api.try_recv() {
        match msg {
            ToApi::Start { conversation_id, wake_word_phrase } => {
                if !subscribed {
                    let _ = voice.from_api.send(FromApi::Started { error: true });
                    continue;
                }
                send(stream, 90, &VoiceAssistantRequest {
                    start: true,
                    conversation_id,
                    flags: 1, // USE_VAD: Home Assistant detects the end of speech
                    audio_settings: Some(VoiceAssistantAudioSettings {
                        noise_suppression_level: 0,
                        auto_gain: 0,
                        volume_multiplier: 1.0,
                    }),
                    wake_word_phrase,
                    ..Default::default()
                })?;
            }
            ToApi::Audio(data) => {
                if subscribed {
                    send(stream, 106, &VoiceAssistantAudio { data, end: false, ..Default::default() })?;
                }
            }
            ToApi::Stop => {
                if subscribed {
                    send(stream, 90, &VoiceAssistantRequest { start: false, ..Default::default() })?;
                }
            }
            ToApi::AnnounceFinished { success } => {
                send(stream, 120, &VoiceAssistantAnnounceFinished { success })?;
            }
        }
    }
    Ok(())
}

fn serve(
    mut stream: TcpStream,
    state: &Arc<Mutex<DeviceState>>,
    notify: &Receiver<()>,
    control: &dyn Fn(Control),
    voice: &VoiceLink,
) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_millis(15)))?;
    let mut buf = Vec::new();
    let mut subscribed = false;
    let mut voice_subscribed = false;
    // Select options are only read by Home Assistant when it lists the
    // entities; when the dashboard list changes, reconnect so it lists again.
    let mut listed_dashboards: Option<Vec<String>> = None;
    let mut sent: HashMap<u32, Value> = HashMap::new();
    let mut last_rx = Instant::now();
    let mut ping_sent: Option<Instant> = None;
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                last_rx = Instant::now();
                ping_sent = None;
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e),
        }
        while let Some((kind, payload)) = take_frame(&mut buf)? {
            if std::env::var_os("NSPANEL_API_DEBUG").is_some() && kind != 106 {
                eprintln!("nspanel-app: API rx type {kind} ({} bytes)", payload.len());
            }
            let snapshot = state.lock().unwrap().clone();
            match kind {
                1 => {
                    let hello = HelloRequest::decode(&payload[..]).unwrap_or_default();
                    eprintln!("nspanel-app: ESPHome API client '{}' {}.{}", hello.client_info, hello.api_version_major, hello.api_version_minor);
                    send(&mut stream, 2, &HelloResponse {
                        api_version_major: 1,
                        api_version_minor: 12,
                        server_info: format!("nspanel-app {}", env!("CARGO_PKG_VERSION")),
                        name: "nspanel-pro".into(),
                    })?;
                }
                3 => send(&mut stream, 4, &AuthenticationResponse { invalid_password: false })?,
                5 => {
                    send(&mut stream, 6, &DisconnectResponse {})?;
                    return Ok(());
                }
                7 => send(&mut stream, 8, &PingResponse {})?,
                8 => {}
                9 => send(&mut stream, 10, &DeviceInfoResponse {
                    name: "nspanel-pro".into(),
                    friendly_name: "NSPanel Pro".into(),
                    mac_address: mac_address(),
                    esphome_version: "2026.10.0".into(),
                    compilation_time: env!("NSPANEL_BUILD_TIME").into(),
                    model: "NSPanel Pro 86 (open firmware)".into(),
                    manufacturer: "Sonoff".into(),
                    project_name: "bazsc20ne.nspanel-pro".into(),
                    project_version: env!("CARGO_PKG_VERSION").into(),
                    voice_assistant_feature_flags: VOICE_FEATURES,
                    ..Default::default()
                })?,
                11 => {
                    listed_dashboards = Some(dashboard_titles(&snapshot));
                    list_entities(&mut stream, &snapshot)?;
                }
                20 => {
                    subscribed = true;
                    sent.clear();
                }
                33 | 51 | 54 | 62 => handle_command(kind, &payload, &snapshot, control),
                89 | 91 | 92 | 106 | 115 | 119 | 121 | 123 => {
                    handle_voice(kind, &payload, voice, &mut stream, control, &mut voice_subscribed)?
                }
                // Logs, Home Assistant services/states subscriptions, time: not used.
                _ => {}
            }
        }
        forward_voice(voice, &mut stream, voice_subscribed)?;
        // Push changed states.
        let changed = notify.try_iter().count() > 0;
        if subscribed && (changed || sent.is_empty()) {
            let snapshot = state.lock().unwrap().clone();
            if listed_dashboards.as_ref().is_some_and(|l| *l != dashboard_titles(&snapshot)) {
                eprintln!("nspanel-app: dashboard list changed, asking Home Assistant to reconnect");
                return Ok(());
            }
            for &(id, key, ..) in ENTITIES {
                if let Some(v) = value_of(id, &snapshot) {
                    if sent.get(&key) != Some(&v) {
                        send_state(&mut stream, id, key, &v)?;
                        sent.insert(key, v);
                    }
                }
            }
        }
        // Keep-alive towards Home Assistant.
        if ping_sent.is_none() && last_rx.elapsed() > Duration::from_secs(60) {
            send(&mut stream, 7, &PingRequest {})?;
            ping_sent = Some(Instant::now());
        }
        if ping_sent.is_some_and(|t| t.elapsed() > Duration::from_secs(30)) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "no answer to ping"));
        }
    }
}

pub fn spawn(
    state: Arc<Mutex<DeviceState>>,
    notify: Receiver<()>,
    voice: VoiceLink,
    control: impl Fn(Control) + Send + 'static,
) {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("nspanel-app: ESPHome API port {PORT}: {e}");
                return;
            }
        };
        // Home Assistant reads the entity list once per connection: wait (at
        // most 20 s) for the dashboard list, so the "Dashboard" select is
        // complete from the start and needs no reconnect. Home Assistant's
        // connection waits in the listen backlog meanwhile.
        let started = Instant::now();
        while state.lock().unwrap().dashboards.is_empty() && started.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(200));
        }
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
            eprintln!("nspanel-app: ESPHome API connection from {peer}");
            if let Err(e) = serve(stream, &state, &notify, &control, &voice) {
                eprintln!("nspanel-app: ESPHome API connection ended: {e}");
            }
            // A conversation in progress cannot finish without Home Assistant.
            let _ = voice.from_api.send(crate::voice::FromApi::Event {
                kind: 0,
                data: vec![("message".into(), "Home Assistant disconnected".into())],
            });
            while voice.to_api.try_recv().is_ok() {}
        }
    });
}
