//! The synthwave scenes (home background, timers theme,
//! thermostat look): which view (day, night, the beach's dawn) and the fade
//! between them by the sun, the floor colour, and a passing car now and then.
//! The clock, the floor lines, the palms and the cars' motion run in the UI.

use serde_json::Value;
use slint::{Color, ModelRc, VecModel};

use crate::{AppWindow, SwCar};

const SCENES: [&str; 4] = ["sunset", "city", "beach", "stars"];

/// The cars' neon colours, around the colour wheel: hot pink, neon red, sunset orange, sun yellow,
/// mint, cyan, electric blue, ultraviolet, violet, magenta, and chrome white.
const CAR_TINTS: [u32; 11] = [0xff2a6d, 0xff3b3b, 0xff8b3d, 0xffd319, 0x01ffc3, 0x00e5ff, 0x4d7cff, 0x8a5cff, 0xc86bfa, 0xff3df0, 0xeef0ff];
/// Colours too close to tell apart at a glance (by index into CAR_TINTS).
const CAR_TINT_NEAR: [(usize, usize); 7] = [(0, 1), (1, 2), (4, 5), (6, 7), (7, 8), (8, 9), (9, 0)];
/// How many of the last cars a colour (or one close to it) must not repeat within.
const CAR_TINT_GAP: usize = 5;

/// The next car's colour: never the colour of one of the last five cars or one close to it (so never
/// two alike on the screen either). Of the colours left, only one that still leaves a colour for the car
/// after it (with these colours that is always possible, so the rule always holds; the colour used
/// longest ago is only a safety net).
fn next_tint() -> usize {
    thread_local!(static RECENT: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) });
    let near = |a: usize, b: usize| a == b || CAR_TINT_NEAR.iter().any(|&(x, y)| (x, y) == (a, b) || (y, x) == (a, b));
    RECENT.with(|r| {
        let mut recent = r.borrow_mut();
        // cars ago this colour (or a close one) was last used; usize::MAX = not lately
        let ago = |t: usize, list: &[usize]| list.iter().rev().position(|&u| near(t, u)).unwrap_or(usize::MAX);
        let free = |list: &[usize]| -> Vec<usize> { (0..CAR_TINTS.len()).filter(|&t| ago(t, list) >= CAR_TINT_GAP).collect() };
        let mut options = free(&recent);
        let keeps_room: Vec<usize> = options
            .iter()
            .copied()
            .filter(|&c| {
                let mut then = recent.clone();
                then.push(c);
                !free(&then).is_empty()
            })
            .collect();
        if !keeps_room.is_empty() {
            options = keeps_room;
        }
        let pick = if options.is_empty() {
            (0..CAR_TINTS.len()).max_by_key(|&t| ago(t, &recent)).unwrap_or(0)
        } else {
            options[(random() % options.len() as u64) as usize]
        };
        recent.push(pick);
        let extra = recent.len().saturating_sub(CAR_TINT_GAP * 2);
        recent.drain(..extra);
        pick
    })
}

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

/// A car's trip to the horizon (seconds): a random length in this range, so the speeds vary.
const CAR_TRIP: (f32, f32) = (8.0, 18.0);

/// A car's depth at a share of its trip (as the scene draws it: 0.46 under the front edge … 60).
fn car_depth(share: f32) -> f32 {
    0.46 * (60.0f32 / 0.46).powf(share.clamp(0.0, 1.0))
}

/// Whether a car on `line`, starting at `born` with a trip of `dur`, never touches `other`: either it
/// never reaches it, or where it passes it their lanes are far enough apart at that depth (the widest
/// car body is 46 of its 48 sprite pixels, 176 px wide at depth 1; lanes are 43.8 / d + 2.2 apart; 10 %
/// to spare).
fn clear_of(line: f32, born: f32, dur: f32, other: &SwCar) -> bool {
    // the same share of the trip = the same depth: when (if ever) the new car comes level
    if dur >= other.dur {
        return born >= other.born;
    }
    let t = (born * other.dur - other.born * dur) / (other.dur - dur);
    let share = (t - other.born) / other.dur;
    if share > 1.0 || t < born {
        return true;
    }
    let d = car_depth(share);
    let apart = (line - other.line).abs() * (43.8 / d + 2.2);
    apart >= 1.1 * (46.0 * 176.0 / 48.0) / d
}

