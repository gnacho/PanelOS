//! Thermostat page: the climate entities picked for it, one on the dial at a
//! time (the device chip opens them all), its modes, target and options.

use serde_json::{json, Value};
use slint::{Color, ModelRc, SharedString, VecModel};

use crate::ha::EntityState;
use crate::{icons, with_state, AppWindow, ClimateDev, ClimateRoom};

/// HA's hvac mode → the page's mode code.
fn mode_code(mode: &str) -> Option<i32> {
    Some(match mode {
        "off" => 0,
        "heat" => 1,
        "cool" => 2,
        "heat_cool" | "auto" => 3,
        "dry" => 4,
        "fan_only" => 5,
        _ => return None,
    })
}

fn words(s: &str) -> String {
    let s = s.replace('_', " ");
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn strings(a: Option<&Value>) -> Vec<String> {
    a.and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn model(list: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(list.iter().map(|s| SharedString::from(words(s))).collect::<Vec<_>>()))
}

fn caps(list: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(list.iter().map(|s| SharedString::from(words(s).to_uppercase())).collect::<Vec<_>>()))
}

/// One device for the dial (and the other looks).
pub fn device(id: &str, st: Option<&EntityState>) -> ClimateDev {
    let attr = |k: &str| st.and_then(|s| s.attrs.get(k));
    let num = |k: &str| attr(k).and_then(Value::as_f64);
    let state = st.map(|s| s.state.as_str()).unwrap_or("unavailable");
    let hvac = strings(attr("hvac_modes"));
    let modes: Vec<i32> = hvac.iter().filter_map(|m| mode_code(m)).collect();
    let current_code = mode_code(state).unwrap_or(0);
    let mode = modes.iter().position(|&m| m == current_code).unwrap_or(0) as i32;
    let action = match attr("hvac_action").and_then(Value::as_str).unwrap_or("") {
        "heating" | "preheating" => 1,
        "cooling" => 2,
        "drying" => 4,
        "fan" => 5,
        _ => 0,
    };
    let target = num("temperature");
    let show_target = target.is_some() && matches!(current_code, 1 | 2 | 3);
    let label = match (current_code, action) {
        (0, _) => "Off".to_string(),
        (_, 1) => "Heating to".into(),
        (_, 2) => "Cooling to".into(),
        (_, 4) => "Drying".into(),
        (_, 5) => "Fan".into(),
        (c, _) => format!("{} · Idle", ["Off", "Heat", "Cool", "Auto", "Dry", "Fan"][c.clamp(0, 5) as usize]),
    };
    let fans = strings(attr("fan_modes"));
    let swings = strings(attr("swing_modes"));
    let presets = strings(attr("preset_modes"));
    let fan = attr("fan_mode").and_then(Value::as_str).unwrap_or("").to_string();
    let swing = attr("swing_mode").and_then(Value::as_str).unwrap_or("").to_string();
    let preset = attr("preset_mode").and_then(Value::as_str).filter(|p| *p != "none").unwrap_or("").to_string();
    let fan_level = match fan.as_str() {
        "low" | "1" => 1,
        "medium_low" | "2" => 2,
        "medium" | "mid" | "3" => 3,
        "medium_high" | "4" => 4,
        "high" | "5" => 5,
        _ => 0,
    };
    let cools = hvac.iter().any(|m| m == "cool");
    let icon = icons::glyph(if cools { "air-conditioner" } else { "radiator" }).map(String::from).unwrap_or_default();
    let step = num("target_temp_step").unwrap_or(if cools { 1.0 } else { 0.5 });
    ClimateDev {
        name: attr("friendly_name").and_then(Value::as_str).unwrap_or(id).into(),
        icon: icon.into(),
        mode,
        modes: ModelRc::new(VecModel::from(modes)),
        action,
        label: label.into(),
        target: target.unwrap_or(0.0) as f32,
        show_target,
        current: num("current_temperature").unwrap_or(0.0) as f32,
        humidity: num("current_humidity").unwrap_or(-1.0) as f32,
        tmin: num("min_temp").unwrap_or(7.0) as f32,
        tmax: num("max_temp").unwrap_or(35.0) as f32,
        step: step as f32,
        fan: if fans.is_empty() { SharedString::new() } else { words(&fan).into() },
        swing: if swings.is_empty() { SharedString::new() } else { words(&swing).into() },
        preset: words(&preset).into(),
        has_preset: !presets.is_empty(),
        fans: model(&fans),
        swings: model(&swings),
        presets: model(&presets),
        fan_level,
        fan_i: fans.iter().position(|f| *f == fan).map(|i| i as i32).unwrap_or(-1),
        swing_i: swings.iter().position(|f| *f == swing).map(|i| i as i32).unwrap_or(-1),
        preset_i: presets.iter().position(|f| *f == preset).map(|i| i as i32).unwrap_or(-1),
        fan_caps: caps(&fans),
        swing_caps: caps(&swings),
        preset_caps: caps(&presets),
    }
}

