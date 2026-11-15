//! Home Assistant WebSocket client (own thread): login, dashboard config,
//! live states of the dashboard's entities (`subscribe_entities`, compressed
//! updates), weather forecasts, service calls from the UI, and the PanelOS
//! integration: its settings, show requests and entity commands come in, the
//! panel's device states go out (`panelos/report`, see `panel_device.rs`).

use std::collections::{HashMap, HashSet};
use std::net::TcpStream;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::auth::{Auth, TokenError};

#[derive(Debug, Clone, Default)]
pub struct EntityState {
    pub state: String,
    pub attrs: Map<String, Value>,
}

pub enum Command {
    CallService { domain: String, service: String, entity_id: String, data: Value },
    /// Start a new session (dashboard or extra entities changed).
    Reload,
    /// Ask for the list of dashboards (answered with Event::Dashboards).
    ListDashboards,
    /// Picture of the screen (PNG, base64) asked for by the PanelOS page.
    Screen(String),
}

pub enum Event {
    Status { text: String, connected: bool },
    NeedPairing,
    Dashboard(Value),
    States(Vec<(String, Option<EntityState>)>),
    /// (url_path, title); "" = the default dashboard.
    Dashboards(Vec<(String, String)>),
    /// Assistant mode settings from the PanelOS integration; None = the
    /// integration is not installed (or has no entry for this panel).
    Panel(Option<Value>),
    /// show_view / show_message request from the PanelOS integration.
    Show(Value),
    /// Command from an entity of the panel's device (PanelOS integration).
    Command { key: String, value: Value },
    /// Weather forecast (hourly or daily) of the forecast entity.
    Forecast { hourly: bool, items: Vec<Value> },
}

/// The panel's name for the PanelOS integration (its ESPHome name).
pub const PANEL_ID: &str = "nspanel-pro";