/// The trip for a new car on `line`: the wanted one when it touches no car on the road, else just
/// slower (1 s more) than each car it would touch; None when that is longer than the longest trip.
fn fitting_trip(line: f32, now: f32, want: f32, alive: &[SwCar]) -> Option<f32> {
    let mut dur = want;
    for _ in 0..4 {
        match alive.iter().find(|c| !clear_of(line, now, dur, c)) {
            Some(c) => dur = dur.max(c.dur - (now - c.born) + 1.0),
            None => return (dur <= CAR_TRIP.1).then_some(dur),
        }
    }
    alive.iter().all(|c| clear_of(line, now, dur, c)).then_some(dur).filter(|d| *d <= CAR_TRIP.1)
}

/// How far behind the car it follows the patrol car starts, as a share of their trip (0.3-0.6 s was too
/// close; 0.7-1.1 s still overlapped near the viewer). From 15 % on, the patrol car's
/// roof stays below the car ahead, so it never covers it; with the same trip it keeps this distance.
const PATROL_BEHIND: (f32, f32) = (0.15, 0.18);

/// The patrol car's kind (black and white with a light bar; never an ordinary car).
const PATROL: i32 = 5;

/// A synthwave scene is on the screen now (the page shown, not under the screensaver).
fn on_screen(ui: &AppWindow, s: &crate::UiState) -> bool {
    if s.screensaver {
        return false;
    }
    let pages = ui.get_pages();
    let Some(p) = slint::Model::row_data(&pages, ui.get_page().max(0) as usize) else { return false };
    let theme = if p.theme >= 0 { p.theme } else { ui.get_weather().theme };
    (p.kind == "home" && theme == 8) || (p.kind == "timers" && p.variant == 3) || (p.kind == "thermostat" && p.variant == 3)
}

thread_local! {
    /// The patrol car's clock: seconds a synthwave scene was on screen since the last one, and when
    /// the next one is due (a random 20-30 minutes of that).
    static PATROL_CLOCK: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, -1.0)) };
}

/// Counts a second of synthwave on screen (called once a second); true when the patrol car is due.
fn patrol_clock(shown: bool) -> bool {
    PATROL_CLOCK.with(|c| {
        let (mut on, mut due) = c.get();
        if due < 0.0 {
            due = 1200.0 + (random() % 601) as f32;
        }
        if shown && on < due {
            on += 1.0;
        }
        c.set((on, due));
        on >= due
    })
}

/// Development (load test): `n` cars on the road at once, spread out from near to far, the last one
/// the patrol car just behind the one before it on a neighbouring line. With more cars than lines two
/// may share a line (at different depths).
pub fn test_cars(ui: &AppWindow, n: usize) {
    let n = n.clamp(1, 20);
    let now = ui.get_scene_t();
    let lanes = [-2.0f32, 2.0, -1.0, 1.0, 0.0];
    let mut cars: Vec<SwCar> = Vec::new();
    for k in 0..n {
        let dur = 14.0;
        if k == n - 1 && k > 0 {
            let lead = cars[k - 1].clone();
            let line = if lead.line < 2.0 { lead.line + 1.0 } else { lead.line - 1.0 };
            cars.push(SwCar { line, kind: PATROL, tint: Color::from_rgb_u8(255, 255, 255), born: lead.born + lead.dur * PATROL_BEHIND.0, dur: lead.dur, ..Default::default() });
            break;
        }
        let t = CAR_TINTS[next_tint()];
        cars.push(SwCar {
            line: lanes[k % lanes.len()],
            kind: (random() % PATROL as u64) as i32,
            tint: Color::from_rgb_u8((t >> 16) as u8, (t >> 8) as u8, t as u8),
            // the first is the farthest: born earliest
            born: now - (n - 1 - k) as f32 * 1.1,
            dur,
            ..Default::default()
        });
    }
    cars.sort_by(|a, b| a.born.total_cmp(&b.born));
    set_cars(ui, cars);
}

