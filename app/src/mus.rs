//! Music page player picker: which player the page shows (remembered on the
//! panel), moving the music to another one, playing it there too. The list is
//! read when the picker opens (Home Assistant's REST states: Music Assistant
//! players, else every media player), in a thread.

use std::io::Read;
use std::time::Duration;

use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{icons, with_state, AppWindow, MusicPlayer};

thread_local! {
    /// The picker's rows: entity ids in the order shown.
    static IDS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn open(ui: &AppWindow) {
    ui.set_music_picker(true);
    let (url, auth, active) = with_state(|s| (s.config.as_ref().map(|c| c.ha_url.clone()), s.auth.clone(), crate::media_entity(s)));
    let (Some(url), Some(auth)) = (url, auth) else { return };
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let Ok(token) = auth.access_token() else { return };
        // Only the media players, in Home Assistant's own words (a template):
        // the whole state list is too big for the panel's Wi-Fi.
        let template = r#"[{% for s in states.media_player if s.state != 'unavailable' %}{"entity_id":{{ s.entity_id|tojson }},"state":{{ s.state|tojson }},"attributes":{"friendly_name":{{ s.name|tojson }},"mass_player_type":{{ s.attributes.mass_player_type|default(none)|tojson }},"media_title":{{ s.attributes.media_title|default(none)|tojson }},"media_artist":{{ s.attributes.media_artist|default(none)|tojson }},"device_class":{{ s.attributes.device_class|default(none)|tojson }},"group_members":{{ s.attributes.group_members|default([])|list|tojson }}}}{% if not loop.last %},{% endif %}{% endfor %}]"#;
        let resp = ureq::post(&format!("{url}/api/template"))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .timeout(Duration::from_secs(10))
            .send_string(&json!({ "template": template }).to_string());
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                eprintln!("nspanel-app: music players: {e}");
                return;
            }
        };
        let mut body = String::new();
        if resp.into_reader().take(2 * 1024 * 1024).read_to_string(&mut body).is_err() {
            return;
        }
        let all: Vec<Value> = match serde_json::from_str::<Value>(&body) {
            Ok(Value::Array(a)) => a,
            other => {
                eprintln!("nspanel-app: music players: unexpected answer ({:?})", other.err());
                return;
            }
        };
        // A missing attribute comes back as null: treat it as absent.
        let all: Vec<Value> = all
            .into_iter()
            .map(|mut e| {
                if let Some(a) = e["attributes"].as_object_mut() {
                    a.retain(|_, v| !v.is_null());
                }
                e
            })
            .collect();
        let players: Vec<&Value> = all
            .iter()
            .filter(|e| e["entity_id"].as_str().is_some_and(|i| i.starts_with("media_player.")) && e["state"] != "unavailable")
            .collect();
        let mass: Vec<&Value> = players.iter().copied().filter(|e| e["attributes"].get("mass_player_type").is_some()).collect();
        let list = if mass.is_empty() { players } else { mass };
        let group: Vec<String> = list
            .iter()
            .find(|e| e["entity_id"] == active.as_str())
            .and_then(|e| e["attributes"]["group_members"].as_array())
            .map(|g| g.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        let rows: Vec<(String, String, String, bool, bool, String)> = list
            .iter()
            .map(|e| {
                let id = e["entity_id"].as_str().unwrap_or("").to_string();
                let a = &e["attributes"];
                let name = a["friendly_name"].as_str().unwrap_or(&id).to_string();
                let state = e["state"].as_str().unwrap_or("");
                let playing = state == "playing";
                let now = match (a["media_title"].as_str(), a["media_artist"].as_str()) {
                    (Some(t), Some(ar)) if matches!(state, "playing" | "paused") => format!("{t} · {ar}"),
                    (Some(t), None) if matches!(state, "playing" | "paused") => t.to_string(),
                    _ => String::new(),
                };
                let icon = if a["device_class"] == "tv" { "television" } else { "speaker" };
                (id, name, icon.to_string(), playing, group.iter().any(|g| *g == e["entity_id"]), now)
            })
            .collect();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let players: Vec<MusicPlayer> = rows
                .iter()
                .map(|(id, name, icon, playing, grouped, now)| MusicPlayer {
                    name: name.as_str().into(),
                    icon: icons::glyph(icon).map(String::from).unwrap_or_default().into(),
                    active: *id == active,
                    grouped: *grouped && *id != active,
                    playing: *playing,
                    now: now.as_str().into(),
                })
                .collect();
            IDS.with(|i| *i.borrow_mut() = rows.into_iter().map(|r| r.0).collect());
            ui.set_music_players(ModelRc::new(VecModel::from(players)));
        });
    });
}

/// Taps on the music page's picker (`music.picker`, `music.show` …).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let id = value.parse::<usize>().ok().and_then(|i| IDS.with(|ids| ids.borrow().get(i).cloned()));
    match what {
        "music.picker" => open(ui),
        "music.picker-close" => ui.set_music_picker(false),
        "music.show" => {
            let Some(id) = id else { return };
            with_state(|s| {
                s.settings.music_player = id;
                s.settings.save();
            });
            ui.set_music_picker(false);
            crate::apply_panel_config(ui);
        }
        // Music Assistant moves the queue; the page follows the music.
        "music.transfer" => {
            let Some(id) = id else { return };
            let from = with_state(|s| crate::media_entity(s));
            crate::call("music_assistant", "transfer_queue", &id, json!({ "source_player": from }));
            with_state(|s| {
                s.settings.music_player = id;
                s.settings.save();
            });
            ui.set_music_picker(false);
            crate::apply_panel_config(ui);
        }
        "music.group" => {
            let Some(id) = id else { return };
            let active = with_state(|s| crate::media_entity(s));
            let grouped = ui.get_music_players().iter().enumerate().any(|(i, p)| p.grouped && IDS.with(|ids| ids.borrow().get(i) == Some(&id)));
            if grouped {
                crate::call("media_player", "unjoin", &id, json!({}));
            } else {
                crate::call("media_player", "join", &active, json!({ "group_members": [id] }));
            }
            // The list again once Home Assistant has it.
            let weak = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(1500), move || {
                if let Some(ui) = weak.upgrade() {
                    if ui.get_music_picker() {
                        open(&ui);
                    }
                }
            });
        }
        _ => {}
    }
}
