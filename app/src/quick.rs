//! Quick controls page: the entities picked for it in the integration, as
//! tiles (tap toggles, a long press opens the details sheet), a mixing desk
//! (faders) or a flight deck (levers).

use std::collections::HashMap;

use serde_json::{json, Value};
use slint::{Color, ModelRc, SharedString, VecModel};

use crate::ha::EntityState;
use crate::{icons, texts, with_state, AppWindow, QuickTile};

/// The tiles for the page's entities, in their order (the grid of the 4 x 4
/// look: wide cards for lights and covers, icon cells for the rest).
pub fn tiles(states: &HashMap<String, EntityState>, entities: &[String], accent: Color) -> Vec<QuickTile> {
    let (mut col, mut row) = (0i32, 0i32);
    entities
        .iter()
        .map(|id| {
            let st = states.get(id);
            let domain = id.split('.').next().unwrap_or("");
            let state = st.map(|s| s.state.as_str()).unwrap_or("unavailable");
            let attr = |k: &str| st.and_then(|s| s.attrs.get(k));
            let num = |k: &str| attr(k).and_then(Value::as_f64);
            let name = attr("friendly_name").and_then(Value::as_str).unwrap_or(id).to_string();
            let gone = matches!(state, "unavailable" | "unknown");
            let kind = match domain {
                "light" => 0,
                "cover" => 2,
                "climate" => 3,
                "vacuum" => 4,
                _ => 1,
            };
            let on = match domain {
                "cover" => matches!(state, "open" | "opening"),
                "climate" => state != "off" && !gone,
                "vacuum" => matches!(state, "cleaning" | "returning"),
                _ => matches!(state, "on" | "playing" | "home"),
            };
            let (dim, value, short) = match domain {
                "light" if num("brightness").is_some() || on => {
                    let b = num("brightness").unwrap_or(0.0) / 255.0;
                    (true, b as f32, format!("{:.0} %", b * 100.0))
                }
                "cover" if num("current_position").is_some() => {
                    let p = num("current_position").unwrap_or(0.0);
                    (true, (p / 100.0) as f32, format!("{p:.0} %"))
                }
                "climate" if num("temperature").is_some() => {
                    let t = num("temperature").unwrap_or(0.0);
                    let (lo, hi) = (num("min_temp").unwrap_or(7.0), num("max_temp").unwrap_or(35.0));
                    (true, ((t - lo) / (hi - lo).max(1.0)).clamp(0.0, 1.0) as f32, format!("{t:.1} °C").replace(".0 °C", " °C"))
                }
                _ => (false, 0.0, String::new()),
            };
            let words = match (domain, state) {
                (_, "unavailable") | (_, "unknown") => texts::fresh("quick.gone"),
                ("cover", "open") => "Open".into(),
                ("cover", "closed") => "Closed".into(),
                ("cover", "opening") => "Opening".into(),
                ("cover", "closing") => "Closing".into(),
                ("climate", s) => {
                    let mode = s.replace('_', " ");
                    let mode = mode[..1].to_uppercase() + &mode[1..];
                    if dim && on { format!("{mode} · {short}") } else { mode }
                }
                (_, "on") if dim => format!("On · {short}"),
                (_, "on") => "On".into(),
                (_, "off") => "Off".into(),
                (_, s) => {
                    let s = s.replace('_', " ");
                    s[..1].to_uppercase() + &s[1..]
                }
            };
            // A colour light glows in its own colour; everything else in the accent.
            let rgb = attr("rgb_color").and_then(Value::as_array).filter(|_| on).and_then(|c| {
                let v: Vec<u8> = c.iter().filter_map(Value::as_u64).map(|x| x.min(255) as u8).collect();
                (v.len() == 3).then(|| Color::from_rgb_u8(v[0], v[1], v[2]))
            });
            let tint = rgb.unwrap_or(accent);
            let led = rgb.unwrap_or(Color::from_rgb_u8(0x69, 0xdb, 0x7c));
            let icon = attr("icon")
                .and_then(Value::as_str)
                .and_then(icons::glyph)
                .or_else(|| icons::glyph(icons::default_icon(domain, state, attr("device_class").and_then(Value::as_str))))
                .map(String::from)
                .unwrap_or_default();
            let w = if matches!(kind, 0 | 2 | 3) { 2 } else { 1 };
            if col + w > 4 {
                col = 0;
                row += 1;
            }
            let tile = QuickTile {
                name: name.into(),
                label: SharedString::new(),
                icon: icon.into(),
                kind,
                on,
                dim,
                value,
                state: words.into(),
                short: short.into(),
                gone,
                tint,
                led,
                col,
                row,
                w,
                tape: Default::default(),
                has_tape: false,
            };
            col += w;
            tile
        })
        .collect()
}