/// Development: the patrol car is due now (it joins the next ordinary car).
pub fn patrol_now() {
    PATROL_CLOCK.with(|c| c.set((1.0, 0.0)));
}

/// The patrol car set off: the next one comes after another 20-30 minutes of synthwave on screen.
fn patrol_went() {
    PATROL_CLOCK.with(|c| c.set((0.0, 1200.0 + (random() % 601) as f32)));
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
    // beach has a dawn view for the morning: it comes in over the night view while the
    // sun rises from -12° to -6° (never at solar midnight, never in the evening).
    let sun = s.states.get("sun.sun");
    let el = sun.and_then(|e| e.attrs.get("elevation")).and_then(Value::as_f64).unwrap_or(30.0);
    let rising = sun.and_then(|e| e.attrs.get("rising")).and_then(Value::as_bool).unwrap_or(false);
    let night_view = "night";
    let dawn = if scene == 2 && rising { ((el + 12.0) / 6.0).clamp(0.0, 1.0) as f32 } else { 0.0 };
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
        // the beach's dawn view (the other scenes have none)
        ui.set_home_fl_back_c(if scene == 2 { pic("dawn", false) } else { slint::Image::default() });
        ui.set_home_fl_floor_c(if scene == 2 { pic("dawn", true) } else { slint::Image::default() });
        ui.set_timers_sw_scene(scene as i32);
        ui.set_timers_sw_back(crate::quick::picture(&format!("timers/sw/scene-{scene}-back.png")));
        ui.set_timers_sw_floor(crate::quick::picture(&format!("timers/sw/scene-{scene}-floor.png")));
    }
    if (ui.get_home_fl_mix() - mix).abs() > 0.004 {
        ui.set_home_fl_mix(mix);
    }
    if (ui.get_home_fl_mix_c() - dawn).abs() > 0.004 || (dawn == 0.0 && ui.get_home_fl_mix_c() != 0.0) {
        ui.set_home_fl_mix_c(dawn);
    }
    // the view's colour: day, night and (the beach in the morning) dawn, mixed like the pictures
    let lerp_c = |a: Color, b: Color, k: f32| {
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
        Color::from_rgb_u8(l(a.red(), b.red()), l(a.green(), b.green()), l(a.blue(), b.blue()))
    };
    let g = grid(scene, "day");
    let g = if mix > 0.0 {
        let n = grid(scene, night_view);
        let n = if dawn > 0.0 { lerp_c(n, grid(scene, "dawn"), dawn) } else { n };
        lerp_c(g, n, mix)
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
        for i in 0..slint::Model::row_count(&cars) {
            if let Some(mut c) = slint::Model::row_data(&cars, i) {
                c.born -= shift;
                slint::Model::set_row_data(&cars, i, c);
            }
        }
        ui.set_palm_gust_at(ui.get_palm_gust_at() - shift);
    }
    // The palms' gusts: the next one 6-14 s after the last started, 1.6-2.0x the calm sway, held 1-3 s;
    // between gusts the calm is 0.6-0.8x.
    let st = ui.get_scene_t();
    let at = ui.get_palm_gust_at();
    // (scheduled once the right palm, 0.22 s behind, has settled too)
    if st > at + 3.8 + ui.get_palm_gust_hold() {
        let next = st + 6.0 + (random() % 800) as f32 / 100.0;
        ui.set_palm_gust_at(next);
        ui.set_palm_gust_peak(1.6 + (random() % 40) as f32 / 100.0);
        ui.set_palm_gust_hold(1.0 + (random() % 200) as f32 / 100.0);
        ui.set_palm_calm_before(ui.get_palm_calm());
        ui.set_palm_calm(0.6 + (random() % 20) as f32 / 100.0);
    }
    // The cars: each on its own line (never two on one), random kind and vivid colour, a random 8–18 s
    // to the horizon, so their speeds differ. A faster one may overtake a slower one only where the two
    // never touch (see clear_of); else it is made a little slower. The traffic setting (the
    // integration's synthwave page) only sets how often a new one starts: light = one at a time (a new
    // one when the last is gone), more = up to 2, busy = up to 3, a new one once the newest is a third /
    // a fifth of its way.
    let traffic = crate::page_option(s, "home", "traffic").and_then(|v| v.as_u64()).unwrap_or(0).min(2) as usize;
    let now = ui.get_scene_t();
    let all: Vec<SwCar> = (0..slint::Model::row_count(&cars)).filter_map(|i| slint::Model::row_data(&cars, i)).collect();
    // a shoved car lost up to 0.64 s of its trip (spec 25.H5)
    let alive: Vec<SwCar> = all.iter().filter(|c| now - c.born <= c.dur + 0.5 + if c.pushed { 0.7 } else { 0.0 }).cloned().collect();
    // the patrol car rides along with another one: the traffic rules count the ordinary cars
    let ordinary: Vec<&SwCar> = alive.iter().filter(|c| c.kind != PATROL).collect();
    let most = [1usize, 2, 3][traffic];
    let gap = [1.0f32, 0.34, 0.2][traffic];
    let newest = ordinary.iter().max_by(|a, b| a.born.total_cmp(&b.born));
    let room = ordinary.len() < most && newest.is_none_or(|c| traffic > 0 && (now - c.born) / c.dur >= gap);
    let patrol_due = patrol_clock(on_screen(ui, s));
    let mut changed = alive.len() != all.len();
    let mut next = alive.clone();
    if room {
        let last_line = all.iter().max_by(|a, b| a.born.total_cmp(&b.born)).map(|c| c.line).unwrap_or(9.0);
        let lines = [-2.0f32, -1.0, 0.0, 1.0, 2.0];
        let choices: Vec<f32> = lines.iter().copied().filter(|l| *l != last_line && !alive.iter().any(|c| c.line == *l)).collect();
        // a random trip; a faster car may overtake a slower one only where the two would not touch
        // (lanes far enough apart at the depth where it passes); otherwise it is made just slower than
        // that one, and when that would need a trip over the longest, it waits
        let line = if choices.is_empty() { 9.0 } else { choices[(random() % choices.len() as u64) as usize] };
        let want = CAR_TRIP.0 + (random() % 1001) as f32 / 1000.0 * (CAR_TRIP.1 - CAR_TRIP.0);
        let trip = fitting_trip(line, now, want, &alive);
        if let (false, Some(dur)) = (choices.is_empty(), trip) {
            let t = CAR_TINTS[next_tint()];
            // at most two cars of one shape at a time (the patrol car never comes as an ordinary one)
            let kinds: Vec<i32> = (0..PATROL).filter(|k| alive.iter().filter(|c| c.kind == *k).count() < 2).collect();
            next.push(SwCar {
                line,
                kind: kinds[(random() % kinds.len() as u64) as usize],
                tint: Color::from_rgb_u8((t >> 16) as u8, (t >> 8) as u8, t as u8),
                born: now,
                dur,
                ..Default::default()
            });
            // the patrol car, when due: just behind this one (15-18 % of their trip later, the same trip, so
            // the same pace) on a neighbouring free line; never two at once
            if patrol_due && !alive.iter().any(|c| c.kind == PATROL) {
                let born = now + dur * (PATROL_BEHIND.0 + (random() % 101) as f32 / 100.0 * (PATROL_BEHIND.1 - PATROL_BEHIND.0));
                // a free neighbouring line where, at the leader's pace, it touches no other car
                let side: Vec<f32> = [line - 1.0, line + 1.0]
                    .into_iter()
                    .filter(|l| l.abs() <= 2.0 && !alive.iter().any(|c| c.line == *l))
                    .filter(|l| alive.iter().all(|c| clear_of(*l, born, dur, c)))
                    .collect();
                if !side.is_empty() {
                    next.push(SwCar {
                        line: side[(random() % side.len() as u64) as usize],
                        kind: PATROL,
                        tint: Color::from_rgb_u8(255, 255, 255),
                        born,
                        dur,
                        ..Default::default()
                    });
                    patrol_went();
                }
            }
            changed = true;
        }
    }
    if changed {
        // far ones first (the newest is the nearest: drawn last, on top)
        next.sort_by(|a, b| a.born.total_cmp(&b.born));
        set_cars(ui, next);
    }
}