/// The rooms view: one card per device (its name, the room's temperature
/// and humidity as the device measures them, what it does).
fn rooms(devices: &[ClimateDev], sel: usize) -> Vec<ClimateRoom> {
    let tints = [0x9aa0a6u32, 0xff7a3d, 0x3d9bff, 0x2fd27a, 0xc9a227, 0x7fd1ae];
    devices
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let code = slint::Model::row_data(&d.modes, d.mode as usize).unwrap_or(0);
            let line = if d.show_target { format!("{} {:.0}°", d.label, d.target) } else { d.label.to_string() };
            let c = tints[code.clamp(0, 5) as usize];
            ClimateRoom {
                name: d.name.clone(),
                temp: format!("{:.1}°", d.current).into(),
                hum: if d.humidity >= 0.0 { format!("{:.0} %", d.humidity).into() } else { SharedString::new() },
                outside: false,
                dev: ModelRc::new(VecModel::from(vec![SharedString::from(line)])),
                dev_icon: ModelRc::new(VecModel::from(vec![d.icon.clone()])),
                dev_tint: ModelRc::new(VecModel::from(vec![Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)])),
                sel: if i == sel { 0 } else { -1 },
            }
        })
        .collect()
}

/// The thermostat page from the states (only when the panel has one).
pub fn refresh(ui: &AppWindow, s: &mut crate::UiState) {
    let pages = ui.get_pages();
    let Some(slot) =
        (0..slint::Model::row_count(&pages)).filter_map(|i| slint::Model::row_data(&pages, i)).find(|p| p.kind == "thermostat")
    else {
        return;
    };
    ui.set_thermo_look(slot.variant);
    let entities = crate::page_entities(s, "thermostat");
    let devices: Vec<ClimateDev> = entities
        .iter()
        .map(|id| {
            let mut d = device(id, s.states.get(id));
            // a value just set stays shown until Home Assistant reports it (or 5 s pass)
            if let Some(t) = pending(id) {
                if (d.target - t).abs() < 0.01 {
                    PENDING.with(|p| p.borrow_mut().remove(id));
                } else {
                    d.target = t;
                }
            }
            d
        })
        .collect();
    if devices.is_empty() {
        return;
    }
    let sel = (ui.get_thermo_sel().max(0) as usize).min(devices.len() - 1);
    let rooms = rooms(&devices, sel);
    if let Some(m) = crate::pages::update_rows(&ui.get_thermo_devices(), devices) {
        ui.set_thermo_devices(m);
    }
    if let Some(m) = crate::pages::update_rows(&ui.get_thermo_rooms(), rooms) {
        ui.set_thermo_rooms(m);
    }
    ui.set_thermo_sel(sel as i32);
}