/// A picture of the UI bundle (page assets the app sets as properties).
pub fn picture(rel: &str) -> slint::Image {
    let path = std::path::Path::new(env!("PANELOS_UI_PICTURES")).join(rel);
    slint::Image::load_from_path(&path).unwrap_or_default()
}

/// The quick controls page from the states (only when the panel has one).
pub fn refresh(ui: &AppWindow, s: &mut crate::UiState) {
    let pages = ui.get_pages();
    let Some(slot) = (0..slint::Model::row_count(&pages)).filter_map(|i| slint::Model::row_data(&pages, i)).find(|p| p.kind == "quick")
    else {
        return;
    };
    ui.set_quick_look(slot.variant);
    ui.set_quick_title(texts::kept(if slot.variant == 2 { "quick.titles.deck" } else { "quick.titles" }, 0).into());
    // the deck's placard may name the panel's room (lines that need it wait until the panel knows its room)
    let room = crate::config_str(s, "room").to_uppercase();
    ui.set_quick_placard(texts::kept_if("quick.placard", 0, |l| !l.contains("{ROOM}") || !room.is_empty()).replace("{ROOM}", &room).into());
    if ui.get_quick_screw().size().width == 0 {
        // The desk's and the deck's panels and parts (loaded once).
        ui.set_quick_mixer_panel(picture("quick/mixer-panel.png"));
        ui.set_quick_fader(picture("quick/fader.png"));
        ui.set_quick_deck_panel(picture("quick/deck-panel2.png"));
        ui.set_quick_toggle_up(picture("quick/toggle3d-up.png"));
        ui.set_quick_toggle_down(picture("quick/toggle3d-down.png"));
        // the lever's throw: 9 frames, down … up
        let frames: Vec<slint::Image> = (0..9).map(|k| picture(&format!("quick/toggle3d-{k}.png"))).collect();
        if frames.iter().all(|f| f.size().width > 0) {
            ui.set_quick_lever_frames(slint::ModelRc::new(slint::VecModel::from(frames)));
        }
        ui.set_quick_lamp_off(picture("quick/lamp-off.png"));
        ui.set_quick_lamp_glow(picture("quick/lamp-glow.png"));
        ui.set_quick_lamp_spec(picture("quick/lamp-spec.png"));
        ui.set_quick_lamp_amber(picture("quick/lamp-amber.png"));
        ui.set_quick_screw(picture("quick/screw.png"));
    }
    let entities = crate::page_entities(s, "quick");
    let mut list = tiles(&s.states, &entities, crate::accent(s));
    // the mixing desk: a tiny label tape under each fader (the integration's tape text, else the
    // name's last word), coloured by kind
    let looks = crate::page_looks(ui, "quick");
    if looks.contains(&1) || looks.contains(&2) {
        let own = crate::page_option(s, "quick", "tapes").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        for (i, (t, id)) in list.iter_mut().zip(&entities).enumerate() {
            let text = own
                .get(i)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| t.name.split_whitespace().last().unwrap_or("").chars().take(7).collect());
            let tint = match id.split('.').next().unwrap_or("") {
                "climate" | "fan" | "water_heater" => crate::draw::Tint::Green,
                "media_player" => crate::draw::Tint::Red,
                _ => crate::draw::Tint::Blue,
            };
            // the flight deck's plates show the short text too
            t.label = text.clone().into();
            if looks.contains(&1) && crate::draw::AVAILABLE {
                let seed = crate::draw::seed(id);
                t.tape = crate::draw::get(crate::draw::Job::DeskTape { text, tint, seed });
                t.has_tape = t.tape.size().width > 0;
            }
        }
    }
    let mood = mood(&list);
    if ui.get_quick_mood().as_str() != mood {
        ui.set_quick_mood(mood.into());
    }
    if let Some(m) = crate::pages::update_rows(&ui.get_quick_tiles(), list) {
        ui.set_quick_tiles(m);
    }
    if ui.get_quick_sheet() {
        details(ui, ui.get_quick_sheet_index().max(0) as usize);
    }
}