/// The cars on the road, far ones first. Row by row in the one shared model: a car that leaves or
/// comes only takes or adds its own row, so the scene does not build every car anew (a whole new
/// model cost a 30-80 ms frame each time one car left).
fn set_cars(ui: &AppWindow, next: Vec<SwCar>) {
    use slint::Model;
    let cars = ui.get_home_fl_cars();
    let Some(vec) = cars.as_any().downcast_ref::<VecModel<SwCar>>() else {
        let model = ModelRc::new(VecModel::from(next));
        ui.set_home_fl_cars(model.clone());
        ui.set_timers_sw_cars(model);
        return;
    };
    // the cars that went (by their birth: each car has its own), then the new ones in their place
    let mut i = 0;
    while i < vec.row_count() {
        let keep = vec.row_data(i).is_some_and(|c| next.iter().any(|n| n.born == c.born && n.line == c.line));
        if keep {
            i += 1;
        } else {
            vec.remove(i);
        }
    }
    for (k, c) in next.into_iter().enumerate() {
        let here = vec.row_data(k).is_some_and(|r| r.born == c.born && r.line == c.line);
        if !here {
            vec.insert(k.min(vec.row_count()), c);
        }
    }
}

thread_local! {
    /// When a tap last asked for a car (spec 25.H4: at most one every 3 s).
    static ASKED: std::cell::Cell<f32> = const { std::cell::Cell::new(-100.0) };
}

