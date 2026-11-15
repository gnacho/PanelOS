//! Alarms on the timers page: kept on the panel (settings.json), set in the
//! page's editor (drums, days, sound, label), ringing on the panel at their
//! minute like a finished timer.

use serde::{Deserialize, Serialize};
use slint::{Color, ModelRc, SharedString, VecModel};

use crate::pages::{local_format, now};
use crate::{with_state, AlarmItem, AppWindow};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Alarm {
    pub h: i32,
    pub m: i32,
    /// Monday … Sunday; none set: once, the next time it comes.
    pub days: [bool; 7],
    pub label: String,
    pub sound: i32,
    pub on: bool,
}

impl Default for Alarm {
    fn default() -> Self {
        Self { h: 7, m: 0, days: [true, true, true, true, true, false, false], label: String::new(), sound: 0, on: true }
    }
}

const TINTS: [u32; 6] = [0xffb21e, 0x3d9bff, 0x2fd27a, 0xc86bfa, 0xff5a36, 0xff4fa0];

fn tint(i: usize) -> Color {
    let c = TINTS[i % TINTS.len()];
    Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn days_text(d: &[bool; 7]) -> String {
    let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    match d {
        [true, true, true, true, true, true, true] => "Every day".into(),
        [true, true, true, true, true, false, false] => "Weekdays".into(),
        [false, false, false, false, false, true, true] => "Weekends".into(),
        _ if d.iter().all(|x| !x) => "Once".into(),
        _ => names.iter().zip(d).filter(|(_, on)| **on).map(|(n, _)| *n).collect::<Vec<_>>().join(", "),
    }
}

/// Minutes from now to the alarm's next ring (None when it never rings).
fn minutes_to(a: &Alarm, t: i64) -> Option<i64> {
    if !a.on {
        return None;
    }
    let wd = (local_format(t, "%u").parse::<i64>().unwrap_or(1) - 1).clamp(0, 6);
    let mins_now = local_format(t, "%H").parse::<i64>().unwrap_or(0) * 60 + local_format(t, "%M").parse::<i64>().unwrap_or(0);
    let target = a.h as i64 * 60 + a.m as i64;
    (0..8)
        .find(|&d| {
            let day = ((wd + d) % 7) as usize;
            let later = d > 0 || target > mins_now;
            later && (a.days.iter().all(|x| !x) || a.days[day])
        })
        .map(|d| d * 1440 + target - mins_now)
}

/// The alarm list and the "next alarm" line.
pub fn refresh(ui: &AppWindow, s: &crate::UiState) {
    let t = now();
    let rows: Vec<AlarmItem> = s
        .settings
        .alarms
        .iter()
        .enumerate()
        .map(|(i, a)| AlarmItem {
            time: format!("{}:{:02}", a.h, a.m).into(),
            days: days_text(&a.days).into(),
            label: if a.label.is_empty() { "Alarm".into() } else { a.label.clone().into() },
            on: a.on,
            tint: tint(i),
        })
        .collect();
    if let Some(m) = crate::pages::update_rows(&ui.get_timers_alarms(), rows) {
        ui.set_timers_alarms(m);
    }
    let next = s.settings.alarms.iter().enumerate().filter_map(|(i, a)| minutes_to(a, t).map(|m| (m, i, a))).min_by_key(|(m, _, _)| *m);
    let text = match next {
        Some((m, i, a)) => {
            ui.set_timers_next_tint(tint(i));
            let (h, mm) = (m / 60, m % 60);
            let when = if h >= 24 { format!("in {} d {} h", h / 24, h % 24) } else if h > 0 { format!("in {h} h {mm} min") } else { format!("in {mm} min") };
            format!("{}:{:02} · {when}", a.h, a.m)
        }
        None => String::new(),
    };
    if ui.get_timers_next_alarm().as_str() != text {
        ui.set_timers_next_alarm(text.into());
    }
}

/// Alarms that ring this minute (called when the minute turns).
pub fn due(s: &mut crate::UiState) -> Option<(String, String)> {
    let t = now();
    let wd = (local_format(t, "%u").parse::<usize>().unwrap_or(1) - 1).min(6);
    let (h, m) = (local_format(t, "%H").parse::<i32>().unwrap_or(-1), local_format(t, "%M").parse::<i32>().unwrap_or(-1));
    let mut rang = None;
    let mut changed = false;
    for a in s.settings.alarms.iter_mut() {
        let once = a.days.iter().all(|x| !x);
        if a.on && a.h == h && a.m == m && (once || a.days[wd]) {
            rang = Some((format!("{}:{:02}", a.h, a.m), if a.label.is_empty() { "Alarm".to_string() } else { a.label.clone() }));
            if once {
                a.on = false;
                changed = true;
            }
        }
    }
    if changed {
        s.settings.save();
    }
    rang
}

fn open_editor(ui: &AppWindow, a: &Alarm, new: bool, i: usize) {
    ui.set_timers_edit_timer(false);
    ui.set_timers_edit_new(new);
    ui.set_timers_edit_h(a.h);
    ui.set_timers_edit_m(a.m);
    ui.set_timers_edit_days(ModelRc::new(VecModel::from(a.days.to_vec())));
    ui.set_timers_edit_label(a.label.clone().into());
    ui.set_timers_edit_sound(a.sound);
    ui.set_timers_edit_tint(tint(i));
    ui.set_timers_edit_keyboard(false);
    ui.set_timers_edit_info(false);
    ui.set_timers_edit(true);
}

/// Taps on the alarms tab and in the editor (`alarms.*`, `alarm.*`).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let n: usize = value.parse().unwrap_or(0);
    match what {
        "alarms.new" => {
            let count = with_state(|s| {
                s.alarm_edit = None;
                s.settings.alarms.len()
            });
            open_editor(ui, &Alarm::default(), true, count);
        }
        "alarms.edit" => {
            let Some(a) = with_state(|s| {
                s.alarm_edit = Some(n);
                s.settings.alarms.get(n).cloned()
            }) else {
                return;
            };
            open_editor(ui, &a, false, n);
        }
        "alarms.toggle" => with_state(|s| {
            if let Some(a) = s.settings.alarms.get_mut(n) {
                a.on = !a.on;
                s.settings.save();
            }
        }),
        "alarm.day" => {
            let days = ui.get_timers_edit_days();
            if let Some(v) = slint::Model::row_data(&days, n) {
                slint::Model::set_row_data(&days, n, !v);
            }
        }
        "alarm.sound" => ui.set_timers_edit_sound(n as i32),
        "alarm.info" => ui.set_timers_edit_info(!ui.get_timers_edit_info()),
        "alarm.label" => {
            ui.set_timers_kb_layer(0);
            ui.set_timers_edit_keyboard(true);
        }
        "alarm.key" => {
            let mut label = ui.get_timers_edit_label().to_string();
            let layer = ui.get_timers_kb_layer();
            match value {
                "enter" => ui.set_timers_edit_keyboard(false),
                "back" => {
                    label.pop();
                }
                "clear" => label.clear(),
                "shift" => ui.set_timers_kb_layer(match layer {
                    0 => 1,
                    1 => 0,
                    2 => 3,
                    _ => 2,
                }),
                "symbols" => ui.set_timers_kb_layer(if layer >= 2 { 0 } else { 2 }),
                "layout" => {}
                k => {
                    if label.chars().count() < 32 {
                        label.push_str(k);
                    }
                    // one capital, then small letters again
                    if layer == 1 {
                        ui.set_timers_kb_layer(0);
                    }
                }
            }
            ui.set_timers_edit_label(SharedString::from(label));
        }
        "alarm.save" => {
            let days: Vec<bool> = slint::Model::iter(&ui.get_timers_edit_days()).collect();
            let mut d = [false; 7];
            for (i, v) in days.iter().take(7).enumerate() {
                d[i] = *v;
            }
            let a = Alarm {
                h: ui.get_timers_edit_h(),
                m: ui.get_timers_edit_m(),
                days: d,
                label: ui.get_timers_edit_label().trim().to_string(),
                sound: ui.get_timers_edit_sound(),
                on: true,
            };
            with_state(|s| {
                match s.alarm_edit.take() {
                    Some(i) if i < s.settings.alarms.len() => s.settings.alarms[i] = a,
                    _ => s.settings.alarms.push(a),
                }
                s.settings.alarms.sort_by_key(|a| a.h * 60 + a.m);
                s.settings.save();
            });
            ui.set_timers_edit(false);
        }
        "alarm.delete" => {
            with_state(|s| {
                if let Some(i) = s.alarm_edit.take() {
                    if i < s.settings.alarms.len() {
                        s.settings.alarms.remove(i);
                        s.settings.save();
                    }
                }
            });
            ui.set_timers_edit(false);
        }
        "alarm.close" => {
            ui.set_timers_edit_keyboard(false);
            ui.set_timers_edit(false);
        }
        _ => {}
    }
    with_state(|s| refresh(ui, s));
}