/// The line under the tiles: all off, some on, all on.
pub fn mood(tiles: &[QuickTile]) -> String {
    let live: Vec<&QuickTile> = tiles.iter().filter(|t| !t.gone).collect();
    if live.is_empty() {
        return String::new();
    }
    let on = live.iter().filter(|t| t.on).count();
    texts::kept(
        if on == 0 {
            "quick.off"
        } else if on == live.len() {
            "quick.allon"
        } else {
            "quick.some"
        },
        0,
    )
}

/// Taps on the page (`quick.*`): the entity at the tile's index.
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let (entities, states) = with_state(|s| (crate::page_entities(s, "quick"), s.states.clone()));
    let index = |v: &str| v.split(':').next().and_then(|i| i.parse::<usize>().ok());
    let arg = |v: &str| v.split(':').nth(1).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    let id_of = |v: &str| index(v).and_then(|i| entities.get(i)).cloned();
    match what {
        "quick.toggle" => {
            if let Some(id) = id_of(value) {
                toggle(&id, &states);
            }
        }
        "quick.alloff" => {
            for id in &entities {
                let domain = id.split('.').next().unwrap_or("");
                match domain {
                    "light" | "switch" | "fan" | "input_boolean" | "media_player" | "climate" => {
                        crate::call(domain, "turn_off", id, json!({}))
                    }
                    _ => {}
                }
            }
        }
        "quick.details" => {
            let Some(i) = index(value) else { return };
            ui.set_quick_sheet(true);
            details(ui, i);
        }
        "quick.close" => ui.set_quick_sheet(false),
        // value "index:level" with the level 0 … 1
        "quick.level" => {
            let Some(id) = id_of(value) else { return };
            let level = arg(value).clamp(0.0, 1.0);
            let domain = id.split('.').next().unwrap_or("");
            match domain {
                "light" if level <= 0.005 => crate::call("light", "turn_off", &id, json!({})),
                "light" => crate::call("light", "turn_on", &id, json!({"brightness_pct": (level * 100.0).round().max(1.0)})),
                "cover" => crate::call("cover", "set_cover_position", &id, json!({"position": (level * 100.0).round()})),
                "climate" => {
                    let st = states.get(&id);
                    let num = |k: &str| st.and_then(|s| s.attrs.get(k)).and_then(Value::as_f64);
                    let (lo, hi) = (num("min_temp").unwrap_or(7.0), num("max_temp").unwrap_or(35.0));
                    let step = num("target_temp_step").unwrap_or(0.5);
                    let t = ((lo + level * (hi - lo)) / step).round() * step;
                    crate::call("climate", "set_temperature", &id, json!({"temperature": t}));
                }
                _ => toggle(&id, &states),
            }
        }
        // colour temperature 0 (warm) … 1 (cool)
        "quick.temp" => {
            let Some(id) = id_of(value) else { return };
            let st = states.get(&id);
            let num = |k: &str| st.and_then(|s| s.attrs.get(k)).and_then(Value::as_f64);
            let (warm, cool) = (num("min_color_temp_kelvin").unwrap_or(2000.0), num("max_color_temp_kelvin").unwrap_or(6500.0));
            let k = warm + arg(value).clamp(0.0, 1.0) * (cool - warm);
            crate::call("light", "turn_on", &id, json!({"color_temp_kelvin": k.round()}));
        }
        "quick.colour" => {
            let Some(id) = id_of(value) else { return };
            let c = PALETTE[(arg(value) as usize).min(PALETTE.len() - 1)];
            crate::call("light", "turn_on", &id, json!({"rgb_color": [c >> 16, (c >> 8) & 255, c & 255]}));
        }
        "quick.effect" => {
            let Some(id) = id_of(value) else { return };
            let effect = value.split_once(':').map(|(_, e)| e).unwrap_or("");
            crate::call("light", "turn_on", &id, json!({"effect": effect}));
        }
        // 0 open, 1 stop, 2 close
        "quick.cover" => {
            let Some(id) = id_of(value) else { return };
            let service = match arg(value) as i32 {
                0 => "open_cover",
                1 => "stop_cover",
                _ => "close_cover",
            };
            crate::call("cover", service, &id, json!({}));
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled)"),
    }
}