/// Taps on the synthwave home background (spec 25.H4 / H5): `synth.push` "born:side" shoves that car (a fresh shove while
/// it skids), `synth.lane` x asks for a car in the lane under the tap.
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    use slint::Model;
    let cars = ui.get_home_fl_cars();
    let Some(vec) = cars.as_any().downcast_ref::<VecModel<SwCar>>() else {
        return;
    };
    let now = ui.get_scene_t();
    match what {
        "synth.push" => {
            let mut it = value.split(':');
            let born: f32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(-1.0);
            let dir: f32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(1.0);
            for i in 0..vec.row_count() {
                if let Some(mut c) = vec.row_data(i) {
                    if (c.born - born).abs() < 0.001 {
                        c.pushed = true;
                        c.push_t = now;
                        c.push_dir = if dir < 0.0 { -1.0 } else { 1.0 };
                        vec.set_row_data(i, c);
                        eprintln!("nspanel-app: synthwave car shoved");
                    }
                }
            }
        }
        "synth.lane" => {
            if now - ASKED.with(|a| a.get()) < 3.0 {
                return;
            }
            let x: f32 = value.parse().unwrap_or(240.0);
            let line = ((x - 240.0) / 46.0).round().clamp(-2.0, 2.0);
            let all: Vec<SwCar> = (0..vec.row_count()).filter_map(|i| vec.row_data(i)).collect();
            let alive: Vec<SwCar> = all.iter().filter(|c| now - c.born <= c.dur + 0.5).cloned().collect();
            if alive.iter().any(|c| c.line == line) {
                return;
            }
            let want = CAR_TRIP.0 + (random() % 1001) as f32 / 1000.0 * (CAR_TRIP.1 - CAR_TRIP.0);
            let Some(dur) = fitting_trip(line, now, want, &alive) else {
                return;
            };
            ASKED.with(|a| a.set(now));
            let t = CAR_TINTS[next_tint()];
            let mut next = alive;
            next.push(SwCar {
                line,
                kind: (random() % PATROL as u64) as i32,
                tint: Color::from_rgb_u8((t >> 16) as u8, (t >> 8) as u8, t as u8),
                born: now,
                dur,
                ..Default::default()
            });
            next.sort_by(|a, b| a.born.total_cmp(&b.born));
            set_cars(ui, next);
            eprintln!("nspanel-app: synthwave car asked for (lane {line})");
        }
        _ => {}
    }
}
