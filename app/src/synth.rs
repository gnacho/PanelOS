//! The synthwave scenes ("In the Fast Lane": home background, timers theme,
//! thermostat look): which view (day, night, the beach's dawn) and the fade
//! between them by the sun, the floor colour, and a passing car now and then.
//! The clock, the floor lines, the palms and the cars' motion run in the UI.

use serde_json::Value;
use slint::{Color, ModelRc, VecModel};

use crate::{AppWindow, SwCar};

const SCENES: [&str; 4] = ["sunset", "city", "beach", "stars"];

fn random() -> u64 {
    use std::cell::Cell;
    thread_local!(static SEED: Cell<u64> = Cell::new(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1
    ));
    SEED.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        x
    })
}

fn colour(hex: &str) -> Color {
    crate::pages::parse_color(hex).unwrap_or(Color::from_rgb_u8(0xff, 0x2b, 0xd6))
}

/// The floor colour of a view (colours.json of the scenes).
fn grid(scene: usize, view: &str) -> Color {
    thread_local!(static TABLE: Value = std::fs::read_to_string(
        std::path::Path::new(env!("PANELOS_UI_PICTURES")).join("home/fl/colours.json")
    ).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null));
    TABLE.with(|t| colour(t[SCENES[scene]][view][0].as_str().unwrap_or("#ff2bd6")))
}

/// The scene clock's step (seconds): a whole number of the scenes' 40 s loops.
const SCENE_WRAP_S: f32 = 1200.0;

/// Updates the scenes that are in use (once a second).
pub fn tick(ui: &AppWindow, s: &crate::UiState) {
    // any page on the list with a synthwave look (a kind may be there more than once)
    let pages = ui.get_pages();
    let any = |f: &dyn Fn(&crate::PageSlot) -> bool| (0..slint::Model::row_count(&pages)).filter_map(|i| slint::Model::row_data(&pages, i)).any(|p| f(&p));
    let home = ui.get_weather().theme == 8 || any(&|p| p.kind == "home" && p.theme == 8);
    let timers = any(&|p| p.kind == "timers" && p.variant == 3);
    let thermo = any(&|p| p.kind == "thermostat" && p.variant == 3);
    if !home && !timers && !thermo {
        return;
    }
    let scene = crate::page_option(s, "home", "scene")
        .and_then(|v| v.as_u64())
        .or_else(|| s.panel_config.as_ref().and_then(|c| c.get("fl_scene")).and_then(Value::as_u64))
        .unwrap_or(0)
        .min(3) as usize;
    // The view by the sun: day above +3°, night below -6°, a fade between; the
    // beach has a dawn view for the morning.
    let sun = s.states.get("sun.sun");
    let el = sun.and_then(|e| e.attrs.get("elevation")).and_then(Value::as_f64).unwrap_or(30.0);
    let rising = sun.and_then(|e| e.attrs.get("rising")).and_then(Value::as_bool).unwrap_or(false);
    let night_view = if scene == 2 && rising { "dawn" } else { "night" };
    let mix = ((3.0 - el) / 9.0).clamp(0.0, 1.0) as f32;
    let pic = |view: &str, floor: bool| {
        crate::quick::picture(&format!("home/fl/fl-{scene}-{view}{}.png", if floor { "-floor" } else { "" }))
    };
    if ui.get_home_fl_scene() != scene as i32 || ui.get_home_fl_back_a().size().width == 0 {
        ui.set_home_fl_scene(scene as i32);
        ui.set_home_fl_back_a(pic("day", false));
        ui.set_home_fl_floor_a(pic("day", true));
        ui.set_home_fl_back_b(pic(night_view, false));
        ui.set_home_fl_floor_b(pic(night_view, true));
        ui.set_timers_sw_scene(scene as i32);
        ui.set_timers_sw_back(crate::quick::picture(&format!("timers/sw/scene-{scene}-back.png")));
        ui.set_timers_sw_floor(crate::quick::picture(&format!("timers/sw/scene-{scene}-floor.png")));
    }
    if (ui.get_home_fl_mix() - mix).abs() > 0.004 {
        ui.set_home_fl_mix(mix);
    }
    let g = grid(scene, "day");
    let g = if mix > 0.0 {
        let n = grid(scene, night_view);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * mix).round() as u8;
        Color::from_rgb_u8(lerp(g.red(), n.red()), lerp(g.green(), n.green()), lerp(g.blue(), n.blue()))
    } else {
        g
    };
    if ui.get_home_fl_grid() != g {
        ui.set_home_fl_grid(g);
    }
    ui.set_home_fl_star(mix);
    // The scene clock moves its base on by 20 minutes (30 times the 40 s loop of the floor, the
    // palms and the star) and the car on the road with it, in the same step: nothing jumps.
    let cars = ui.get_home_fl_cars();
    let st = ui.get_scene_t();
    if st > SCENE_WRAP_S {
        // (several steps at once when no scene ran for a long time)
        let shift = (st / SCENE_WRAP_S).floor() * SCENE_WRAP_S;
        ui.set_scene_base(ui.get_scene_base() + shift as i64 * 1000);
        if let Some(mut c) = slint::Model::row_data(&cars, 0) {
            c.born -= shift;
            slint::Model::set_row_data(&cars, 0, c);
        }
    }
    // One car at a time: a new one when the last is gone (random line, never
    // the same one twice, random kind and vivid colour, 10–30 s to the horizon).
    let now = ui.get_scene_t();
    let last = slint::Model::row_data(&cars, 0);
    if last.as_ref().is_none_or(|c| now - c.born > c.dur + 0.5) {
        let old_line = last.map(|c| c.line).unwrap_or(9.0);
        let lines = [-2.0f32, -1.0, 0.0, 1.0, 2.0];
        let choices: Vec<f32> = lines.iter().copied().filter(|l| *l != old_line).collect();
        let tints = [0xff2a6d, 0xffd319, 0x00e5ff, 0xc86bfa, 0x01ffc3, 0xff8b3d];
        let t = tints[(random() % tints.len() as u64) as usize];
        let car = SwCar {
            line: choices[(random() % choices.len() as u64) as usize],
            kind: (random() % 3) as i32,
            tint: Color::from_rgb_u8((t >> 16) as u8, (t >> 8) as u8, t as u8),
            born: now,
            dur: 10.0 + (random() % 2000) as f32 / 100.0,
        };
        let model = ModelRc::new(VecModel::from(vec![car]));
        ui.set_home_fl_cars(model.clone());
        ui.set_timers_sw_cars(model);
    }
}