/// Settings the UI thread can change while the client runs.
#[derive(Default)]
pub struct Shared {
    pub dashboard: std::sync::Mutex<String>,
    pub extra_entities: std::sync::Mutex<Vec<String>>,
    /// Weather entity whose forecasts are followed ("" = none).
    pub forecast_entity: std::sync::Mutex<String>,
    /// What the panel's device in Home Assistant shows; filled by the UI
    /// thread (the ESPHome API reads the voice part of it).
    pub device: Arc<std::sync::Mutex<crate::esphome::DeviceState>>,
}

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// Every entity id mentioned in a dashboard config ("entity" keys, "entities" lists).
pub fn entities_in(config: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut HashSet<String>) {
        match v {
            Value::Object(map) => {
                for (k, val) in map {
                    match (k.as_str(), val) {
                        ("entity", Value::String(s)) => {
                            out.insert(s.clone());
                        }
                        ("entities", Value::Array(items)) => {
                            for item in items {
                                match item {
                                    Value::String(s) => {
                                        out.insert(s.clone());
                                    }
                                    other => walk(other, out),
                                }
                            }
                        }
                        _ => walk(val, out),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let mut set = HashSet::new();
    walk(config, &mut set);
    let mut v: Vec<String> = set.into_iter().filter(|e| e.contains('.')).collect();
    v.sort();
    v
}

fn read_json(ws: &mut Socket) -> Result<Option<Value>, String> {
    match ws.read() {
        Ok(Message::Text(t)) => serde_json::from_str(t.as_str()).map(Some).map_err(|e| e.to_string()),
        Ok(Message::Close(_)) => Err("connection closed".into()),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(e))
            if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
        {
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

fn send_json(ws: &mut Socket, v: &Value) -> Result<(), String> {
    ws.send(Message::text(v.to_string())).map_err(|e| e.to_string())
}

fn set_timeout(ws: &mut Socket, t: Option<Duration>) {
    if let MaybeTlsStream::Plain(s) = ws.get_mut() {
        let _ = s.set_read_timeout(t);
    }
}

/// Applies one `subscribe_entities` event to the state map; returns the changed entities.
fn apply_compressed(states: &mut HashMap<String, EntityState>, ev: &Value) -> Vec<(String, Option<EntityState>)> {
    let mut changed = Vec::new();
    if let Some(added) = ev.get("a").and_then(Value::as_object) {
        for (id, v) in added {
            let st = EntityState {
                state: v.get("s").and_then(Value::as_str).unwrap_or_default().to_string(),
                attrs: v.get("a").and_then(Value::as_object).cloned().unwrap_or_default(),
            };
            states.insert(id.clone(), st.clone());
            changed.push((id.clone(), Some(st)));
        }
    }
    if let Some(diffs) = ev.get("c").and_then(Value::as_object) {
        for (id, d) in diffs {
            let st = states.entry(id.clone()).or_default();
            if let Some(plus) = d.get("+") {
                if let Some(s) = plus.get("s").and_then(Value::as_str) {
                    st.state = s.to_string();
                }
                if let Some(a) = plus.get("a").and_then(Value::as_object) {
                    for (k, v) in a {
                        st.attrs.insert(k.clone(), v.clone());
                    }
                }
            }
            if let Some(minus) = d.get("-").and_then(|m| m.get("a")).and_then(Value::as_array) {
                for k in minus.iter().filter_map(Value::as_str) {
                    st.attrs.remove(k);
                }
            }
            changed.push((id.clone(), Some(st.clone())));
        }
    }
    if let Some(removed) = ev.get("r").and_then(Value::as_array) {
        for id in removed.iter().filter_map(Value::as_str) {
            states.remove(id);
            changed.push((id.to_string(), None));
        }
    }
    changed
}

fn session(
    ws_url: &str,
    shared: &Shared,
    auth: &Auth,
    commands: &Receiver<Command>,
    emit: &dyn Fn(Event),
) -> Result<(), String> {
    let dashboard_name = shared.dashboard.lock().unwrap().clone();
    let dashboard = dashboard_name.as_str();
    let url_path = if dashboard.is_empty() { Value::Null } else { Value::String(dashboard.to_string()) };
    let token = match auth.access_token() {
        Ok(t) => t,
        Err(TokenError::NotPaired) => {
            emit(Event::NeedPairing);
            return Err("not paired".into());
        }
        Err(TokenError::Retry(e)) => return Err(e),
    };

    let (mut ws, response) = tungstenite::connect(ws_url).map_err(|e| format!("connect: {e}"))?;
    // No battery-backed clock: until NTP has run, take Home Assistant's time.
    if let Some(date) = response.headers().get("date").and_then(|v| v.to_str().ok()) {
        if crate::clock::set_from_http_date(date) {
            eprintln!("nspanel-app: clock set from Home Assistant ({date})");
        }
    }
    set_timeout(&mut ws, Some(Duration::from_secs(15)));
    let hello = read_json(&mut ws)?.ok_or("no greeting")?;
    if hello["type"] != "auth_required" {
        return Err(format!("unexpected greeting: {hello}"));
    }
    send_json(&mut ws, &json!({"type": "auth", "access_token": token}))?;
    let answer = read_json(&mut ws)?.ok_or("no auth answer")?;
    if answer["type"] != "auth_ok" {
        auth.invalidate_access();
        return Err(format!("login rejected: {}", answer["message"]));
    }

    let mut next_id: u64 = 1;
    let dash_id = next_id;
    send_json(&mut ws, &json!({"id": dash_id, "type": "lovelace/config", "url_path": url_path}))?;
    let config = loop {
        let msg = read_json(&mut ws)?.ok_or("no dashboard answer")?;
        if msg["id"] == dash_id {
            if msg["success"] != true {
                return Err(format!("dashboard '{dashboard}': {}", msg["error"]["message"]));
            }
            break msg["result"].clone();
        }
    };
    let mut entities = entities_in(&config);
    for extra in shared.extra_entities.lock().unwrap().iter() {
        if !extra.is_empty() && !entities.contains(extra) {
            entities.push(extra.clone());
        }
    }
    emit(Event::Dashboard(config));

    next_id += 1;
    let sub_id = next_id;
    send_json(&mut ws, &json!({"id": sub_id, "type": "subscribe_entities", "entity_ids": entities}))?;
    // Dashboard edits in Home Assistant: reload the config (and the entity list).
    next_id += 1;
    let lovelace_id = next_id;
    send_json(&mut ws, &json!({"id": lovelace_id, "type": "subscribe_events", "event_type": "lovelace_updated"}))?;
    // Assistant mode settings and show requests (own integration; an
    // "unknown command" error means it is not installed).
    next_id += 1;
    let mut panel_id = next_id;
    send_json(&mut ws, &json!({"id": panel_id, "type": "panelos/subscribe", "panel_id": PANEL_ID}))?;
    // Retried every minute while the integration is missing, so installing
    // it needs no panel restart.
    let mut panel_retry_at: Option<Instant> = None;
    // Weather forecasts (no longer a state attribute in Home Assistant).
    let forecast_entity = shared.forecast_entity.lock().unwrap().clone();
    let mut forecast_ids: Vec<(u64, bool)> = Vec::new();
    if !forecast_entity.is_empty() {
        for (kind, hourly) in [("hourly", true), ("daily", false)] {
            next_id += 1;
            forecast_ids.push((next_id, hourly));
            send_json(
                &mut ws,
                &json!({"id": next_id, "type": "weather/subscribe_forecast", "entity_id": forecast_entity, "forecast_type": kind}),
            )?;
        }
    }
    emit(Event::Status { text: "Connected".into(), connected: true });

    set_timeout(&mut ws, Some(Duration::from_millis(150)));
    let mut states: HashMap<String, EntityState> = HashMap::new();
    let mut last_rx = Instant::now();
    let mut ping_sent: Option<Instant> = None;
    let mut dashboards_request: Option<u64> = None;
    // The panel's device (PanelOS integration): reported once the
    // subscription is accepted, then only what changed.
    let mut report = Report::default();
    loop {
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                Command::CallService { domain, service, entity_id, data } => {
                    next_id += 1;
                    send_json(
                        &mut ws,
                        &json!({"id": next_id, "type": "call_service", "domain": domain, "service": service,
                                "target": {"entity_id": entity_id}, "service_data": data}),
                    )?;
                }
                Command::Reload => return Ok(()),
                Command::ListDashboards => {
                    next_id += 1;
                    dashboards_request = Some(next_id);
                    send_json(&mut ws, &json!({"id": next_id, "type": "lovelace/dashboards/list"}))?;
                }
                Command::Screen(png) if report.linked => {
                    next_id += 1;
                    send_json(&mut ws, &json!({"id": next_id, "type": "panelos/screen", "panel_id": PANEL_ID, "png": png}))?;
                }
                Command::Screen(_) => {}
            }
        }

        match read_json(&mut ws)? {
            Some(msg) => {
                last_rx = Instant::now();
                ping_sent = None;
                let report_answer = msg["type"] == "result" && report.answered(msg["id"].as_u64().unwrap_or(0));
                match msg["type"].as_str() {
                    Some("result") if report_answer => {
                        if msg["success"] == false {
                            // An older integration without panelos/report.
                            eprintln!("nspanel-app: device report not accepted: {}", msg["error"]["message"]);
                            report.linked = false;
                        }
                    }
                    Some("event") if msg["id"] == sub_id => {
                        let changed = apply_compressed(&mut states, &msg["event"]);
                        if !changed.is_empty() {
                            emit(Event::States(changed));
                        }
                    }
                    Some("event") if msg["id"] == lovelace_id => {
                        let changed = msg["event"]["data"]["url_path"].as_str().unwrap_or("");
                        if changed == dashboard {
                            // Start a fresh session: new config, new subscriptions.
                            return Ok(());
                        }
                    }
                    Some("event") if msg["id"] == panel_id => {
                        let ev = &msg["event"];
                        if let Some(config) = ev.get("config") {
                            let found = config.as_object().is_some_and(|m| !m.is_empty());
                            emit(Event::Panel(found.then(|| config.clone())));
                        }
                        if let Some(show) = ev.get("show") {
                            emit(Event::Show(show.clone()));
                        }
                        if let Some(key) = ev["command"]["key"].as_str() {
                            emit(Event::Command { key: key.to_string(), value: ev["command"]["value"].clone() });
                        }
                    }
                    Some("result") if msg["id"] == panel_id && msg["success"] == false => {
                        emit(Event::Panel(None));
                        panel_retry_at = Some(Instant::now() + Duration::from_secs(60));
                    }
                    Some("result") if msg["id"] == panel_id => report = Report { linked: true, ..Default::default() },
                    Some("event") if forecast_ids.iter().any(|(id, _)| msg["id"] == *id) => {
                        let hourly = forecast_ids.iter().any(|(id, h)| msg["id"] == *id && *h);
                        let items = msg["event"]["forecast"].as_array().cloned().unwrap_or_default();
                        emit(Event::Forecast { hourly, items });
                    }
                    Some("result") if msg["success"] == false && forecast_ids.iter().any(|(id, _)| msg["id"] == *id) => {
                        // This weather entity does not offer this forecast type.
                    }
                    Some("result") if Some(msg["id"].as_u64().unwrap_or(0)) == dashboards_request => {
                        let mut list = vec![(String::new(), "Overview".to_string())];
                        for d in msg["result"].as_array().into_iter().flatten() {
                            if let Some(url) = d["url_path"].as_str() {
                                let title = d["title"].as_str().unwrap_or(url).trim().to_string();
                                list.push((url.to_string(), title));
                            }
                        }
                        emit(Event::Dashboards(list));
                    }
                    Some("result") if msg["success"] == false => {
                        eprintln!("nspanel-app: request {} failed: {}", msg["id"], msg["error"]);
                    }
                    _ => {}
                }
            }
            None => {
                let _ = ws.flush();
            }
        }

        if let Some(msg) = report.next(&shared.device) {
            next_id += 1;
            let mut msg = msg;
            msg["id"] = next_id.into();
            send_json(&mut ws, &msg)?;
            report.sent(next_id);
        }

        if panel_retry_at.is_some_and(|t| Instant::now() >= t) {
            panel_retry_at = None;
            next_id += 1;
            panel_id = next_id;
            send_json(&mut ws, &json!({"id": panel_id, "type": "panelos/subscribe", "panel_id": PANEL_ID}))?;
        }

        // Keep-alive: Home Assistant answers {"type":"ping"} with a pong. A
        // vanished server is noticed within about 30 s.
        if ping_sent.is_none() && last_rx.elapsed() > Duration::from_secs(20) {
            next_id += 1;
            send_json(&mut ws, &json!({"id": next_id, "type": "ping"}))?;
            ping_sent = Some(Instant::now());
        }
        if let Some(t) = ping_sent {
            if t.elapsed() > Duration::from_secs(10) {
                return Err("no answer to ping".into());
            }
        }
    }
}

/// What the panel's device in Home Assistant was last told, so a report
/// carries only changes. Checked at most every 200 ms.
#[derive(Default)]
struct Report {
    /// The integration accepted the panel's subscription.
    linked: bool,
    device_sent: bool,
    dashboard: Option<String>,
    states: Map<String, Value>,
    options: Map<String, Value>,
    pending: HashSet<u64>,
    checked: Option<Instant>,
}

impl Report {
    fn next(&mut self, device: &std::sync::Mutex<crate::esphome::DeviceState>) -> Option<Value> {
        if !self.linked || self.checked.is_some_and(|t| t.elapsed() < Duration::from_millis(200)) {
            return None;
        }
        self.checked = Some(Instant::now());
        let snapshot = device.lock().unwrap().clone();
        if !snapshot.ready {
            return None;
        }
        let changed = |now: Map<String, Value>, sent: &mut Map<String, Value>| {
            let diff: Map<String, Value> = now.into_iter().filter(|(k, v)| sent.get(k) != Some(v)).collect();
            sent.extend(diff.clone());
            diff
        };
        let states = changed(crate::panel_device::states(&snapshot), &mut self.states);
        let options = changed(crate::panel_device::options(&snapshot), &mut self.options);
        let dashboard = (self.dashboard.as_deref() != Some(snapshot.web_dashboard.as_str())).then(|| snapshot.web_dashboard.clone());
        if states.is_empty() && options.is_empty() && dashboard.is_none() && self.device_sent {
            return None;
        }
        let mut msg = json!({"type": "panelos/report", "panel_id": PANEL_ID});
        if let Some(d) = dashboard {
            msg["dashboard"] = d.clone().into();
            self.dashboard = Some(d);
        }
        if !self.device_sent {
            msg["device"] = crate::panel_device::device_info(&crate::esphome::mac_address().to_lowercase());
            self.device_sent = true;
        }
        if !options.is_empty() {
            msg["options"] = options.into();
        }
        if !states.is_empty() {
            msg["states"] = states.into();
        }
        Some(msg)
    }

    fn sent(&mut self, id: u64) {
        self.pending.insert(id);
    }

    /// True (once) for the answer to one of the reports.
    fn answered(&mut self, id: u64) -> bool {
        self.pending.remove(&id)
    }
}

/// Why a session failed, for the status line and the retry delay.
#[derive(Clone, Copy, PartialEq)]
enum Failure {
    /// No network (Wi-Fi not up yet, or gone).
    Offline,
    /// The network is there, Home Assistant does not answer.
    Unreachable,
    /// Home Assistant answered with an error (login, dashboard, …).
    Other,
}

fn classify(e: &str) -> Failure {
    if e.contains("os error 101") || e.contains("Network is unreachable") {
        Failure::Offline
    } else if e.starts_with("connect:")
        || e.contains("Connection Failed")
        || e.contains("os error 111")
        || e.contains("os error 113")
        || e.contains("timed out")
        || e.contains("Connection reset")
    {
        Failure::Unreachable
    } else {
        Failure::Other
    }
}

/// Runs the client for good: Home Assistant may be away for hours (power cut,
/// server down), and the panel must come back by itself.
///
/// Retry delays: without network every 0.5 s during the first minute after
/// the start (Wi-Fi comes up a few seconds after the app, and the first screen
/// waits for this connection), then every 5 s; while Home Assistant does not
/// answer 2, 4, 8, then every 10 s. A failure is logged when its cause
/// changes, not on every try, and the recovery with the length of the outage.
pub fn spawn(
    ws_url: String,
    shared: Arc<Shared>,
    auth: Arc<Auth>,
    commands: Receiver<Command>,
    emit: impl Fn(Event) + Send + 'static,
) {
    std::thread::spawn(move || {
        let client_started = Instant::now();
        let mut delay = 2;
        let mut reloading = false;
        // Current outage: since when, and the last logged cause. Cells: the
        // session reports its connection through the emit callback.
        let failing_since: std::cell::Cell<Option<Instant>> = std::cell::Cell::new(None);
        let last_logged: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
        loop {
            if !reloading && failing_since.get().is_none() {
                emit(Event::Status { text: "Connecting to Home Assistant…".into(), connected: false });
            }
            let started = Instant::now();
            let result = session(&ws_url, &shared, &auth, &commands, &|event| {
                if let Event::Status { connected: true, .. } = event {
                    if let Some(since) = failing_since.take() {
                        eprintln!("nspanel-app: Home Assistant connected again after {} s", since.elapsed().as_secs());
                    }
                    last_logged.borrow_mut().take();
                }
                emit(event)
            });
            // Ok means the dashboard or the settings changed: reconnect at once, quietly.
            reloading = result.is_ok();
            if reloading {
                eprintln!("nspanel-app: reloading the dashboard");
                continue;
            }
            let Err(e) = &result else { continue };
            if !auth.is_paired() {
                eprintln!("nspanel-app: Home Assistant session ended: {e}");
                emit(Event::NeedPairing);
                // Wait until a new login is stored.
                while !auth.is_paired() {
                    std::thread::sleep(Duration::from_secs(1));
                }
                continue;
            }
            let failure = classify(e);
            let cause = match failure {
                Failure::Offline => "no network".to_string(),
                _ => e.clone(),
            };
            if last_logged.borrow().as_deref() != Some(cause.as_str()) {
                eprintln!("nspanel-app: Home Assistant session ended: {e}");
                *last_logged.borrow_mut() = Some(cause);
            }
            if failing_since.get().is_none() {
                failing_since.set(Some(Instant::now()));
            }
            let text = match failure {
                Failure::Offline => "Waiting for the network…".to_string(),
                Failure::Unreachable => "Home Assistant not reachable, retrying…".to_string(),
                Failure::Other => format!("Home Assistant: {e}"),
            };
            emit(Event::Status { text, connected: false });
            if failure == Failure::Offline {
                let wait = if client_started.elapsed() < Duration::from_secs(60) { 500 } else { 5000 };
                std::thread::sleep(Duration::from_millis(wait));
                continue;
            }
            if started.elapsed() > Duration::from_secs(60) {
                delay = 2;
            }
            std::thread::sleep(Duration::from_secs(delay));
            delay = (delay * 2).min(10);
        }
    });
}