fn toggle(id: &str, states: &HashMap<String, EntityState>) {
    let domain = id.split('.').next().unwrap_or("");
    match domain {
        "scene" => crate::call("scene", "turn_on", id, json!({})),
        "script" => crate::call("script", "turn_on", id, json!({})),
        "cover" => crate::call("cover", "toggle", id, json!({})),
        "vacuum" => {
            let cleaning = states.get(id).is_some_and(|s| s.state == "cleaning");
            crate::call("vacuum", if cleaning { "return_to_base" } else { "start" }, id, json!({}));
        }
        "climate" => {
            let off = states.get(id).is_none_or(|s| s.state == "off");
            crate::call("climate", if off { "turn_on" } else { "turn_off" }, id, json!({}));
        }
        _ => crate::call("homeassistant", "toggle", id, json!({})),
    }
}

/// Colours offered for colour lights in the details sheet.
const PALETTE: [u32; 7] = [0xffd8a8, 0xff6b6b, 0xffa94d, 0xffe066, 0x69db7c, 0x4dabf7, 0xb197fc];

/// The details sheet of tile `i`: what the entity can do.
pub fn details(ui: &AppWindow, i: usize) {
    let (tile, st) = with_state(|s| {
        let entities = crate::page_entities(s, "quick");
        let tiles = tiles(&s.states, &entities, crate::accent(s));
        (tiles.get(i).cloned(), entities.get(i).and_then(|id| s.states.get(id)).map(|e| (e.state.clone(), e.attrs.clone())))
    });
    let Some(tile) = tile else { return };
    let modes: Vec<String> = st
        .as_ref()
        .and_then(|(_, a)| a.get("supported_color_modes"))
        .and_then(Value::as_array)
        .map(|m| m.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let attrs = st.map(|(_, a)| a).unwrap_or_default();
    let num = |k: &str| attrs.get(k).and_then(Value::as_f64);
    let temp = if modes.iter().any(|m| m == "color_temp") {
        let (warm, cool) = (num("min_color_temp_kelvin").unwrap_or(2000.0), num("max_color_temp_kelvin").unwrap_or(6500.0));
        num("color_temp_kelvin").map(|k| ((k - warm) / (cool - warm).max(1.0)).clamp(0.0, 1.0) as f32).unwrap_or(0.5)
    } else {
        -1.0
    };
    ui.set_quick_sheet_temp(temp);
    ui.set_quick_sheet_temp_text(num("color_temp_kelvin").map(|k| format!("{k:.0} K")).unwrap_or_default().into());
    let colourful = modes.iter().any(|m| matches!(m.as_str(), "hs" | "rgb" | "rgbw" | "rgbww" | "xy"));
    let colours: Vec<Color> = if colourful {
        PALETTE.iter().map(|c| Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, *c as u8)).collect()
    } else {
        Vec::new()
    };
    ui.set_quick_sheet_colours(ModelRc::new(VecModel::from(colours)));
    let effects: Vec<SharedString> = attrs
        .get("effect_list")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_str).take(4).map(SharedString::from).collect())
        .unwrap_or_default();
    ui.set_quick_sheet_effects(ModelRc::new(VecModel::from(effects)));
    ui.set_quick_sheet_index(i as i32);
    ui.set_quick_sheet_tile(tile);
}