thread_local! {
    /// Targets set on the panel that Home Assistant has not reported back yet: (value, when).
    static PENDING: std::cell::RefCell<std::collections::HashMap<String, (f32, std::time::Instant)>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

fn pending(id: &str) -> Option<f32> {
    PENDING.with(|p| p.borrow().get(id).filter(|(_, at)| at.elapsed() < std::time::Duration::from_secs(5)).map(|(t, _)| *t))
}

fn set_pending(id: &str, t: f32) {
    PENDING.with(|p| p.borrow_mut().insert(id.to_string(), (t, std::time::Instant::now())));
}

/// Taps on the page (`thermo.*`).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let (entities, states) = with_state(|s| (crate::page_entities(s, "thermostat"), s.states.clone()));
    let sel = ui.get_thermo_sel().max(0) as usize;
    let Some(id) = entities.get(sel).cloned() else { return };
    let st: Option<&EntityState> = states.get(&id);
    let attr = |k: &str| st.and_then(|s| s.attrs.get(k));
    let n: usize = value.parse().unwrap_or(0);
    match what {
        "thermo.minus" | "thermo.plus" => {
            let d = device(&id, st);
            // quick presses count one by one: the next step starts from the value just set, not from Home Assistant's
            // state that has not caught up yet
            let base = pending(&id).unwrap_or(d.target);
            let t = (base + if what == "thermo.plus" { d.step } else { -d.step }).clamp(d.tmin, d.tmax);
            set_pending(&id, t);
            call("set_temperature", &id, json!({"temperature": t}));
            // Shown at once; Home Assistant's new state follows.
            let devs = ui.get_thermo_devices();
            if let Some(mut cur) = slint::Model::row_data(&devs, sel) {
                cur.target = t;
                slint::Model::set_row_data(&devs, sel, cur);
            }
        }
        // a value from the wall look's knob, to the device's step
        "thermo.target" => {
            let d = device(&id, st);
            let v: f32 = value.parse().unwrap_or(d.target);
            let t = ((v / d.step).round() * d.step).clamp(d.tmin, d.tmax);
            set_pending(&id, t);
            call("set_temperature", &id, json!({"temperature": t}));
            let devs = ui.get_thermo_devices();
            if let Some(mut cur) = slint::Model::row_data(&devs, sel) {
                cur.target = t;
                slint::Model::set_row_data(&devs, sel, cur);
            }
        }
        "thermo.mode" => {
            let hvac: Vec<String> = strings(attr("hvac_modes")).into_iter().filter(|m| mode_code(m).is_some()).collect();
            if let Some(mode) = hvac.get(n) {
                call("set_hvac_mode", &id, json!({"hvac_mode": mode}));
            }
        }
        "thermo.fan" | "thermo.swing" | "thermo.preset" => {
            let (key, service, field) = match what {
                "thermo.fan" => ("fan_modes", "set_fan_mode", "fan_mode"),
                "thermo.swing" => ("swing_modes", "set_swing_mode", "swing_mode"),
                _ => ("preset_modes", "set_preset_mode", "preset_mode"),
            };
            if let Some(v) = strings(attr(key)).get(n) {
                call(service, &id, json!({ field: v }));
            }
        }
        // The remote's keys: power off / on, the next mode (not off), the next
        // fan or swing step, the next preset (one preset: on / off).
        "thermo.key" => {
            let state = st.map(|s| s.state.as_str()).unwrap_or("off");
            let next = |list: Vec<String>, cur: &str| -> Option<String> {
                if list.is_empty() {
                    return None;
                }
                let i = list.iter().position(|v| v == cur).map(|i| (i + 1) % list.len()).unwrap_or(0);
                Some(list[i].clone())
            };
            match n {
                0 => call(if state == "off" { "turn_on" } else { "turn_off" }, &id, json!({})),
                1 => {
                    let modes: Vec<String> = strings(attr("hvac_modes")).into_iter().filter(|m| m != "off" && mode_code(m).is_some()).collect();
                    if let Some(m) = next(modes, state) {
                        call("set_hvac_mode", &id, json!({ "hvac_mode": m }));
                    }
                }
                2 => {
                    if let Some(f) = next(strings(attr("fan_modes")), attr("fan_mode").and_then(Value::as_str).unwrap_or("")) {
                        call("set_fan_mode", &id, json!({ "fan_mode": f }));
                    }
                }
                3 => {
                    if let Some(f) = next(strings(attr("swing_modes")), attr("swing_mode").and_then(Value::as_str).unwrap_or("")) {
                        call("set_swing_mode", &id, json!({ "swing_mode": f }));
                    }
                }
                _ => {
                    if let Some(p) = next(strings(attr("preset_modes")), attr("preset_mode").and_then(Value::as_str).unwrap_or("")) {
                        call("set_preset_mode", &id, json!({ "preset_mode": p }));
                    }
                }
            }
        }
        "thermo.sheet" => ui.set_thermo_sheet(true),
        "thermo.sheet-close" => ui.set_thermo_sheet(false),
        "thermo.picker" => ui.set_thermo_picker(true),
        "thermo.picker-close" => ui.set_thermo_picker(false),
        "thermo.room" => {
            if n < entities.len() {
                ui.set_thermo_sel(n as i32);
            }
            ui.set_thermo_picker(false);
            with_state(|s| refresh(ui, s));
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled)"),
    }
}

fn call(service: &str, id: &str, data: Value) {
    crate::call("climate", service, id, data);
}

